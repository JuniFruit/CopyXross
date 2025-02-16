use std::ffi::OsString;

use crate::debug_println;

use super::{Clipboard, ClipboardData, ClipboardError};
use winapi::shared::minwindef::UINT;
use winapi::shared::ntdef::{FALSE, NULL};
use winapi::shared::windef::HWND__;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winnt::HANDLE;
use winapi::um::winuser::{
    CloseClipboard, EnumClipboardFormats, GetClipboardData, IsClipboardFormatAvailable,
    OpenClipboard, CF_DIB, CF_HDROP, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT,
};

pub struct WindowsClipboard;

impl WindowsClipboard {
    fn open() -> Result<(), ClipboardError> {
        unsafe {
            let clipboard = OpenClipboard(NULL as *mut HWND__);

            if clipboard == FALSE.into() {
                return Err(ClipboardError::Init("Failed to open clipboard".to_string()));
            }
            Ok(())
        }
    }
    fn close() -> Result<(), ClipboardError> {
        unsafe {
            let success = CloseClipboard();
            if success == FALSE.into() {
                return Err(ClipboardError::Init(
                    ("Failed to close clipboard".to_string()),
                ));
            }
            Ok(())
        }
    }
    fn get_clipboard_data_handle(clipboard_type: UINT) -> Result<HANDLE, ClipboardError> {
        unsafe {
            let handle: HANDLE = GetClipboardData(clipboard_type);
            if handle.is_null() {
                let last_err = GetLastError();
                return Err(ClipboardError::Read(format!(
                    "Failed to get clipboard data. Err: {}",
                    last_err
                )));
            }
            Ok(handle)
        }
    }

    fn read_text() -> Result<ClipboardData, ClipboardError> {
        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT) == FALSE.into() {
                return Err(ClipboardError::Read(
                    "Clipboard does not contain text".to_string(),
                ));
            }

            let handle: HANDLE = WindowsClipboard::get_clipboard_data_handle(CF_UNICODETEXT)?;

            let text_ptr: *const u16 = handle as *const u16;
            let mut len = 0;
            while *text_ptr.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(text_ptr, len);

            debug_println!("Text: {}", String::from_utf16_lossy(slice));

            Ok(ClipboardData::String(vec![]))
        }
    }

    fn read_file() -> Result<ClipboardData, ClipboardError> {
        todo!()
    }
}
#[allow(non_snake_case)]
impl Clipboard for WindowsClipboard {
    fn init() -> Result<Self, ClipboardError> {
        Ok(WindowsClipboard)
    }
    fn write(&self, data: ClipboardData) -> Result<(), ClipboardError> {
        todo!()
    }
    fn read(&self) -> Result<ClipboardData, ClipboardError> {
        unsafe {
            // Open clipboard (NULL or GetDesktopWindow())
            WindowsClipboard::open()?;

            let mut format = 0;
            let mut latest_format = 0;

            // Find the latest available clipboard format
            while {
                format = EnumClipboardFormats(format);
                format != 0
            } {
                latest_format = format;
            }

            if latest_format == 0 {
                WindowsClipboard::close()?;
                return Err(ClipboardError::Read(
                    "No clipboard formats found".to_string(),
                ));
            }

            // Handle different clipboard formats
            let result = match latest_format {
                CF_UNICODETEXT => Self::read_text(),
                CF_OEMTEXT => Self::read_text(),
                CF_TEXT => Self::read_text(),
                CF_HDROP => Self::read_file(),
                CF_DIB => Err(ClipboardError::Read(
                    "Clipboard contains an image (DIB)".to_string(),
                )),
                _ => Err(ClipboardError::Read(format!(
                    "Unsupported format: {}",
                    latest_format
                ))),
            };

            WindowsClipboard::close()?;
            result
        }
    }
}
