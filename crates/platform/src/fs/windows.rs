use super::{Creation, FileIdentity};
use std::os::windows::process::CommandExt;
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::fs::OpenOptionsExt,
    path::Path,
    sync::OnceLock,
};
use windows_permissions::{
    LocalBox, SecurityDescriptor,
    constants::{AceFlags, AceType, SeObjectType, SecurityInformation},
    wrappers::{GetSecurityInfo, SetSecurityInfo},
};
use windows_sys::Win32::{
    Foundation::{GENERIC_ALL, GENERIC_READ, GENERIC_WRITE},
    Storage::FileSystem::{
        DELETE, FILE_APPEND_DATA, FILE_DELETE_CHILD, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
        FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
    },
};

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Path must be private to the current Windows user",
    )
}

fn user_sid() -> io::Result<String> {
    static SID: OnceLock<String> = OnceLock::new();
    if let Some(value) = SID.get() {
        return Ok(value.clone());
    }
    let program = std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or_else(denied)?)
        .join("System32/whoami.exe");
    let output = std::process::Command::new(program)
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .output()?; // CREATE_NO_WINDOW
    let text = String::from_utf8_lossy(&output.stdout);
    let sid = text
        .split('"')
        .find(|part| {
            part.starts_with("S-1-")
                && part
                    .bytes()
                    .all(|b| b == b'S' || b == b'-' || b.is_ascii_digit())
        })
        .filter(|_| output.status.success())
        .ok_or_else(denied)?
        .to_owned();
    let _ = SID.set(sid.clone());
    Ok(sid)
}

fn handle(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if super::is_link(&file.metadata()?) {
        return Err(denied());
    }
    Ok(file)
}

enum Access {
    Private,
    Ancestor,
}
fn check(file: &File, access: Access) -> io::Result<()> {
    if super::is_link(&file.metadata()?) {
        return Err(denied());
    }
    let sid = user_sid()?;
    let descriptor = GetSecurityInfo(
        file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )?;
    if descriptor
        .owner()
        .is_none_or(|owner| owner.to_string() != sid && owner.to_string() != "S-1-5-32-544")
    {
        return Err(denied());
    }
    let acl = descriptor.dacl().ok_or_else(denied)?;
    for index in 0..acl.len() {
        let ace = acl.get_ace(index).ok_or_else(denied)?;
        if matches!(
            ace.ace_type(),
            AceType::ACCESS_ALLOWED_ACE_TYPE
                | AceType::ACCESS_ALLOWED_OBJECT_ACE_TYPE
                | AceType::ACCESS_ALLOWED_CALLBACK_ACE_TYPE
                | AceType::ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE
        ) {
            let target = ace.sid().ok_or_else(denied)?.to_string();
            if target == "S-1-3-0" && ace.flags().contains(AceFlags::InheritOnly) {
                continue;
            }
            if target != sid && target != "S-1-5-18" && target != "S-1-5-32-544" {
                let writable = GENERIC_WRITE
                    | GENERIC_ALL
                    | WRITE_DAC
                    | WRITE_OWNER
                    | DELETE
                    | FILE_WRITE_DATA
                    | FILE_APPEND_DATA
                    | FILE_WRITE_EA
                    | FILE_WRITE_ATTRIBUTES
                    | FILE_DELETE_CHILD;
                if matches!(access, Access::Private) || ace.mask().bits() & writable != 0 {
                    return Err(denied());
                }
            }
        }
    }
    Ok(())
}

pub fn protect(file: &File) -> io::Result<()> {
    let sid = user_sid()?;
    let descriptor: LocalBox<SecurityDescriptor> =
        format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)").parse()?;
    let mut writable = file.try_clone()?;
    SetSecurityInfo(
        &mut writable,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        descriptor.dacl(),
        None,
    )?;
    check(file, Access::Private)
}

pub fn private_dir(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(denied());
            }
            return check_private(path);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = path.parent().ok_or_else(denied)?;
    if !parent.exists() {
        private_dir(parent)?;
    } else {
        // Readable ancestors are safe when other users cannot replace the child.
        check(&handle(parent)?, Access::Ancestor)?;
    }
    match fs::create_dir(path) {
        Ok(()) => {
            let file = OpenOptions::new()
                .access_mode(GENERIC_READ | WRITE_DAC)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)?;
            protect(&file)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => check_private(path),
        Err(error) => Err(error),
    }
}

pub fn check_private(path: &Path) -> io::Result<()> {
    check(&handle(path)?, Access::Private)
}

pub fn open_private(path: &Path, creation: Creation) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .access_mode(GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    if !matches!(creation, Creation::Existing) {
        match options.create_new(true).open(path) {
            Ok(file) => {
                protect(&file)?;
                return Ok(file);
            }
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists
                    && matches!(creation, Creation::IfMissing) => {}
            Err(error) => return Err(error),
        }
        options.create_new(false);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(denied());
    }
    check(&file, Access::Private)?;
    Ok(file)
}

pub fn identity(file: &File) -> io::Result<FileIdentity> {
    let info = winapi_util::file::information(file)?;
    Ok(FileIdentity {
        device: info.volume_serial_number(),
        inode: info.file_index(),
    })
}

pub fn mode(metadata: &fs::Metadata) -> u32 {
    if metadata.is_dir() {
        0o755
    } else if metadata.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

pub fn set_mode(file: &File, mode: u32) -> io::Result<()> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_readonly(mode & 0o200 == 0);
    file.set_permissions(permissions)
}
