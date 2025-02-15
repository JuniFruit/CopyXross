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
    }
}
