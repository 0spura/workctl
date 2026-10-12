use std::path::Path;

use rusqlite::Connection;

use crate::domain::AppError;

#[cfg(unix)]
use std::{
    fs::{File, Metadata, OpenOptions, Permissions},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
};

#[cfg(unix)]
const DATABASE_OPEN_FLAGS: rusqlite::OpenFlags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
    .union(rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX)
    .union(rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW);

#[cfg(not(any(unix, windows)))]
pub(crate) fn open(_path: &Path) -> Result<Connection, AppError> {
    Err(AppError::config(
        "private mutation event outbox is unavailable on this platform",
    ))
}

#[cfg(windows)]
pub(crate) fn open(path: &Path) -> Result<Connection, AppError> {
    use std::{ffi::OsString, fs::File};

    let parent = path
        .parent()
        .ok_or_else(|| AppError::config("mutation event outbox path is invalid"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::config("mutation event outbox path is invalid"))?;
    let _directory = super::private_windows::PrivateDirectory::open_or_create(parent)
        .map_err(|_| AppError::config("could not secure mutation event outbox"))?;
    let database_file = super::private_windows::create_private_file(path)
        .map_err(|_| AppError::config("could not open mutation event outbox"))?;
    let _sidecar_guards = ["-wal", "-shm", "-journal"]
        .map(|suffix| {
            let mut name = OsString::from(file_name);
            name.push(suffix);
            parent.join(name)
        })
        .into_iter()
        .map(|path| {
            super::private_windows::validate_optional_sidecar_file(&path)
                .map_err(|_| AppError::config("mutation event outbox sidecar is unsafe"))
        })
        .collect::<Result<Vec<Option<File>>, AppError>>()?;
    drop(_sidecar_guards);

    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        .union(rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX);
    let connection = Connection::open_with_flags(path, flags)
        .map_err(|_| AppError::config("could not open mutation event outbox"))?;
    let mut sqlite_handle: *mut std::ffi::c_void = std::ptr::null_mut();
    let result = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            connection.handle(),
            b"main\0".as_ptr().cast(),
            rusqlite::ffi::SQLITE_FCNTL_WIN32_GET_HANDLE,
            (&mut sqlite_handle as *mut *mut std::ffi::c_void).cast(),
        )
    };
    if result != rusqlite::ffi::SQLITE_OK
        || sqlite_handle.is_null()
        || !super::private_windows::same_file_handle(&database_file, sqlite_handle)
            .map_err(|_| AppError::config("could not verify mutation event outbox"))?
    {
        return Err(AppError::config("mutation event outbox handle changed"));
    }
    connection
        .busy_timeout(std::time::Duration::from_millis(100))
        .map_err(|_| AppError::config("could not initialize mutation event outbox"))?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=FULL;
         CREATE TABLE IF NOT EXISTS events (
             id TEXT PRIMARY KEY NOT NULL,
             payload TEXT NOT NULL,
             created_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS deliveries (
             event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
             endpoint_id TEXT NOT NULL,
             delivered INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (event_id, endpoint_id)
         );
         CREATE INDEX IF NOT EXISTS deliveries_pending ON deliveries(endpoint_id, delivered, event_id);
         CREATE TABLE IF NOT EXISTS workflow_actions (
             id TEXT PRIMARY KEY NOT NULL,
             event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
             ordinal INTEGER NOT NULL,
             provider TEXT NOT NULL,
             repository TEXT NOT NULL,
             scope TEXT NOT NULL DEFAULT '',
             approval_fingerprint TEXT NOT NULL DEFAULT '',
             target_resource TEXT NOT NULL,
             target_id TEXT NOT NULL,
             action TEXT NOT NULL,
             state TEXT NOT NULL DEFAULT 'pending'
                 CHECK (state IN ('pending', 'running', 'completed', 'failed', 'uncertain')),
             attempts INTEGER NOT NULL DEFAULT 0,
             safe_error TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             UNIQUE (event_id, ordinal)
         );
         CREATE INDEX IF NOT EXISTS workflow_actions_pending
             ON workflow_actions(state, created_at, event_id, ordinal);
         CREATE TABLE IF NOT EXISTS workflow_pr_references (
             source TEXT NOT NULL,
             subject TEXT NOT NULL,
             repository TEXT NOT NULL,
             number INTEGER NOT NULL CHECK (number > 0),
             PRIMARY KEY (source, subject, repository, number)
         );
         CREATE TABLE IF NOT EXISTS work_tracking_sessions (
             id TEXT PRIMARY KEY NOT NULL,
             scope TEXT NOT NULL,
             approval_fingerprint TEXT NOT NULL,
             provider TEXT NOT NULL,
             repository TEXT NOT NULL,
             issue_id TEXT NOT NULL,
             worktree TEXT NOT NULL,
             branch TEXT NOT NULL,
             started_at_ms INTEGER NOT NULL,
             finished_at_ms INTEGER,
             active_millis INTEGER NOT NULL DEFAULT 0,
             state TEXT NOT NULL DEFAULT 'active'
                 CHECK (state IN ('active','finishing','completed','uncertain')),
             finish_event_id TEXT
         );
         CREATE INDEX IF NOT EXISTS work_tracking_active
             ON work_tracking_sessions(scope, approval_fingerprint, provider, repository, issue_id, state);
         CREATE TABLE IF NOT EXISTS work_tracking_branches (
             scope TEXT NOT NULL,
             approval_fingerprint TEXT NOT NULL,
             provider TEXT NOT NULL,
             repository TEXT NOT NULL,
             worktree TEXT NOT NULL,
             branch TEXT NOT NULL,
             issue_id TEXT NOT NULL,
             PRIMARY KEY (scope, approval_fingerprint, provider, repository, worktree, branch)
         );
         CREATE TABLE IF NOT EXISTS work_tracking_cursor (
             slot INTEGER PRIMARY KEY CHECK (slot = 1),
             scope TEXT NOT NULL,
             approval_fingerprint TEXT NOT NULL,
             provider TEXT NOT NULL,
             repository TEXT NOT NULL,
             worktree TEXT NOT NULL,
             branch TEXT NOT NULL,
             issue_id TEXT NOT NULL,
             observed_at_ms INTEGER NOT NULL
         );",
    ).map_err(|_| AppError::config("could not initialize mutation event outbox"))?;
    ensure_workflow_action_auth_columns(&connection)?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = OsString::from(file_name);
        name.push(suffix);
        super::private_windows::validate_optional_sidecar_file(&parent.join(name))
            .map_err(|_| AppError::config("mutation event outbox sidecar is unsafe"))?;
    }
    Ok(connection)
}

#[cfg(unix)]
pub(crate) fn open(path: &Path) -> Result<Connection, AppError> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::config("mutation event outbox path is invalid"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::config("mutation event outbox path is invalid"))?;
    let directory = secure_directory(parent)?;
    let parent = std::fs::canonicalize(parent)
        .map_err(|_| AppError::config("could not secure mutation event outbox"))?;
    verify_identity(&parent, &directory)?;
    let path = parent.join(path.file_name().unwrap());
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|_| AppError::config("could not open mutation event outbox"))?;
    let connection = open_database(&path, &file)?;
    verify_identity(&parent, &directory)?;
    connection
        .busy_timeout(std::time::Duration::from_millis(100))
        .map_err(|_| AppError::config("could not initialize mutation event outbox"))?;
    connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS events (
                 id TEXT PRIMARY KEY NOT NULL,
                 payload TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS deliveries (
                 event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                 endpoint_id TEXT NOT NULL,
                 delivered INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (event_id, endpoint_id)
             );
             CREATE INDEX IF NOT EXISTS deliveries_pending ON deliveries(endpoint_id, delivered, event_id);
             CREATE TABLE IF NOT EXISTS workflow_actions (
                 id TEXT PRIMARY KEY NOT NULL,
                 event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                 ordinal INTEGER NOT NULL,
                 provider TEXT NOT NULL,
                 repository TEXT NOT NULL,
                 scope TEXT NOT NULL DEFAULT '',
                 approval_fingerprint TEXT NOT NULL DEFAULT '',
                 target_resource TEXT NOT NULL,
                 target_id TEXT NOT NULL,
                 action TEXT NOT NULL,
                 state TEXT NOT NULL DEFAULT 'pending'
                     CHECK (state IN ('pending', 'running', 'completed', 'failed', 'uncertain')),
                 attempts INTEGER NOT NULL DEFAULT 0,
                 safe_error TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 UNIQUE (event_id, ordinal)
             );
             CREATE INDEX IF NOT EXISTS workflow_actions_pending
                 ON workflow_actions(state, created_at, event_id, ordinal);
             CREATE TABLE IF NOT EXISTS workflow_pr_references (
                 source TEXT NOT NULL,
                 subject TEXT NOT NULL,
                 repository TEXT NOT NULL,
                 number INTEGER NOT NULL CHECK (number > 0),
                 PRIMARY KEY (source, subject, repository, number)
             );
             CREATE TABLE IF NOT EXISTS work_tracking_sessions (
                 id TEXT PRIMARY KEY NOT NULL,
                 scope TEXT NOT NULL,
                 approval_fingerprint TEXT NOT NULL,
                 provider TEXT NOT NULL,
                 repository TEXT NOT NULL,
                 issue_id TEXT NOT NULL,
                 worktree TEXT NOT NULL,
                 branch TEXT NOT NULL,
                 started_at_ms INTEGER NOT NULL,
                 finished_at_ms INTEGER,
                 active_millis INTEGER NOT NULL DEFAULT 0,
                 state TEXT NOT NULL DEFAULT 'active'
                     CHECK (state IN ('active','finishing','completed','uncertain')),
                 finish_event_id TEXT
             );
             CREATE INDEX IF NOT EXISTS work_tracking_active
                 ON work_tracking_sessions(scope, approval_fingerprint, provider, repository, issue_id, state);
             CREATE TABLE IF NOT EXISTS work_tracking_branches (
                 scope TEXT NOT NULL,
                 approval_fingerprint TEXT NOT NULL,
                 provider TEXT NOT NULL,
                 repository TEXT NOT NULL,
                 worktree TEXT NOT NULL,
                 branch TEXT NOT NULL,
                 issue_id TEXT NOT NULL,
                 PRIMARY KEY (scope, approval_fingerprint, provider, repository, worktree, branch)
             );
             CREATE TABLE IF NOT EXISTS work_tracking_cursor (
                 slot INTEGER PRIMARY KEY CHECK (slot = 1),
                 scope TEXT NOT NULL,
                 approval_fingerprint TEXT NOT NULL,
                 provider TEXT NOT NULL,
                 repository TEXT NOT NULL,
                 worktree TEXT NOT NULL,
                 branch TEXT NOT NULL,
                 issue_id TEXT NOT NULL,
                 observed_at_ms INTEGER NOT NULL
             );"
        )
        .map_err(|_| AppError::config("could not initialize mutation event outbox"))?;
    ensure_workflow_action_auth_columns(&connection)?;
    verify_identity(&path, &file)?;
    verify_identity(&parent, &directory)?;
    Ok(connection)
}

#[cfg(unix)]
fn secure_directory(path: &Path) -> Result<File, AppError> {
    // Validate the existing ancestry before creating anything beneath it.
    let ancestor = path
        .ancestors()
        .find(|ancestor| ancestor.exists())
        .ok_or_else(|| AppError::config("mutation event outbox directory is invalid"))?;
    verify_ancestors(ancestor)?;
    let canonical = std::fs::canonicalize(ancestor)
        .map_err(|_| AppError::config("mutation event outbox directory is invalid"))?;
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(&canonical)
        .map_err(|_| AppError::config("mutation event outbox directory is invalid"))?;
    verify_identity(&canonical, &directory)?;
    // Anchor each mkdir/open to the retained parent descriptor, so a missing
    // directory cannot be replaced by a symlink between creation and opening.
    for component in path.strip_prefix(ancestor).unwrap().components() {
        let std::path::Component::Normal(name) = component else {
            return Err(AppError::config("mutation event outbox path is invalid"));
        };
        let name = std::ffi::CString::new(name.as_bytes())
            .map_err(|_| AppError::config("mutation event outbox path is invalid"))?;
        let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
            return Err(AppError::config("could not secure mutation event outbox"));
        }
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(AppError::config(
                "mutation event outbox directory is invalid",
            ));
        }
        // openat returned a new descriptor; this File uniquely owns it.
        directory = unsafe { File::from_raw_fd(fd) };
        let metadata = directory
            .metadata()
            .map_err(|_| AppError::config("could not secure mutation event outbox"))?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(AppError::config(
                "mutation event outbox directory is unsafe",
            ));
        }
    }
    let metadata = directory
        .metadata()
        .map_err(|_| AppError::config("could not secure mutation event outbox"))?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o022 != 0
    {
        return Err(AppError::config(
            "mutation event outbox directory is unsafe",
        ));
    }
    verify_ancestors(path)?;
    verify_identity(path, &directory)?;
    directory
        .set_permissions(Permissions::from_mode(0o700))
        .map_err(|_| AppError::config("could not secure mutation event outbox"))?;
    Ok(directory)
}

#[cfg(unix)]
fn verify_ancestors(path: &Path) -> Result<(), AppError> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| AppError::config("mutation event outbox directory is invalid"))?;
    let uid = unsafe { libc::geteuid() };
    for ancestor in path.ancestors().chain(canonical.ancestors()) {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|_| AppError::config("mutation event outbox directory is invalid"))?;
        // Trusted sticky directories (e.g. /tmp) prevent other users replacing
        // our entries. Ordinary group/world-writable ancestors do not.
        if (metadata.uid() != uid && metadata.uid() != 0)
            || (!metadata.is_dir() && !metadata.file_type().is_symlink())
            || (metadata.is_dir()
                && metadata.mode() & 0o022 != 0
                && metadata.mode() & libc::S_ISVTX as u32 == 0)
        {
            return Err(AppError::config(
                "mutation event outbox directory is unsafe",
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn verify_identity(path: &Path, file: &File) -> Result<Metadata, AppError> {
    let opened = file
        .metadata()
        .map_err(|_| AppError::config("could not inspect mutation event outbox"))?;
    let named = std::fs::symlink_metadata(path)
        .map_err(|_| AppError::config("mutation event outbox path changed"))?;
    if named.file_type().is_symlink() || opened.dev() != named.dev() || opened.ino() != named.ino()
    {
        return Err(AppError::config("mutation event outbox path changed"));
    }
    Ok(opened)
}

#[cfg(unix)]
fn open_database(path: &Path, file: &File) -> Result<Connection, AppError> {
    let metadata = verify_identity(path, file)?;
    if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } || metadata.nlink() != 1
    {
        return Err(AppError::config("mutation event outbox file is unsafe"));
    }
    file.set_permissions(Permissions::from_mode(0o600))
        .map_err(|_| AppError::config("could not secure mutation event outbox"))?;
    // The guard descriptor cannot be passed to SQLite's standard VFS. Keep it
    // alive, prohibit SQLite itself from following links, and check identity
    // before and after its separate open. The private directory is the boundary
    // against other users; this does not defend against a hostile same-UID process.
    let connection = Connection::open_with_flags(path, DATABASE_OPEN_FLAGS)
        .map_err(|_| AppError::config("could not open mutation event outbox"))?;
    verify_identity(path, file)?;
    Ok(connection)
}

fn ensure_workflow_action_auth_columns(connection: &Connection) -> Result<(), AppError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(workflow_actions)")
        .map_err(|_| AppError::config("could not initialize mutation event outbox"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|_| AppError::config("could not initialize mutation event outbox"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::config("could not initialize mutation event outbox"))?;
    for (name, definition) in [
        ("scope", "TEXT NOT NULL DEFAULT ''"),
        ("approval_fingerprint", "TEXT NOT NULL DEFAULT ''"),
    ] {
        if !columns.iter().any(|column| column == name) {
            connection
                .execute_batch(&format!(
                    "ALTER TABLE workflow_actions ADD COLUMN {name} {definition}"
                ))
                .map_err(|_| AppError::config("could not initialize mutation event outbox"))?;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn rf_evt_1_outbox_persists_independent_endpoint_state_across_reopen() {
        let dir = std::env::temp_dir().join(format!("workctl-outbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("events.sqlite3");
        {
            let connection = open(&path).unwrap();
            connection
                .execute(
                    "INSERT INTO events(id,payload,created_at) VALUES (?1,?2,?3)",
                    ("stable-id", r#"{"id":"stable-id"}"#, "2026-01-01T00:00:00Z"),
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO deliveries(event_id,endpoint_id,delivered) VALUES (?1,?2,0)",
                    ("stable-id", "first"),
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO deliveries(event_id,endpoint_id,delivered) VALUES (?1,?2,0)",
                    ("stable-id", "second"),
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE deliveries SET delivered=1 WHERE endpoint_id=?1",
                    ["first"],
                )
                .unwrap();
        }
        let connection = open(&path).unwrap();
        let states = connection
            .prepare("SELECT endpoint_id,delivered FROM deliveries ORDER BY endpoint_id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(states, [("first".into(), true), ("second".into(), false)]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(connection);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rf_evt_1_symlink_swap_after_preopen_leaves_target_unchanged() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let dir =
            std::env::temp_dir().join(format!("workctl-outbox-race-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("events.sqlite3");
        let target = dir.join("unrelated-file");
        std::fs::write(&target, b"must remain unchanged").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let mut options = std::fs::OpenOptions::new();
        use std::os::unix::fs::OpenOptionsExt;
        options
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW);
        let file = options.open(&path).unwrap();
        std::fs::rename(&path, dir.join("original.sqlite3")).unwrap();
        symlink(&target, &path).unwrap();

        assert!(open_database(&path, &file).is_err());
        assert!(Connection::open_with_flags(&path, DATABASE_OPEN_FLAGS).is_err());
        assert!(open(&path).is_err());
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"must remain unchanged");
        drop(file);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_evt_1_outbox_secures_existing_file_and_private_sidecars() {
        let dir =
            std::env::temp_dir().join(format!("workctl-outbox-private-{}", uuid::Uuid::new_v4()));
        let path = dir.join("nested/events.sqlite3");
        let connection = open(&path).unwrap();
        connection
            .execute("INSERT INTO events VALUES ('id','payload','now')", [])
            .unwrap();
        for file in [
            &path,
            &path.with_file_name("events.sqlite3-wal"),
            &path.with_file_name("events.sqlite3-shm"),
        ] {
            assert_eq!(
                std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        drop(connection);
        std::fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();
        let connection = open(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(connection);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_evt_1_outbox_rejects_unsafe_ancestor_before_creating_state() {
        let dir =
            std::env::temp_dir().join(format!("workctl-outbox-unsafe-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, Permissions::from_mode(0o777)).unwrap();
        let path = dir.join("state/workctl/events.sqlite3");
        assert!(open(&path).is_err());
        assert!(!dir.join("state").exists());
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o777
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_evt_1_outbox_rejects_symlink_directory_without_chmodding_target() {
        let dir =
            std::env::temp_dir().join(format!("workctl-outbox-directory-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let target = dir.join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, Permissions::from_mode(0o755)).unwrap();
        let link = dir.join("state");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(open(&link.join("events.sqlite3")).is_err());
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(!target.join("events.sqlite3").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_evt_1_outbox_rejects_hardlink_without_chmodding_target() {
        let dir =
            std::env::temp_dir().join(format!("workctl-outbox-hardlink-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let target = dir.join("target");
        std::fs::write(&target, b"must remain unchanged").unwrap();
        std::fs::set_permissions(&target, Permissions::from_mode(0o644)).unwrap();
        let path = dir.join("events.sqlite3");
        std::fs::hard_link(&target, &path).unwrap();
        assert!(open(&path).is_err());
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"must remain unchanged");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rf_evt_1_outbox_rejects_nonregular_file_without_blocking() {
        let dir =
            std::env::temp_dir().join(format!("workctl-outbox-fifo-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("events.sqlite3");
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(open(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
