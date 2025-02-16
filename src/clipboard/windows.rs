use super::{Clipboard, ClipboardData, ClipboardError};
use winapi::shared::ntdef::{FALSE, NULL};
use winapi::shared::windef::HWND__;
use winapi::um::winuser::{CloseClipboard, EnumClipboardFormats, OpenClipboard};

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

    fn read_text() -> Result<ClipboardData, ClipboardError> {
        todo!()
    }

    fn read_file() -> Result<ClipboardData, ClipboardError> {
        todo!()
    }
}

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
