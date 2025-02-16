use super::{Clipboard, ClipboardData, ClipboardError};

struct WindowsClipboard;

impl Clipboard for WindowsClipboard {
    fn init() -> Result<Self, ClipboardError> {
        Ok(WindowsClipboard)
    }
    fn write(&self, data: ClipboardData) -> Result<(), ClipboardError> {
        todo!()
    }
    fn read(&self) -> Result<ClipboardData, ClipboardError> {
        todo!()
        // unsafe {
        //     // Open clipboard (NULL or GetDesktopWindow())
        //     if OpenClipboard(GetDesktopWindow()).is_err() {
        //         return Err(ClipboardError::Read("Failed to open clipboard".to_string()));
        //     }
        //
        //     let mut format = 0;
        //     let mut latest_format = 0;
        //
        //     // Find the latest available clipboard format
        //     while {
        //         format = EnumClipboardFormats(format);
        //         format != 0
        //     } {
        //         latest_format = format;
        //     }
        //
        //     if latest_format == 0 {
        //         CloseClipboard();
        //         return Err(ClipboardError::Read(
        //             "No clipboard formats found".to_string(),
        //         ));
        //     }
        //
        //     // Handle different clipboard formats
        //     let result = match latest_format {
        //         CF_UNICODETEXT => Self::read_text(),
        //         CF_HDROP => Self::read_files(),
        //         CF_DIB => Err(ClipboardError::InitFailed(
        //             "Clipboard contains an image (DIB)".to_string(),
        //         )),
        //         _ => Err(ClipboardError::Read(format!(
        //             "Unsupported format: {}",
        //             latest_format
        //         ))),
        //     };
        //
        //     CloseClipboard();
        //     result
        // }
    }
}
