use super::Clipboard;
use super::ClipboardData;
use super::ClipboardError;
use crate::debug_println;
use crate::utils::Result as AnyResult;
use objc::class;
use objc::msg_send;
use objc::runtime::Object;
use objc::sel;
use objc::sel_impl;
use std::ffi::CStr;

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

type ObjectId = *mut Object;

pub struct MacosClipboard {
    p: ObjectId,
}

#[allow(unexpected_cfgs)]
impl Clipboard for MacosClipboard {
    fn init() -> Result<Self, ClipboardError> {
        let pb: ObjectId = unsafe {
            let res: ObjectId = msg_send![class!(NSPasteboard), generalPasteboard];
            res
        };
        if pb.is_null() {
            return Err(ClipboardError::Init(String::from(
                "General pasteboard is not returned. Pointer is null",
            )));
        }
        Ok(MacosClipboard { p: pb })
    }
    fn write(&self, data: ClipboardData) -> Result<(), ClipboardError> {
        todo!()
    }
    fn read(&self) -> Result<ClipboardData, ClipboardError> {
        unsafe {
            let pb = self.p;

            // Get the first available type
            let types: *mut Object = msg_send![pb, types];
            if types.is_null() {
                return Err(ClipboardError::Read(
                    "Failed to get pasteboard types".to_string(),
                ));
            }

            let first_type: *mut Object = msg_send![types, firstObject];
            if first_type.is_null() {
                return Err(ClipboardError::Read("Clipboard is empty".to_string()));
            }

            // Convert the type to a Rust string
            let type_utf8: *const i8 = msg_send![first_type, UTF8String];
            let type_cstr = CStr::from_ptr(type_utf8);
            let type_str = type_cstr.to_string_lossy().into_owned();

            // Handle different types
            if type_str == "public.utf8-plain-text" {
                let ns_string: *mut Object = msg_send![pb, stringForType: first_type];
                if ns_string.is_null() {
                    return Err(ClipboardError::Read(
                        "Failed to read text from clipboard".to_string(),
                    ));
                }
                let utf8: *const i8 = msg_send![ns_string, UTF8String];
                let c_str = CStr::from_ptr(utf8);
                debug_println!("Text: {}", c_str.to_string_lossy());
                Ok(ClipboardData::String(
                    c_str.to_string_lossy().as_bytes().to_vec(),
                ))
            } else if type_str == "public.file-url" {
                let ns_url: *mut Object = msg_send![pb, propertyListForType: first_type];
                if ns_url.is_null() {
                    return Err(ClipboardError::Read(
                        "Failed to read file URL from clipboard".to_string(),
                    ));
                }
                let utf8: *const i8 = msg_send![ns_url, UTF8String];
                let c_str = CStr::from_ptr(utf8);
                debug_println!("File: {}", c_str.to_string_lossy());
                let file_buf = read_file(&c_str.to_string_lossy()).map_err(|err| {
                    ClipboardError::Read(format!("Failed to read file from buffer: {:?}", err))
                });
                Ok(ClipboardData::String(
                    c_str.to_string_lossy().as_bytes().to_vec(),
                ))
            } else if type_str.starts_with("public.")
                && (type_str.contains("png")
                    || type_str.contains("jpeg")
                    || type_str.contains("tiff"))
            {
                debug_println!("Image format detected: {}", type_str);
                Ok(ClipboardData::String(type_str.as_bytes().to_vec()))
            } else {
                Err(ClipboardError::Read(format!(
                    "Clipboard contains unsupported type: {}",
                    type_str
                )))
            }
        }
    }
}

fn read_file(path: &str) -> AnyResult<Vec<u8>> {
    todo!()
}

fn read_image() -> AnyResult<Vec<u8>> {
    todo!()
}
