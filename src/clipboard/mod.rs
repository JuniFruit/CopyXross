mod macos;
mod windows;
use crate::utils::Filename;

#[derive(Debug)]
pub enum ClipboardError {
    Init(String),
    Read(String),
    Write(String),
}

#[derive(Debug, PartialEq)]
pub enum ClipboardData {
    String(Vec<u8>),
    File((Filename, Vec<u8>)),
}

pub trait Clipboard: Sized {
    fn init() -> Result<Self, ClipboardError>;
    fn write(&self, data: ClipboardData) -> Result<(), ClipboardError>;
    fn read(&self) -> Result<ClipboardData, ClipboardError>;
}

// Conditional imports
#[cfg(target_os = "windows")]
use windows::WindowsClipboard as PlatformClipboard;

#[cfg(target_os = "macos")]
use macos::MacosClipboard as PlatformClipboard;

// Factory function to always return the correct implementation
pub fn new_clipboard() -> Result<impl Clipboard, ClipboardError> {
    PlatformClipboard::init()
}
