//! Read a bounded regular file through one native descriptor without following aliases.
use serde_json::json;
use std::path::Path;
use work_feature::error::{ExitCode, WorkError};
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
fn unavailable() -> WorkError {
    WorkError::new(
        ExitCode::ArtifactIntegrity,
        "command_capture_unverified",
        "A bounded regular evidence file could not be verified.",
        json!({}),
    )
}
fn native_error(operation: &str, error: std::io::Error) -> WorkError {
    WorkError::new(
        ExitCode::IoFailure,
        "command_capture_read_failed",
        "Evidence could not be read; preserve current execution state.",
        json!({"operation":operation,"os_error":error.raw_os_error(),"error":error.to_string()}),
    )
}
use std::fs::{File, Metadata};
use std::io::Read;

#[cfg(target_os = "macos")]
fn open(path: &Path) -> Result<File, WorkError> {
    use std::os::unix::fs::OpenOptionsExt;
    // Darwin O_NOFOLLOW_ANY | O_CLOEXEC. O_NOFOLLOW is mutually exclusive
    // with O_NOFOLLOW_ANY in vn_open_auth. Unsupported enforcement
    // fails here; it never falls back to following a path checked earlier.
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x20000000 | 0x01000000 | 0x4)
        .open(path)
        .map_err(|error| native_error("open alias-free input descriptor", error))
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::io::FromRawHandle;

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *mut u16,
    }
    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root: *mut c_void,
        name: *mut UnicodeString,
        attributes: u32,
        security: *mut c_void,
        quality: *mut c_void,
    }
    #[repr(C)]
    struct IoStatus {
        status: usize,
        information: usize,
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtCreateFile(
            handle: *mut *mut c_void,
            access: u32,
            object: *mut ObjectAttributes,
            status: *mut IoStatus,
            allocation: *const i64,
            attributes: u32,
            sharing: u32,
            disposition: u32,
            options: u32,
            extended: *const c_void,
            extended_length: u32,
        ) -> i32;
    }

    pub(super) fn open(path: &Path) -> Result<File, WorkError> {
        let raw = path.to_str().ok_or_else(unavailable)?;
        let drive = raw.strip_prefix("\\\\?\\").unwrap_or(raw);
        if drive.len() < 3
            || !drive.as_bytes()[0].is_ascii_alphabetic()
            || &drive.as_bytes()[1..3] != b":\\"
        {
            return Err(unavailable());
        }
        let mut buffer: Vec<u16> = format!("\\??\\{drive}").encode_utf16().collect();
        let length = u16::try_from(buffer.len().checked_mul(2).ok_or_else(unavailable)?)
            .map_err(|_| unavailable())?;
        let mut name = UnicodeString {
            length,
            maximum_length: length,
            buffer: buffer.as_mut_ptr(),
        };
        let mut object = ObjectAttributes {
            length: std::mem::size_of::<ObjectAttributes>() as u32,
            root: std::ptr::null_mut(),
            name: &mut name,
            // OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE; applies to the entire path.
            attributes: 0x40 | 0x1000,
            security: std::ptr::null_mut(),
            quality: std::ptr::null_mut(),
        };
        let mut status = IoStatus {
            status: 0,
            information: 0,
        };
        let mut handle = std::ptr::null_mut();
        // SAFETY: all native structures have their documented C layouts and
        // borrow initialized storage for this synchronous FILE_OPEN call.
        let result = unsafe {
            NtCreateFile(
                &mut handle,
                0x120089,
                &mut object,
                &mut status,
                std::ptr::null(),
                0,
                1,
                1,
                0x40 | 0x20,
                std::ptr::null(),
                0,
            )
        };
        if result < 0 || handle.is_null() || handle as isize == -1 {
            return Err(WorkError::new(
                ExitCode::IoFailure,
                "command_capture_open_failed",
                "The alias-free input descriptor could not be opened; no fallback was used.",
                json!({"ntstatus":format!("0x{:08x}", result as u32)}),
            ));
        }
        // SAFETY: successful NtCreateFile transfers ownership of exactly one live handle.
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    pub(super) fn regular(file: &File, tool: bool) -> Result<(), WorkError> {
        let (attributes, links) =
            crate::files::windows_runtime_metadata::descriptor_attributes_and_links(file)
                .map_err(|error| native_error("inspect alias-free input descriptor", error))?;
        if attributes & (0x10 | 0x400) != 0 || links == 0 || (!tool && links != 1) {
            let mut error = unavailable();
            error.details = json!({"stage":"input_descriptor_metadata","constraint":if links == 0 || (!tool && links != 1) { "single_link_regular_file" } else { "alias_free_regular_file" },"link_count":links,"attributes":attributes});
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn open(path: &Path) -> Result<File, WorkError> {
    windows::open(path)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn open(_: &Path) -> Result<File, WorkError> {
    Err(unavailable())
}

fn regular(file: &File, metadata: &Metadata, limit: u64, tool: bool) -> Result<(), WorkError> {
    if !metadata.is_file() || metadata.len() > limit {
        let mut error = unavailable();
        error.details = json!({"stage":"input_descriptor_metadata","constraint":"bounded_regular_file","is_file":metadata.is_file(),"size_bytes":metadata.len(),"maximum_bytes":limit});
        return Err(error);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() == 0 || (!tool && metadata.nlink() != 1) {
            let mut error = unavailable();
            error.details = json!({"stage":"input_descriptor_metadata","constraint":"single_link_regular_file","link_count":metadata.nlink()});
            return Err(error);
        }
    }
    #[cfg(windows)]
    windows::regular(file, tool)?;
    #[cfg(not(windows))]
    let _ = file;
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = tool;
    Ok(())
}

pub(super) fn read(path: &Path, limit: u64) -> Result<Vec<u8>, WorkError> {
    let limit = limit.min(MAX_FILE_BYTES);
    let file = open(path)?;
    let before = file
        .metadata()
        .map_err(|error| native_error("inspect evidence", error))?;
    regular(&file, &before, limit, false)?;
    let mut bytes = Vec::new();
    (&file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| native_error("read evidence", error))?;
    let after = file
        .metadata()
        .map_err(|error| native_error("recheck evidence", error))?;
    regular(&file, &after, limit, false)?;
    unchanged(&before, &after, bytes.len() as u64)?;
    Ok(bytes)
}
fn unchanged(before: &Metadata, after: &Metadata, length: u64) -> Result<(), WorkError> {
    if length != before.len()
        || before.len() != after.len()
        || before.modified().map_err(|_| unavailable())?
            != after.modified().map_err(|_| unavailable())?
    {
        return Err(unavailable());
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        if (
            before.dev(),
            before.ino(),
            before.mode(),
            before.uid(),
            before.gid(),
            before.ctime(),
            before.ctime_nsec(),
        ) != (
            after.dev(),
            after.ino(),
            after.mode(),
            after.uid(),
            after.gid(),
            after.ctime(),
            after.ctime_nsec(),
        ) {
            return Err(unavailable());
        }
    }
    Ok(())
}
