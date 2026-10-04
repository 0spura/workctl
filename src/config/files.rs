use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::config::Provider;
use crate::domain::AppError;

const SHARED_FILE: &str = ".workctl.json";
const LOCAL_FILE: &str = ".workctl.local.json";

const MAX_CONFIG_BYTES: usize = 64 * 1024;

#[derive(Debug, Default)]
pub struct Config {
    pub provider: Option<Provider>,
    pub work_item_provider: Option<Provider>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigFile {
    provider: Option<Provider>,
    work_item_provider: Option<Provider>,
}

pub fn load(root: &Path) -> Result<Config, AppError> {
    let mut config = Config::default();
    merge_file(root.join(SHARED_FILE), &mut config)?;
    merge_file(root.join(LOCAL_FILE), &mut config)?;
    Ok(config)
}

fn merge_file(path: impl AsRef<Path>, config: &mut Config) -> Result<(), AppError> {
    let Some(contents) = read_config(path.as_ref())? else {
        return Ok(());
    };
    let file: ConfigFile = serde_json::from_slice(&contents)
        .map_err(|_| AppError::config("project configuration is invalid"))?;
    if file.provider.is_some() {
        config.provider = file.provider;
    }
    if file.work_item_provider.is_some() {
        config.work_item_provider = file.work_item_provider;
    }
    Ok(())
}

fn read_config(path: &Path) -> Result<Option<Vec<u8>>, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AppError::config("could not inspect project configuration")),
    };
    if !metadata.file_type().is_file() {
        return Err(AppError::config(
            "project configuration must be a regular file",
        ));
    }

    let file =
        open_config(path).map_err(|_| AppError::config("could not read project configuration"))?;
    if !file
        .metadata()
        .map_err(|_| AppError::config("could not inspect project configuration"))?
        .is_file()
    {
        return Err(AppError::config(
            "project configuration must be a regular file",
        ));
    }
    let mut limited = file.take(MAX_CONFIG_BYTES as u64 + 1);
    let mut contents = Vec::new();
    limited
        .read_to_end(&mut contents)
        .map_err(|_| AppError::config("could not read project configuration"))?;
    if contents.len() > MAX_CONFIG_BYTES {
        return Err(AppError::config(
            "project configuration exceeds the size limit",
        ));
    }
    Ok(Some(contents))
}

fn open_config(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::load;
    use crate::config::Provider;

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "workctl-config-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create isolated config root");
            Self(path)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn local_config_overrides_each_shared_field_independently() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"provider":"github","workItemProvider":"github"}"#,
        )
        .expect("write shared config");
        fs::write(
            root.0.join(".workctl.local.json"),
            r#"{"provider":"gitlab"}"#,
        )
        .expect("write local config");

        let config = load(&root.0).expect("load valid config");
        assert_eq!(config.provider, Some(Provider::Gitlab));
        assert_eq!(config.work_item_provider, Some(Provider::Github));
    }

    #[test]
    fn malformed_and_unknown_configuration_fails_closed() {
        let root = TempRoot::new();
        for contents in [r#"{"provider":"invalid"}"#, r#"{"repo":"owner/repo"}"#] {
            fs::write(root.0.join(".workctl.json"), contents).expect("write invalid config");
            assert_eq!(load(&root.0).unwrap_err().code, "config");
        }
    }

    #[test]
    fn rejects_configuration_over_the_fixed_size_limit() {
        let root = TempRoot::new();
        fs::write(
            root.0.join(".workctl.json"),
            vec![b' '; super::MAX_CONFIG_BYTES + 1],
        )
        .expect("write oversized config");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }

    #[test]
    fn rejects_directories_as_configuration_files() {
        let root = TempRoot::new();
        fs::create_dir(root.0.join(".workctl.json")).expect("create invalid config directory");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_configuration_without_reading_its_target() {
        let root = TempRoot::new();
        std::os::unix::fs::symlink("/dev/zero", root.0.join(".workctl.json"))
            .expect("create device symlink");
        assert_eq!(load(&root.0).unwrap_err().code, "config");
    }
}
