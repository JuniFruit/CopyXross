use super::Clipboard;
use super::ClipboardData;
use super::ClipboardError;
use crate::debug_println;
use crate::utils::create_file;
use crate::utils::open_file;
use dirs_next::desktop_dir;
use objc::class;
use objc::msg_send;
use objc::rc::autoreleasepool;
use objc::runtime::Object;
use objc::sel;
use objc::sel_impl;
use std::ffi::CStr;
use std::ffi::CString;
use std::ptr;
use std::str::FromStr;

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

// #[repr(C)]
// #[derive(Debug, Copy, Clone)]
// struct NSRange {
//     location: usize,
//     length: usize,
// }

type ObjectId = *mut Object;
type NSData = ObjectId;
type NSType = ObjectId;

#[allow(non_upper_case_globals, dead_code)]
const NSUTF16Encoding: i32 = 10;
#[allow(non_upper_case_globals, dead_code)]
const NSUTF8Encoding: i32 = 4;

#[allow(clippy::upper_case_acronyms)]
enum PasteboardType {
    FILEPATH,
    IMAGE,
    TEXT,
    TEXTU16,
    RTF,
    HTML,
}

impl FromStr for PasteboardType {
    type Err = String;

    fn from_str(input: &str) -> Result<PasteboardType, Self::Err> {
        if input == "public.utf8-plain-text" {
            return Ok(PasteboardType::TEXT);
        }

        if input == "public.utf16-external-plain-text" {
            return Ok(PasteboardType::TEXTU16);
        }

        if input == "public.file-url" {
            return Ok(PasteboardType::FILEPATH);
        }

        if input.contains(".rtf") {
            return Ok(PasteboardType::RTF);
        }

        if input.contains(".html") {
            return Ok(PasteboardType::HTML);
        }

        if input.starts_with("public.")
            && (input.contains("png") || input.contains("jpeg") || input.contains("tiff"))
        {
            return Ok(PasteboardType::IMAGE);
        }
        Err(String::from(input))
    }
}

pub struct MacosClipboard {
    p: ObjectId,
}

#[allow(unexpected_cfgs)]
impl MacosClipboard {
    fn read_file(&self, first_type: *mut Object) -> Result<ClipboardData, ClipboardError> {
        let pb = self.p;
        autoreleasepool(|| unsafe {
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
            // slice file:// part
            let file_buf = open_file(&c_str[7..]).map_err(|err| {
                ClipboardError::Read(format!("Failed to read file from buffer: {:?}", err))
            })?;
            let filename = c_str.split("/").last().unwrap_or("unknown_file");
            Ok(ClipboardData::File((String::from(filename), file_buf)))
        })
    }

    /// param plain_string - NSString
    fn convert_plain_string(
        &self,
        plain_string: ObjectId,
    ) -> Result<ClipboardData, ClipboardError> {
        unsafe {
            if plain_string.is_null() {
                return Err(ClipboardError::Read(
                    "Failed to extract plain text from clipboard.".to_string(),
                ));
            }

            // NSData
            let string_data: NSData = msg_send![plain_string, dataUsingEncoding:NSUTF8Encoding];

            if string_data.is_null() {
                return Err(ClipboardError::Read(
                    "Failed to get string data from RTF string".to_string(),
                ));
            }
            let length: usize = msg_send![string_data, length];
            let bytes: *const u8 = msg_send![string_data, bytes];

            if length == 0 || bytes.is_null() {
                return Err(ClipboardError::Read(
                    "Failed to read binary data from clipboard. No data in buffer".to_string(),
                ));
            }

            let slice = std::slice::from_raw_parts(bytes, length);

            Ok(ClipboardData::String(slice.to_vec()))
        }
    }

    fn read_u8_str(&self, t: ObjectId) -> Result<ClipboardData, ClipboardError> {
        let pb = self.p;
        unsafe {
            let ns_string: ObjectId = msg_send![pb, stringForType: t];
            self.convert_plain_string(ns_string)
        }
    }
    fn read_text_from_rtf(&self, first_type: ObjectId) -> Result<ClipboardData, ClipboardError> {
        autoreleasepool(|| {
            unsafe {
                // Get RTF data from the clipboard
                let data: *mut Object = msg_send![self.p, dataForType: first_type];

                if data.is_null() {
                    return Err(ClipboardError::Read(
                        "No RTF data found in clipboard.".to_string(),
                    ));
                }

                // Convert NSData (RTF) to NSAttributedString
                let attributed_string: ObjectId = msg_send![class!(NSAttributedString), alloc];
                let mut document_attributes: ObjectId = ptr::null_mut();
                let attributed_string: ObjectId = msg_send![attributed_string,
                    initWithData:data options:ptr::null::<u8>()
                    documentAttributes:&mut document_attributes error:ptr::null::<u8>()
                ];

                if document_attributes.is_null() {
                    return Err(ClipboardError::Read(
                        "RTF Document attributes is null".to_string(),
                    ));
                }

                if attributed_string.is_null() {
                    return Err(ClipboardError::Read("Failed to parse RTF data".to_string()));
                }

                // Extract the plain string
                // NSString
                let plain_string: ObjectId = msg_send![attributed_string, string];
                self.convert_plain_string(plain_string)
            }
        })
    }
    fn read_text(&self, first_type: ObjectId) -> Result<ClipboardData, ClipboardError> {
        autoreleasepool(|| {
            unsafe {
                // Get NSString from Pasteboard
                let ns_string: ObjectId = msg_send![self.p, stringForType: first_type];
                self.convert_plain_string(ns_string)
            }
        })
    }
    fn read_image(&self, first_type: ObjectId) -> Result<Vec<u8>, ClipboardError> {
        let pb = self.p;
        autoreleasepool(|| {
            unsafe {
                let ns_data: ObjectId = msg_send![pb, dataForType: first_type];
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
        })
    }

    fn read_html(&self, first_type: ObjectId) -> Result<ClipboardData, ClipboardError> {
        autoreleasepool(|| {
            unsafe {
                // Request HTML data
                let data: ObjectId = msg_send![self.p, dataForType: first_type];

                if data.is_null() {
                    return Err(ClipboardError::Read("HTML data is empty".to_string()));
                }

                // Convert NSData to NSString
                let html_string: ObjectId = msg_send![class!(NSString), alloc];
                let html_string: ObjectId =
                    msg_send![html_string, initWithData:data encoding:NSUTF8Encoding];

                self.convert_plain_string(html_string)
            }
        })
    }

    fn write_file(&self, file_data: (String, Vec<u8>)) -> Result<(), ClipboardError> {
        let (filename, data) = file_data;
        let mime_type_opt = filename.split(".").last();
        if mime_type_opt.is_none() {
            return Err(ClipboardError::Write(
                "Failed to write bytes into clipboard. Unknown mime_type".to_string(),
            ));
        }

        let deskt_dir = desktop_dir();
        if deskt_dir.is_none() {
            return Err(ClipboardError::Write(
                "Could not find Desktop directory".to_string(),
            ));
        }

        let mut deskt_dir = deskt_dir.unwrap();
        deskt_dir.push(&filename);

        create_file(data.as_slice(), deskt_dir.to_str().unwrap_or(""))
            .map_err(|err| ClipboardError::Write(format!("Could not write file: {:?}", err)))?;

        debug_println!("Binary data written as {}", mime_type_opt.unwrap());
        Ok(())
    }
    fn write_text(&self, text: &[u8]) -> Result<(), ClipboardError> {
        autoreleasepool(|| {
            unsafe {
                let text = String::from_utf8(text.to_vec()).map_err(|err| {
                    ClipboardError::Write(format!(
                        "Failed to write to clipboard. String is invalid: {:?}",
                        err
                    ))
                })?;
                // Convert Rust `&str` to `NSString`
                let c_text = CString::new(text.as_str()).unwrap();
                let ns_string: ObjectId =
                    msg_send![class!(NSString), stringWithUTF8String: c_text.as_ptr()];

                if ns_string.is_null() {
                    return Err(ClipboardError::Write(
                        "Failed to create NSString".to_string(),
                    ));
                }

                let c_public_text_type = CString::new("public.utf8-plain-text").map_err(|err| {
                    ClipboardError::Write(format!(
                        "Failed to create public text type C string: {:?}",
                        err
                    ))
                })?;

                self.write_str_into_clipboard(&c_public_text_type, ns_string)?;

                debug_println!("Text written to clipboard: {}", text);
                Ok(())
            }
        })
    }
    fn write_str_into_clipboard(
        &self,
        c_public_text_type: &CString,
        ns_string: ObjectId,
    ) -> Result<(), ClipboardError> {
        let pb = self.p;
        unsafe {
            // Define the public text type
            let utf8_type: ObjectId =
                msg_send![class!(NSString), stringWithUTF8String: c_public_text_type.as_ptr()];

            // Clear clipboard before writing
            let _: () = msg_send![pb, clearContents];

            // Write the text to the clipboard
            let success: bool = msg_send![pb, setString: ns_string forType: utf8_type];

            if !success {
                return Err(ClipboardError::Write(
                    "Failed to write text to clipboard".to_string(),
                ));
            }
            Ok(())
        }
    }
    fn get_clipboard_type(&self) -> Result<(PasteboardType, NSType, String), ClipboardError> {
        unsafe {
            autoreleasepool(|| {
                // Get all available types
                let types: ObjectId = msg_send![self.p, types];

                let count: usize = msg_send![types, count];

                for i in 0..count {
                    let ns_type: ObjectId = msg_send![types, objectAtIndex: i];
                    let utf8_cstr: *const i8 = msg_send![ns_type, UTF8String];
                    if !utf8_cstr.is_null() {
                        let type_str = CStr::from_ptr(utf8_cstr).to_string_lossy().into_owned();
                        let pb_type = PasteboardType::from_str(type_str.as_str());

                        match pb_type {
                            Err(_) => continue,
                            _ => {
                                return Ok((pb_type.unwrap(), ns_type, type_str));
                            }
                        }
                    }
                }

                Err(ClipboardError::Read(
                    "Could not find known types".to_string(),
                ))
            })
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
        autoreleasepool(|| {
            match data {
                ClipboardData::File(file_data) => self.write_file(file_data)?,
                ClipboardData::String(data) => self.write_text(data.as_slice())?,
            }
            Ok(())
        })
    }
    fn read(&self) -> Result<ClipboardData, ClipboardError> {
        autoreleasepool(|| {
            unsafe {
                let pb = self.p;

                // Get the first available type
                let types: ObjectId = msg_send![pb, types];
                if types.is_null() {
                    return Err(ClipboardError::Read(
                        "Failed to get pasteboard types".to_string(),
                    ));
                }

                let (p_type, first_type, type_str) = self.get_clipboard_type()?;
                if first_type.is_null() {
                    return Err(ClipboardError::Read("Clipboard is empty".to_string()));
                }

                debug_println!("Pasteboard type: {:?}", type_str);

                match p_type {
                    PasteboardType::TEXT => self.read_u8_str(first_type),
                    PasteboardType::TEXTU16 => self.read_text(first_type),
                    PasteboardType::IMAGE => {
                        debug_println!("Image format detected: {}", type_str);
                        let mime_type = type_str.split(".").last().unwrap_or("png");
                        let filename = format!("image.{}", mime_type);
                        let buff = self.read_image(first_type)?;
                        Ok(ClipboardData::File((filename, buff)))
                    }
                    PasteboardType::RTF => self.read_text_from_rtf(first_type),
                    PasteboardType::FILEPATH => self.read_file(first_type),
                    PasteboardType::HTML => self.read_html(first_type),
                }
            }
        })
    }
}
