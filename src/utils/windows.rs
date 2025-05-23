use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use dirs_next::data_dir;
use winapi::shared::winerror::*;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winbase::GetComputerNameW;

use crate::debug_println;

use super::log_into_file;

#[derive(Debug)]
#[allow(dead_code)]
pub enum WindowsError {
    FileNotFound,
    PathNotFound,
    AccessDenied,
    InvalidParameter,
    OutOfMemory,
    AlreadyExists,
    BadFormat,
    InvalidHandle,
    GeneralFailure,
    SharingViolation,
    WriteProtect,
    DiskFull,
    SemaphoreTimeout,
    InvalidName,
    ModuleNotFound,
    ProcedureNotFound,
    NoMoreItems,
    HandleEOF,
    BufferOverflow,
    InsufficientBuffer,
    BrokenPipe,
    OperationAborted,
    IoPending,
    DirectoryNotEmpty,
    ServiceDoesNotExist,
    ServiceAlreadyRunning,
    ServiceDisabled,
    Unknown(u32),
}

impl WindowsError {
    pub fn from_last_error() -> Self {
        let error_code = unsafe { GetLastError() };
        let err = WindowsError::from(error_code);
        debug_println!("WinErr: {:?}", err);
        err
    }
}

impl From<u32> for WindowsError {
    fn from(error: u32) -> Self {
        match error {
            ERROR_FILE_NOT_FOUND => WindowsError::FileNotFound,
            ERROR_PATH_NOT_FOUND => WindowsError::PathNotFound,
            ERROR_ACCESS_DENIED => WindowsError::AccessDenied,
            ERROR_INVALID_PARAMETER => WindowsError::InvalidParameter,
            ERROR_NOT_ENOUGH_MEMORY => WindowsError::OutOfMemory,
            ERROR_ALREADY_EXISTS => WindowsError::AlreadyExists,
            ERROR_BAD_FORMAT => WindowsError::BadFormat,
            ERROR_INVALID_HANDLE => WindowsError::InvalidHandle,
            ERROR_GEN_FAILURE => WindowsError::GeneralFailure,
            ERROR_SHARING_VIOLATION => WindowsError::SharingViolation,
            ERROR_WRITE_PROTECT => WindowsError::WriteProtect,
            ERROR_DISK_FULL => WindowsError::DiskFull,
            ERROR_SEM_TIMEOUT => WindowsError::SemaphoreTimeout,
            ERROR_INVALID_NAME => WindowsError::InvalidName,
            ERROR_MOD_NOT_FOUND => WindowsError::ModuleNotFound,
            ERROR_PROC_NOT_FOUND => WindowsError::ProcedureNotFound,
            ERROR_NO_MORE_ITEMS => WindowsError::NoMoreItems,
            ERROR_HANDLE_EOF => WindowsError::HandleEOF,
            ERROR_BUFFER_OVERFLOW => WindowsError::BufferOverflow,
            ERROR_INSUFFICIENT_BUFFER => WindowsError::InsufficientBuffer,
            ERROR_BROKEN_PIPE => WindowsError::BrokenPipe,
            ERROR_OPERATION_ABORTED => WindowsError::OperationAborted,
            ERROR_IO_PENDING => WindowsError::IoPending,
            ERROR_DIR_NOT_EMPTY => WindowsError::DirectoryNotEmpty,
            ERROR_SERVICE_DOES_NOT_EXIST => WindowsError::ServiceDoesNotExist,
            ERROR_SERVICE_ALREADY_RUNNING => WindowsError::ServiceAlreadyRunning,
            ERROR_SERVICE_DISABLED => WindowsError::ServiceDisabled,
            _ => WindowsError::Unknown(error),
        }
    }
}

pub fn get_host_name() -> String {
    unsafe {
        let mut buff: [u16; 33] = [0; 33];
        let mut size = (buff.len() * 2) as u32;
        let res = GetComputerNameW(buff.as_mut_ptr(), &mut size);
        if res == 0 {
            let _ = log_into_file(
                format!(
                    "Could not get PC name: {:?}",
                    WindowsError::from_last_error()
                )
                .as_str(),
            );
            return "Unknown WIN Machine".to_string();
        }
        let str = OsString::from_wide(&buff).to_string_lossy().into_owned();
        let str = str.trim_end_matches("\0");
        str.to_string()
    }
}

pub fn get_log_path() -> PathBuf {
    let home = data_dir().unwrap_or(PathBuf::from(""));
    home
}

pub fn get_asset(file: &str) -> PathBuf {
    let mut curr_dir = std::env::current_dir().unwrap_or(PathBuf::from(""));

    curr_dir.push("assets");
    curr_dir.push(file);
    curr_dir
}
