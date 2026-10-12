#![cfg(windows)]

use std::{
    ffi::{OsStr, c_void},
    fs::File,
    io::{self, Read},
    mem::{offset_of, size_of},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
    ptr::{NonNull, null, null_mut},
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_PATH_NOT_FOUND, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        ConvertStringSidToSidW, GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
    },
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION,
        EqualSid, GetAce, GetLengthSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
        GetTokenInformation, INHERIT_ONLY_ACE, INHERITED_ACE, IsValidAcl, IsValidSid,
        NO_PROPAGATE_INHERIT_ACE, OBJECT_INHERIT_ACE, OWNER_SECURITY_INFORMATION, PSID,
        SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ADD_SUBDIRECTORY, FILE_ALL_ACCESS,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO, FILE_EXECUTE, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_ID_INFO, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_READ_EA, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_STANDARD_INFO, FILE_TRAVERSE, FileAttributeTagInfo, FileIdInfo, FileStandardInfo,
        GetDriveTypeW, GetFileInformationByHandleEx, GetVolumeInformationW, OPEN_EXISTING,
    },
    System::{
        SystemServices::{ACCESS_ALLOWED_ACE_TYPE, FILE_PERSISTENT_ACLS},
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

const DRIVE_REMOVABLE_LOCAL: u32 = 2;
const DRIVE_FIXED_LOCAL: u32 = 3;
const DRIVE_RAMDISK_LOCAL: u32 = 6;
#[cfg(test)]
const FILE_DELETE_CHILD_LOCAL: u32 = 0x0040;
#[cfg(test)]
const FILE_ADD_FILE_LOCAL: u32 = 0x0002;
#[cfg(test)]
const FILE_WRITE_ATTRIBUTES_LOCAL: u32 = 0x0100;

const GENERIC_ALL: u32 = 0x1000_0000;
const GENERIC_EXECUTE: u32 = 0x2000_0000;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const READ_CONTROL: u32 = 0x0002_0000;
#[cfg(test)]
const WRITE_DAC: u32 = 0x0004_0000;
#[cfg(test)]
const DELETE: u32 = 0x0001_0000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const VALID_ACE_FLAGS: u8 = (OBJECT_INHERIT_ACE
    | CONTAINER_INHERIT_ACE
    | NO_PROPAGATE_INHERIT_ACE
    | INHERIT_ONLY_ACE
    | INHERITED_ACE) as u8;
const MAX_PATH_UNITS: usize = 32_767;

fn denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut result: Vec<u16> = value.encode_wide().collect();
    if result.contains(&0) || result.len() >= MAX_PATH_UNITS {
        return Err(invalid("Windows private path is invalid"));
    }
    result.push(0);
    Ok(result)
}

fn checked_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() || path.to_str().is_none() {
        return Err(invalid(
            "Windows private path must be an absolute Unicode path",
        ));
    }
    for component in path.components() {
        match component {
            Component::Prefix(prefix) if matches!(prefix.kind(), Prefix::Disk(_)) => {}
            Component::RootDir | Component::Normal(_) => {}
            Component::ParentDir | Component::CurDir | Component::Prefix(_) => {
                return Err(invalid(
                    "Windows private path contains an unsupported component",
                ));
            }
        }
    }
    for component in path.components() {
        if let Component::Normal(name) = component {
            let text = name.to_string_lossy();
            if text.contains(':') || text.ends_with(' ') || text.ends_with('.') {
                return Err(invalid(
                    "Windows alternate streams and aliased names are unsupported",
                ));
            }
            let stem = text.split('.').next().unwrap_or("").to_ascii_uppercase();
            if matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
            {
                return Err(invalid("Windows device paths are unsupported"));
            }
        }
    }
    Ok(path.to_path_buf())
}

fn check_local_acl_volume(path: &Path) -> io::Result<()> {
    let prefix = path
        .components()
        .find_map(|component| match component {
            Component::Prefix(prefix) => Some(prefix),
            _ => None,
        })
        .ok_or_else(|| invalid("Windows private path has no drive"))?;
    let Prefix::Disk(_) = prefix.kind() else {
        return Err(invalid("Windows private storage requires a local drive"));
    };
    let mut root = prefix.as_os_str().to_os_string();
    root.push("\\");
    let root = wide(&root)?;
    // Reject mapped network drives even when their lexical prefix looks like a drive letter.
    let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
    if !matches!(
        drive_type,
        DRIVE_FIXED_LOCAL | DRIVE_REMOVABLE_LOCAL | DRIVE_RAMDISK_LOCAL
    ) {
        return Err(denied("Windows private storage requires a local volume"));
    }
    let mut flags = 0u32;
    if unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            null_mut(),
            0,
        )
    } == 0
        || flags & FILE_PERSISTENT_ACLS == 0
    {
        return Err(denied(
            "Windows private storage requires ACL-capable storage",
        ));
    }
    Ok(())
}

struct LocalMemory(NonNull<c_void>);

impl LocalMemory {
    fn new(pointer: *mut c_void) -> io::Result<Self> {
        NonNull::new(pointer)
            .map(Self)
            .ok_or_else(|| io::Error::other("Windows security API returned no data"))
    }
}

impl Drop for LocalMemory {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0.as_ptr());
        }
    }
}

struct Identity {
    user_storage: Vec<usize>,
    system: LocalMemory,
    admins: LocalMemory,
    trusted_installer: LocalMemory,
}

impl Identity {
    fn current() -> io::Result<Self> {
        let mut raw = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let user_storage = token_user(&token)?;
        let system = parse_sid("S-1-5-18")?;
        let admins = parse_sid("S-1-5-32-544")?;
        let trusted_installer =
            parse_sid("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")?;
        Ok(Self {
            user_storage,
            system,
            admins,
            trusted_installer,
        })
    }

    fn user(&self) -> PSID {
        // SAFETY: TokenUser's SID pointer refers into `user_storage`, which outlives this borrow.
        unsafe { (*self.user_storage.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    fn trusted(&self, sid: PSID) -> bool {
        unsafe {
            EqualSid(sid, self.user()) != 0
                || EqualSid(sid, self.system.0.as_ptr()) != 0
                || EqualSid(sid, self.admins.0.as_ptr()) != 0
                || EqualSid(sid, self.trusted_installer.0.as_ptr()) != 0
        }
    }

    fn private_principal(&self, sid: PSID) -> Option<usize> {
        unsafe {
            if EqualSid(sid, self.user()) != 0 {
                Some(0)
            } else if EqualSid(sid, self.system.0.as_ptr()) != 0 {
                Some(1)
            } else if EqualSid(sid, self.admins.0.as_ptr()) != 0 {
                Some(2)
            } else {
                None
            }
        }
    }

    fn descriptor(&self, directory: bool) -> io::Result<LocalMemory> {
        let mut user_string = null_mut();
        if unsafe { ConvertSidToStringSidW(self.user(), &mut user_string) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let user_string = LocalMemory::new(user_string.cast())?;
        let pointer = user_string.0.as_ptr().cast::<u16>();
        let mut length = 0usize;
        unsafe {
            while length < 256 && *pointer.add(length) != 0 {
                length += 1;
            }
        }
        if length == 256 {
            return Err(invalid("Windows user SID is invalid"));
        }
        let user = String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
            .map_err(|_| invalid("Windows user SID is invalid"))?;
        let inherit = if directory { "OICI" } else { "" };
        let sddl = format!(
            "O:{user}D:P(A;{inherit};FA;;;{user})(A;{inherit};FA;;;SY)(A;{inherit};FA;;;BA)"
        );
        let descriptor = wide(OsStr::new(&sddl))?;
        let mut raw = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor.as_ptr(),
                SDDL_REVISION_1,
                &mut raw,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        LocalMemory::new(raw.cast())
    }
}

fn parse_sid(text: &str) -> io::Result<LocalMemory> {
    let text = wide(OsStr::new(text))?;
    let mut sid = null_mut();
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    LocalMemory::new(sid.cast())
}

fn token_user(token: &OwnedHandle) -> io::Result<Vec<usize>> {
    let mut needed = 0u32;
    let first = unsafe {
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut needed)
    };
    let error = io::Error::last_os_error();
    if first != 0
        || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
        || needed < size_of::<TOKEN_USER>() as u32
        || needed > 16_384
    {
        return Err(io::Error::other("cannot determine Windows user identity"));
    }
    let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            storage.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
        || needed as usize > storage.len() * size_of::<usize>()
        || needed < size_of::<TOKEN_USER>() as u32
    {
        return Err(io::Error::other("cannot read Windows user identity"));
    }
    Ok(storage)
}

fn security_attributes(descriptor: &LocalMemory) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0.as_ptr(),
        bInheritHandle: 0,
    }
}

fn file_from_handle(handle: HANDLE) -> io::Result<File> {
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: callers transfer each successful CreateFileW handle exactly once.
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn open_directory(path: &Path) -> io::Result<File> {
    let name = wide(path.as_os_str())?;
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    let file = file_from_handle(handle)?;
    verify_kind(&file, true)?;
    Ok(file)
}

fn open_file(
    path: &Path,
    access: u32,
    share: u32,
    disposition: u32,
    descriptor: *const SECURITY_ATTRIBUTES,
) -> io::Result<File> {
    let name = wide(path.as_os_str())?;
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            share,
            descriptor,
            disposition,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    file_from_handle(handle)
}

fn verify_kind(file: &File, directory: bool) -> io::Result<()> {
    let mut attributes = FILE_ATTRIBUTE_TAG_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&mut attributes as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (attributes.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
    {
        return Err(denied(
            "Windows private path must not be a reparse point or unexpected file type",
        ));
    }
    Ok(())
}

fn identity_handle(handle: HANDLE) -> io::Result<FILE_ID_INFO> {
    let mut info = FILE_ID_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

pub(crate) fn same_file_handle(file: &File, handle: HANDLE) -> io::Result<bool> {
    let left = identity_handle(file.as_raw_handle())?;
    let right = identity_handle(handle)?;
    Ok(left.VolumeSerialNumber == right.VolumeSerialNumber
        && left.FileId.Identifier == right.FileId.Identifier)
}

fn verify_regular_single_link(file: &File) -> io::Result<()> {
    verify_kind(file, false)?;
    let mut standard = FILE_STANDARD_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStandardInfo,
            (&mut standard as *mut FILE_STANDARD_INFO).cast(),
            size_of::<FILE_STANDARD_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if standard.Directory || standard.DeletePending || standard.NumberOfLinks != 1 {
        return Err(denied(
            "Windows private file is not a single-link regular file",
        ));
    }
    Ok(())
}

struct SecurityInfo {
    _descriptor: LocalMemory,
    owner: PSID,
    dacl: *mut ACL,
    control: u16,
}

fn security_info(file: &File) -> io::Result<SecurityInfo> {
    let mut owner = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let code = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let descriptor = LocalMemory::new(descriptor.cast())?;
    let mut control = 0u16;
    let mut revision = 0u32;
    if unsafe { GetSecurityDescriptorControl(descriptor.0.as_ptr(), &mut control, &mut revision) }
        == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut present = 0;
    let mut dacl_defaulted = 0;
    let mut verified_dacl = null_mut();
    if unsafe {
        GetSecurityDescriptorDacl(
            descriptor.0.as_ptr(),
            &mut present,
            &mut verified_dacl,
            &mut dacl_defaulted,
        )
    } == 0
        || present == 0
        || verified_dacl.is_null()
        || verified_dacl != dacl
        || owner.is_null()
        || unsafe { IsValidSid(owner) } == 0
        || unsafe { IsValidAcl(dacl) } == 0
    {
        return Err(denied(
            "Windows object has no valid discretionary access list",
        ));
    }
    Ok(SecurityInfo {
        _descriptor: descriptor,
        owner,
        dacl,
        control,
    })
}

fn sid_length(sid: PSID) -> io::Result<usize> {
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return Err(denied("Windows access entry has an invalid SID"));
    }
    let length = unsafe { GetLengthSid(sid) } as usize;
    if !(8..=68).contains(&length) {
        return Err(denied("Windows access entry has an invalid SID length"));
    }
    Ok(length)
}

fn ace_details(info: &SecurityInfo, index: u32) -> io::Result<(u8, u32, PSID)> {
    let mut raw = null_mut();
    if unsafe { GetAce(info.dacl, index, &mut raw) } == 0 || raw.is_null() {
        return Err(denied("Windows discretionary access entry is unavailable"));
    }
    let acl_start = info.dacl as usize;
    let acl_size = unsafe { (*info.dacl).AclSize as usize };
    let offset = (raw as usize)
        .checked_sub(acl_start)
        .filter(|offset| *offset <= acl_size)
        .ok_or_else(|| denied("Windows discretionary access entry is outside its ACL"))?;
    if acl_size - offset < size_of::<ACE_HEADER>() {
        return Err(denied("Windows discretionary access entry is truncated"));
    }
    let header = unsafe { &*raw.cast::<ACE_HEADER>() };
    let ace_size = usize::from(header.AceSize);
    let sid_offset = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
    if header.AceType != ACCESS_ALLOWED_ACE_TYPE as u8
        || ace_size < sid_offset + 8
        || ace_size > acl_size - offset
    {
        return Err(denied(
            "Windows discretionary access entry uses an unsupported form",
        ));
    }
    if header.AceFlags & !VALID_ACE_FLAGS != 0 {
        return Err(denied(
            "Windows discretionary access entry uses unsupported flags",
        ));
    }
    let sid_start = unsafe { raw.cast::<u8>().add(sid_offset) };
    let available = ace_size - sid_offset;
    let sub_authorities = unsafe { *sid_start.add(1) } as usize;
    let length = 8usize
        .checked_add(
            sub_authorities
                .checked_mul(4)
                .ok_or_else(|| denied("Windows access entry has an invalid SID length"))?,
        )
        .ok_or_else(|| denied("Windows access entry has an invalid SID length"))?;
    if sub_authorities > 15 || length > available {
        return Err(denied("Windows discretionary access SID exceeds its entry"));
    }
    let sid: PSID = sid_start.cast();
    if sid_length(sid)? != length {
        return Err(denied("Windows access entry has an invalid SID length"));
    }
    let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
    Ok((header.AceFlags, ace.Mask, sid))
}

fn check_private_acl(
    file: &File,
    identity: &Identity,
    directory: bool,
    require_protected: bool,
) -> io::Result<()> {
    let info = security_info(file)?;
    if identity.private_principal(info.owner).is_none() {
        return Err(denied("Windows private object has an untrusted owner"));
    }
    if require_protected && info.control & SE_DACL_PROTECTED == 0 {
        return Err(denied(
            "Windows private object has an unprotected access list",
        ));
    }
    let count = unsafe { (*info.dacl).AceCount };
    if count != 3 {
        return Err(denied("Windows private object access list is not exact"));
    }
    let mut seen = [false; 3];
    for index in 0..u32::from(count) {
        let (flags, mask, sid) = ace_details(&info, index)?;
        let flags_valid = if directory {
            flags == (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
        } else if require_protected {
            flags == 0
        } else {
            let inherited_flags =
                (INHERITED_ACE | OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8;
            flags == 0 || (flags & INHERITED_ACE as u8 != 0 && flags & !inherited_flags == 0)
        };
        if !flags_valid || mask != FILE_ALL_ACCESS {
            return Err(denied(
                "Windows private object access list grants unexpected rights",
            ));
        }
        let Some(principal) = identity.private_principal(sid) else {
            return Err(denied(
                "Windows private object grants access to an untrusted account",
            ));
        };
        if std::mem::replace(&mut seen[principal], true) {
            return Err(denied(
                "Windows private object access list has duplicate principals",
            ));
        }
    }
    if seen.into_iter().all(|present| present) {
        Ok(())
    } else {
        Err(denied("Windows private object access list is incomplete"))
    }
}

fn expand_generic(mask: u32, directory: bool) -> u32 {
    let mut result = mask & !(GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | GENERIC_ALL);
    if mask & GENERIC_READ != 0 {
        result |= READ_CONTROL
            | SYNCHRONIZE
            | FILE_READ_ATTRIBUTES
            | FILE_READ_EA
            | if directory {
                FILE_LIST_DIRECTORY
            } else {
                FILE_READ_DATA
            };
    }
    if mask & GENERIC_EXECUTE != 0 {
        result |= READ_CONTROL | SYNCHRONIZE | FILE_READ_ATTRIBUTES | FILE_EXECUTE;
    }
    if mask & GENERIC_WRITE != 0 {
        result |= 0x0002 | 0x0004 | 0x0010 | 0x0100 | 0x0008;
    }
    if mask & GENERIC_ALL != 0 {
        result |= u32::MAX;
    }
    result
}

fn safe_external_ancestor_rights() -> u32 {
    FILE_READ_DATA
        | FILE_LIST_DIRECTORY
        | FILE_READ_EA
        | FILE_READ_ATTRIBUTES
        | FILE_TRAVERSE
        | FILE_EXECUTE
        | READ_CONTROL
        | SYNCHRONIZE
        | FILE_ADD_SUBDIRECTORY
}

fn external_ancestor_grant_is_safe(mask: u32) -> bool {
    expand_generic(mask, true) & !safe_external_ancestor_rights() == 0
}

fn check_ancestor_acl(file: &File, identity: &Identity) -> io::Result<()> {
    let info = security_info(file)?;
    if !identity.trusted(info.owner) {
        return Err(denied("Windows path ancestor has an untrusted owner"));
    }
    let count = unsafe { (*info.dacl).AceCount };
    for index in 0..u32::from(count) {
        let (flags, mask, sid) = ace_details(&info, index)?;
        if flags & INHERIT_ONLY_ACE as u8 != 0 {
            continue;
        }
        if identity.trusted(sid) {
            continue;
        }
        if !external_ancestor_grant_is_safe(mask) {
            return Err(denied(
                "Windows path ancestor allows unsafe access to another account",
            ));
        }
        if flags & !VALID_ACE_FLAGS != 0 {
            return Err(denied("Windows path ancestor has unsupported access flags"));
        }
    }
    Ok(())
}

fn create_directory(path: &Path, identity: &Identity, descriptor: &LocalMemory) -> io::Result<()> {
    let name = wide(path.as_os_str())?;
    let attributes = security_attributes(descriptor);
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let file = open_directory(path)?;
    verify_kind(&file, true)?;
    check_private_acl(&file, identity, true, true)
}

pub(crate) struct PrivateDirectory {
    _handles: Vec<File>,
}

impl PrivateDirectory {
    pub(crate) fn open_or_create(path: &Path) -> io::Result<Self> {
        Self::walk(path, true)
    }

    fn open_existing(path: &Path) -> io::Result<Self> {
        Self::walk(path, false)
    }

    fn walk(path: &Path, create: bool) -> io::Result<Self> {
        let path = checked_path(path)?;
        check_local_acl_volume(&path)?;
        let identity = Identity::current()?;
        let descriptor = identity.descriptor(true)?;
        let components = path.components().collect::<Vec<_>>();
        let mut current = PathBuf::new();
        let mut handles = Vec::new();
        let mut private_tree = false;
        for (index, component) in components.iter().enumerate() {
            current.push(component.as_os_str());
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            let final_component = index + 1 == components.len();
            let opened = match open_directory(&current) {
                Ok(file) => Some(file),
                Err(error)
                    if create
                        && matches!(error.raw_os_error(), Some(code) if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PATH_NOT_FOUND as i32) =>
                {
                    match create_directory(&current, &identity, &descriptor) {
                        Ok(()) => {
                            private_tree = true;
                            Some(open_directory(&current)?)
                        }
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                            Some(open_directory(&current)?)
                        }
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            };
            let Some(file) = opened else {
                return Err(denied("Windows path component could not be opened"));
            };
            if final_component || private_tree {
                check_private_acl(&file, &identity, true, final_component || private_tree)?;
            } else {
                check_ancestor_acl(&file, &identity)?;
            }
            handles.push(file);
        }
        if handles.is_empty() {
            return Err(invalid("Windows private directory path is empty"));
        }
        Ok(Self { _handles: handles })
    }
}

pub(crate) fn read_private_file(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let path = checked_path(path)?;
    check_local_acl_volume(&path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Windows private file has no parent"))?;
    let _guard = PrivateDirectory::open_existing(parent)?;
    let identity = Identity::current()?;
    let file = open_file(
        &path,
        FILE_GENERIC_READ | READ_CONTROL,
        FILE_SHARE_READ,
        OPEN_EXISTING,
        null(),
    )?;
    verify_regular_single_link(&file)?;
    check_private_acl(&file, &identity, false, true)?;
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(invalid("Windows private file exceeds its size limit"));
    }
    Ok(bytes)
}

pub(crate) fn validate_private_file(path: &Path) -> io::Result<File> {
    let path = checked_path(path)?;
    check_local_acl_volume(&path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Windows private file has no parent"))?;
    let _guard = PrivateDirectory::open_existing(parent)?;
    let identity = Identity::current()?;
    let file = open_file(
        &path,
        READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        null(),
    )?;
    verify_regular_single_link(&file)?;
    check_private_acl(&file, &identity, false, true)?;
    Ok(file)
}

fn validate_sidecar_file(path: &Path) -> io::Result<File> {
    let path = checked_path(path)?;
    check_local_acl_volume(&path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Windows private file has no parent"))?;
    let _guard = PrivateDirectory::open_existing(parent)?;
    let identity = Identity::current()?;
    let file = open_file(
        &path,
        READ_CONTROL | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        null(),
    )?;
    verify_regular_single_link(&file)?;
    check_private_acl(&file, &identity, false, false)?;
    Ok(file)
}

pub(crate) fn validate_optional_sidecar_file(path: &Path) -> io::Result<Option<File>> {
    match validate_sidecar_file(path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if matches!(error.raw_os_error(), Some(code) if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PATH_NOT_FOUND as i32) => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn create_private_file(path: &Path) -> io::Result<File> {
    let path = checked_path(path)?;
    check_local_acl_volume(&path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Windows private file has no parent"))?;
    let _guard = PrivateDirectory::open_or_create(parent)?;
    match create_file_with_private_acl(&path) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => validate_private_file(&path),
        Err(error) => Err(error),
    }
}

pub(crate) fn create_private_file_new(path: &Path) -> io::Result<File> {
    let path = checked_path(path)?;
    check_local_acl_volume(&path)?;
    create_file_with_private_acl(&path)
}

fn create_file_with_private_acl(path: &Path) -> io::Result<File> {
    let identity = Identity::current()?;
    let descriptor = identity.descriptor(false)?;
    let attributes = security_attributes(&descriptor);
    let file = open_file(
        path,
        FILE_GENERIC_READ
            | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE
            | READ_CONTROL,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        CREATE_NEW,
        &attributes,
    )?;
    verify_regular_single_link(&file)?;
    check_private_acl(&file, &identity, false, true)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write, process::Command};

    struct ReadableRoot(PathBuf);

    impl ReadableRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "workctl-readable-approval-{}",
                uuid::Uuid::new_v4()
            ));
            check_local_acl_volume(&path).expect("ACL-capable local fixture volume");
            let identity = Identity::current().expect("fixture identity");
            let mut raw_user = null_mut();
            assert_ne!(
                unsafe { ConvertSidToStringSidW(identity.user(), &mut raw_user) },
                0
            );
            let user_storage = LocalMemory::new(raw_user.cast()).expect("fixture SID storage");
            let pointer = user_storage.0.as_ptr().cast::<u16>();
            let length = (0..256)
                .position(|offset| unsafe { *pointer.add(offset) == 0 })
                .expect("terminated fixture SID");
            let user = String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
                .expect("fixture SID string");
            let sddl = format!(
                "O:{user}D:P(A;OICI;FA;;;{user})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FR;;;WD)"
            );
            let sddl = wide(OsStr::new(&sddl)).expect("fixture security descriptor string");
            let mut raw_descriptor = null_mut();
            assert_ne!(
                unsafe {
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        sddl.as_ptr(),
                        SDDL_REVISION_1,
                        &mut raw_descriptor,
                        null_mut(),
                    )
                },
                0
            );
            let descriptor =
                LocalMemory::new(raw_descriptor.cast()).expect("fixture security descriptor");
            let attributes = security_attributes(&descriptor);
            let name = wide(path.as_os_str()).expect("fixture directory path");
            assert_ne!(unsafe { CreateDirectoryW(name.as_ptr(), &attributes) }, 0);
            let root = Self(path);
            root.assert_readable();
            root
        }

        fn assert_readable(&self) {
            let file = open_directory(&self.0).expect("read fixture directory ACL");
            let info = security_info(&file).expect("fixture directory security");
            let everyone = parse_sid("S-1-1-0").expect("Everyone SID");
            let count = unsafe { (*info.dacl).AceCount };
            assert!((0..u32::from(count)).any(|index| {
                let (flags, mask, sid) = ace_details(&info, index).expect("fixture access entry");
                let is_everyone = unsafe { EqualSid(sid, everyone.0.as_ptr()) != 0 };
                is_everyone
                    && flags & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
                        == (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE) as u8
                    && expand_generic(mask, false) & FILE_READ_DATA != 0
            }));
            let identity = Identity::current().expect("fixture identity");
            assert!(check_private_acl(&file, &identity, true, true).is_err());
        }
    }

    impl Drop for ReadableRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn assert_private_file(path: &Path) {
        let file = open_file(
            path,
            FILE_GENERIC_READ | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            null(),
        )
        .expect("open fixture private file");
        verify_regular_single_link(&file).expect("single-link fixture file");
        let identity = Identity::current().expect("fixture identity");
        check_private_acl(&file, &identity, false, true)
            .expect("protected ACL for user, SYSTEM and Administrators only");
    }

    #[test]
    fn rf_cfg_10_rejects_unc_and_ambiguous_paths() {
        assert!(checked_path(Path::new(r"\\server\share\workctl")).is_err());
        assert!(checked_path(Path::new(r"C:\Users\me\events:token")).is_err());
        assert!(checked_path(Path::new(r"C:\Users\me\CON\events")).is_err());
    }

    #[test]
    fn rf_cfg_10_allows_read_only_ancestor_rights_but_rejects_mutation() {
        let safe =
            READ_CONTROL | SYNCHRONIZE | FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | FILE_TRAVERSE;
        assert_eq!(expand_generic(safe, true) & !safe, 0);
        assert_ne!(expand_generic(safe | WRITE_DAC, true) & !safe, 0);
        assert_ne!(
            expand_generic(safe | DELETE | FILE_DELETE_CHILD_LOCAL, true) & !safe,
            0
        );
        assert_ne!(expand_generic(safe | GENERIC_WRITE, true) & !safe, 0);
    }
    #[test]
    fn rf_cfg_10_rejects_outsider_reparse_mutation_rights() {
        assert!(external_ancestor_grant_is_safe(FILE_ADD_SUBDIRECTORY));
        assert!(!external_ancestor_grant_is_safe(FILE_ADD_FILE_LOCAL));
        assert!(!external_ancestor_grant_is_safe(
            FILE_WRITE_ATTRIBUTES_LOCAL
        ));
    }

    #[test]
    fn rf_wf_1_temporary_credentials_are_private_before_write_and_after_replacement() {
        let root = ReadableRoot::new();
        let path = root.0.join(".workctl.local.json");
        let bytes = br#"{"decisionModel":{"apiKey":"synthetic-approval-key"}}"#;
        let mut original = create_private_file_new(&path).expect("private original config");
        original.write_all(bytes).expect("write synthetic original");
        original.sync_all().expect("sync fixture config");
        drop(original);
        assert_private_file(&path);

        let temporary = root.0.join(".workctl.local.fixture.tmp");
        let mut file = create_private_file_new(&temporary).expect("private temporary config");
        assert_eq!(file.metadata().expect("empty fixture metadata").len(), 0);
        let identity = Identity::current().expect("fixture identity");
        check_private_acl(&file, &identity, false, true)
            .expect("temporary file is protected before credential bytes");
        file.write_all(bytes).expect("write synthetic credentials");
        file.sync_all().expect("sync fixture temporary config");
        check_private_acl(&file, &identity, false, true)
            .expect("writing bytes cannot weaken the private ACL");
        drop(file);

        fs::rename(&temporary, &path).expect("replace fixture local config");
        assert_private_file(&path);
        assert_eq!(fs::read(&path).expect("read synthetic fixture"), bytes);
        assert!(!temporary.exists());
        root.assert_readable();
    }

    #[test]
    fn rf_wf_1_approval_preserves_private_credentials_in_a_readable_worktree() {
        let root = ReadableRoot::new();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&root.0)
                .status()
                .expect("initialize fixture repository")
                .success()
        );
        fs::write(root.0.join(".gitignore"), ".workctl.local.json\n")
            .expect("ignore fixture local config");
        fs::write(
            root.0.join(".workctl.json"),
            r#"{"workflow":{"rules":[{"on":"issueCreated","actions":[{"type":"addLabel","label":"triage"}]}]}}"#,
        )
        .expect("write fixture workflow");
        let path = root.0.join(".workctl.local.json");
        let mut original = create_private_file_new(&path).expect("private original config");
        original
            .write_all(br#"{"decisionModel":{"apiKey":"synthetic-approval-key"}}"#)
            .expect("write synthetic original credential");
        original.sync_all().expect("sync fixture original config");
        drop(original);
        assert_private_file(&path);

        let fingerprint =
            crate::config::approve_workflow(&root.0).expect("approve fixture workflow");
        let local: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read approved fixture config"))
                .expect("parse approved fixture config");
        assert_eq!(local["decisionModel"]["apiKey"], "synthetic-approval-key");
        assert_eq!(local["workflowApproval"], fingerprint);
        assert_private_file(&path);
        root.assert_readable();
        let config = crate::config::load_for_cwd(&root.0).expect("load approved fixture workflow");
        assert!(crate::config::workflow_approval_status(&config, &root.0).unwrap());
    }
}
