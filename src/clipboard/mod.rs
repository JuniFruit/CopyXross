#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
use std::str::FromStr;

use crate::utils::Filename;

#[derive(Debug)]
#[allow(dead_code)]
pub enum ClipboardError {
    Init(String),
    Read(String),
    Write(String),
}

#[derive(Debug, PartialEq)]
pub enum StringType {
    Html,
    Utf8Plain,
}

impl ToString for StringType {
    fn to_string(&self) -> String {
        match self {
            Self::Html => "HTML".to_string(),
            Self::Utf8Plain => "UTF8P".to_string(),
        }
    }
}

impl FromStr for StringType {
    type Err = ClipboardError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "HTML" => Ok(StringType::Html),
            "UTF8P" => Ok(StringType::Utf8Plain),
            _ => Err(ClipboardError::Init(format!("Unknown string type: {}", s))),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ClipboardData {
    String((StringType, Vec<u8>)),
    File((Filename, Vec<u8>)),
}

pub trait Clipboard: Sized + Send + Sync {
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
