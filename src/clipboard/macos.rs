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
use std::ffi::CString;
use std::fs;

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

type ObjectId = *mut Object;

pub struct MacosClipboard {
    p: ObjectId,
}

#[allow(unexpected_cfgs)]
impl MacosClipboard {
    fn read_file(&self, path: &str) -> AnyResult<Vec<u8>> {
        let file = fs::read(path)?;

        Ok(file)
    }

    fn read_image(&self, first_type: *mut Object) -> Result<Vec<u8>, ClipboardError> {
        let pb = self.p;
        unsafe {
            let ns_data: *mut Object = msg_send![pb, dataForType: first_type];
            if ns_data.is_null() {
                return Err(ClipboardError::Read(
                    "Failed to read binary data from clipboard".to_string(),
                ));
            }

            // Get the byte length
            let length: usize = msg_send![ns_data, length];
            let bytes: *const u8 = msg_send![ns_data, bytes];

            if length == 0 || bytes.is_null() {
                return Err(ClipboardError::Read(
                    "Failed to read binary data from clipboard. No data in buffer".to_string(),
                ));
            }

            // Copy the data into a Rust Vec<u8>
            let slice = std::slice::from_raw_parts(bytes, length);
            Ok(slice.to_vec())
        }
    }
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
        let pb = self.p;

        match data {
            ClipboardData::File(file_data) => {
                unsafe {
                    let (filename, data) = file_data;
                    let mime_type_opt = filename.split(".").last();
                    if mime_type_opt.is_none() {
                        return Err(ClipboardError::Write(
                            "Failed to write bytes into clipboard. Unknown mime_type".to_string(),
                        ));
                    }
                    let mime_type = mime_type_opt.unwrap();
                    // Convert Rust byte slice to NSData
                    let ns_data: *mut Object =
                        msg_send![class!(NSData), dataWithBytes: data.as_ptr() length: data.len()];
                    if ns_data.is_null() {
                        return Err(ClipboardError::Write("Failed to create NSData".to_string()));
                    }

                    // Convert MIME type to NSString
                    let c_type = CString::new(mime_type).unwrap();
                    let ns_type: *mut Object =
                        msg_send![class!(NSString), stringWithUTF8String: c_type.as_ptr()];
                    if ns_type.is_null() {
                        return Err(ClipboardError::Write(
                            "Failed to create NSString for MIME type".to_string(),
                        ));
                    }

                    // Clear clipboard
                    let _: () = msg_send![pb, clearContents];

                    // Store binary data into clipboard
                    let success: bool = msg_send![pb, setData: ns_data forType: ns_type];
                    if !success {
                        return Err(ClipboardError::Write(
                            "Failed to write binary data to clipboard".to_string(),
                        ));
                    }

                    debug_println!("Binary data written to clipboard as {}", mime_type);
                }
            }
            ClipboardData::String(data) => {
                unsafe {
                    let text = String::from_utf8(data).map_err(|err| {
                        ClipboardError::Write(format!(
                            "Failed to write string into clipboard: {:?}",
                            err
                        ))
                    })?;
                    // Convert Rust `&str` to `NSString`
                    let c_text = CString::new(text.as_str()).unwrap();
                    let ns_string: *mut Object =
                        msg_send![class!(NSString), stringWithUTF8String: c_text.as_ptr()];

                    if ns_string.is_null() {
                        return Err(ClipboardError::Write(
                            "Failed to create NSString".to_string(),
                        ));
                    }

                    let c_public_text_type =
                        CString::new("public.utf8-plain-text").map_err(|err| {
                            ClipboardError::Write(format!(
                                "Failed to create public text type C string: {:?}",
                                err
                            ))
                        })?;
                    // Define the public text type
                    let utf8_type: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c_public_text_type.as_ptr()];

                    // Clear clipboard before writing
                    let _: () = msg_send![pb, clearContents];

                    // Write the text to the clipboard
                    let success: bool = msg_send![pb, setString: ns_string forType: utf8_type];

                    if !success {
                        return Err(ClipboardError::Write(
                            "Failed to write text to clipboard".to_string(),
                        ));
                    }

                    debug_println!("Text written to clipboard: {}", text);
                }
            }
        }
        Ok(())
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
                let c_str = CStr::from_ptr(utf8).to_string_lossy();
                debug_println!("File: {}", c_str);
                if c_str.ends_with("/") {
                    return Err(ClipboardError::Read(String::from("Cannot copy folders")));
                }
                let file_buf = self.read_file(&c_str).map_err(|err| {
                    ClipboardError::Read(format!("Failed to read file from buffer: {:?}", err))
                })?;
                let filename = c_str.split("/").last().unwrap_or("unknown_file");
                Ok(ClipboardData::File((String::from(filename), file_buf)))
            } else if type_str.starts_with("public.")
                && (type_str.contains("png")
                    || type_str.contains("jpeg")
                    || type_str.contains("tiff"))
            {
                debug_println!("Image format detected: {}", type_str);
                let mime_type = type_str.split(".").last().unwrap_or("png");
                let filename = format!("image.{}", mime_type);
                let buff = self.read_image(first_type)?;
                Ok(ClipboardData::File((filename, buff)))
            } else {
                Err(ClipboardError::Read(format!(
                    "Clipboard contains unsupported type: {}",
                    type_str
                )))
            }
        }
    }
}
