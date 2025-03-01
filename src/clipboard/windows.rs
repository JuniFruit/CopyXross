use std::ffi::{CString, OsString};
use std::os::windows::ffi::OsStringExt;
use std::ptr::copy_nonoverlapping;

use crate::clipboard::StringType;
use crate::debug_println;
use crate::utils::{create_file, extract_plain_str_from_html, open_file};

use super::{Clipboard, ClipboardData, ClipboardError};
use dirs_next::desktop_dir;
use winapi::shared::minwindef::UINT;
use winapi::shared::ntdef::{FALSE, NULL};
use winapi::shared::windef::HWND__;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::shellapi::{DragQueryFileW, HDROP};
use winapi::um::winbase::{GlobalAlloc, GlobalFree, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use winapi::um::winnt::HANDLE;
use winapi::um::winuser::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardFormatNameA, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatA,
    SetClipboardData, CF_DIB, CF_HDROP, CF_UNICODETEXT,
};
enum ClipboardType {
    TEXT,
    FILE,
    IMAGE,
    // HTML,
}

impl ClipboardType {
    fn from_id(id: UINT) -> Result<Self, ClipboardError> {
        match id {
            CF_UNICODETEXT => Ok(ClipboardType::TEXT),
            CF_HDROP => Ok(ClipboardType::FILE),
            CF_DIB => Ok(ClipboardType::IMAGE),
            _ => Err(ClipboardError::Init(format!(
                "Unknown clipboard ID: {}",
                id
            ))),
        }
    }
}

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
                    "Failed to close clipboard".to_string(),
                ));
            }
            Ok(())
        }
    }
    fn prepare_html_string(html: &str) -> String {
        let mut output = String::new();

        // Define the header template
        let header = concat!(
            "Version:1.0\r\n",
            "StartHTML:<<<<<<<1\r\n",
            "EndHTML:<<<<<<<2\r\n",
            "StartFragment:<<<<<<<3\r\n",
            "EndFragment:<<<<<<<4\r\n"
        );

        output.push_str(header);
        let start_html = output.len();
        if !html.contains("<html>") {
            output.push_str("<html>");
        }

        output.push_str("<!--StartFragment-->");
        let start_fragment = output.len();
        if !html.contains("<body>") {
            output.push_str("<body>");
        }

        output.push_str(html);
        if !html.contains("</body>") {
            output.push_str("</body>");
        }
        let end_fragment = output.len();

        output.push_str("<!--EndFragment-->");
        if !html.contains("</html>") {
            output.push_str("</html>");
        }
        let end_html = output.len();

        // Replace placeholders with formatted values
        let replace_placeholder = |s: &mut String, placeholder: &str, value: usize| {
            *s = s.replacen(placeholder, &format!("{:08}", value), 1);
        };

        replace_placeholder(&mut output, "<<<<<<<1", start_html);
        replace_placeholder(&mut output, "<<<<<<<2", end_html);
        replace_placeholder(&mut output, "<<<<<<<3", start_fragment);
        replace_placeholder(&mut output, "<<<<<<<4", end_fragment);

        output
    }

    fn convert_to_utf16(text: &[u8]) -> Result<Vec<u16>, ClipboardError> {
        let mut utf16: Vec<u16> = String::from_utf8(text.to_vec())
            .map_err(|err| ClipboardError::Write(format!("Could not decode string: {:?}", err)))?
            .encode_utf16()
            .collect();
        utf16.push(0);
        Ok(utf16)
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
            let text = String::from_utf16_lossy(slice);

            debug_println!("Text: {}", text);

            Ok(ClipboardData::String((
                StringType::Utf8Plain,
                text.as_bytes().to_vec(),
            )))
        }
    }
    fn print_cp_types() {
        unsafe {
            let mut format = 0u32;
            println!("📋 Available Clipboard Formats:");

            // Enumerate clipboard formats
            while {
                format = EnumClipboardFormats(format);
                format != 0
            } {
                // Try to get the clipboard format name
                let mut buffer = [0_i8; 127];
                let length = GetClipboardFormatNameA(format, buffer.as_mut_ptr(), 127);

                let format_name = if length > 0 {
                    let u8slice: &[u8] =
                        std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len());
                    String::from_utf8(u8slice[..length as usize].to_vec()).unwrap()
                } else {
                    format!("[System Format: {}]", format)
                };

                println!("🔹 Format ID: {:?} → {:?}", format, format_name);
            }
        }
    }
    fn get_clipboard_type() -> Result<ClipboardType, ClipboardError> {
        unsafe {
            let mut format_undefined: bool = false;
            let mut format = 0;

            while !format_undefined {
                format = EnumClipboardFormats(format);
                let cp_type = ClipboardType::from_id(format);
                format_undefined = format == 0;
                match cp_type {
                    Err(_) => continue,
                    _ => return cp_type,
                }
            }
            Err(ClipboardError::Init(
                "No known clipboard types were found".to_string(),
            ))
        }
    }

    fn read_file() -> Result<ClipboardData, ClipboardError> {
        unsafe {
            let handle = WindowsClipboard::get_clipboard_data_handle(CF_HDROP)?;
            let mut file_path = [0_u16; 1024];
            // Get the number of files

            let file_exists = DragQueryFileW(
                handle as HDROP,
                0,
                file_path.as_mut_ptr(),
                file_path.len() as u32,
            );
            if file_exists == 0 {
                return Err(ClipboardError::Read(
                    "No files were found on the clipboard".to_string(),
                ));
            }

            // Convert UTF-16 buffer to Rust String
            let file_path = OsString::from_wide(&file_path)
                .to_string_lossy()
                .into_owned();
            let file_path = file_path.trim_end_matches("\0");

            let file = open_file(&file_path).map_err(|err| {
                ClipboardError::Read(format!("Could not get file from clipboard: {:?}", err))
            })?;
            let file_name = file_path.split("\\").last().unwrap_or("unknown.txt");
            Ok(ClipboardData::File((file_name.to_string(), file)))
        }
        // Cast handle to HDROP
    }
    fn write_text(text: &[u8], s_type: StringType) -> Result<(), ClipboardError> {
        unsafe {
            if EmptyClipboard() == FALSE.into() {
                return Err(ClipboardError::Write(format!(
                    "Failed to clear clipboard: {:?}",
                    GetLastError()
                )));
            }
            match s_type {
                StringType::Utf8Plain => {
                    let utf16 = WindowsClipboard::convert_to_utf16(&text)?;
                    let size_in_bytes = utf16.len() * size_of::<u16>();

                    WindowsClipboard::write_str_into_cp(
                        utf16.as_ptr(),
                        CF_UNICODETEXT,
                        size_in_bytes,
                        utf16.len(),
                    )
                }
                StringType::Html => {
                    // Format HTML according to the Windows Clipboard HTML format specification
                    let html_content = String::from_utf8(text.to_vec()).map_err(|err| {
                        ClipboardError::Write(format!(
                            "Failed to decode string for writing: {:?}",
                            err
                        ))
                    })?;

                    let html_clipboard_format =
                        WindowsClipboard::prepare_html_string(&html_content);

                    // Convert to UTF-8 bytes
                    let utf8_bytes = html_clipboard_format.as_bytes();

                    let size = utf8_bytes.len();
                    let format_name = CString::new("HTML Format").map_err(|err| {
                        ClipboardError::Init(format!(
                            "Failed to create format name CString: {:?}",
                            err
                        ))
                    })?;

                    let html_format = RegisterClipboardFormatA(format_name.as_ptr());
                    if html_format == 0 {
                        return Err(ClipboardError::Write(format!(
                            "Failed to register clipboard format: {:?}",
                            GetLastError()
                        )));
                    }

                    WindowsClipboard::write_str_into_cp(
                        utf8_bytes.as_ptr(),
                        html_format,
                        size,
                        size,
                    )?;

                    let text = extract_plain_str_from_html(
                        String::from_utf8(text.to_vec())
                            .map_err(|err| {
                                ClipboardError::Write(format!(
                                    "Cannot create string from utf8: {:?}",
                                    err
                                ))
                            })?
                            .as_str(),
                    );
                    let utf16 = WindowsClipboard::convert_to_utf16(&text.as_bytes())?;

                    let size_in_bytes = utf16.len() * size_of::<u16>();

                    WindowsClipboard::write_str_into_cp(
                        utf16.as_ptr(),
                        CF_UNICODETEXT,
                        size_in_bytes,
                        utf16.len(),
                    )
                }
            }
        }
    }
    fn write_str_into_cp<T>(
        data: *const T,
        cp_type: UINT,
        size: usize,
        count: usize,
    ) -> Result<(), ClipboardError> {
        unsafe {
            // Allocate global memory for the string
            let hmem = GlobalAlloc(GMEM_MOVEABLE, size);
            if hmem.is_null() {
                return Err(ClipboardError::Write("Failed to allocate memory.".into()));
            }

            // Lock memory and copy string
            let locked_mem = GlobalLock(hmem) as *mut T;
            if locked_mem.is_null() {
                GlobalFree(hmem);
                return Err(ClipboardError::Write("Failed to lock memory.".into()));
            }

            copy_nonoverlapping(data, locked_mem, count);
            GlobalUnlock(hmem);

            // Set clipboard data
            if SetClipboardData(cp_type, hmem).is_null() {
                GlobalFree(hmem);
                return Err(ClipboardError::Write(format!(
                    "Failed to set clipboard data: {:?}",
                    GetLastError()
                )));
            }

            Ok(())
        }
    }
    fn write_file(filename: String, data: &[u8]) -> Result<(), ClipboardError> {
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

        create_file(data, deskt_dir.to_str().unwrap_or(""))
            .map_err(|err| ClipboardError::Write(format!("Could not write file: {:?}", err)))?;

        debug_println!("Binary data written as {}", mime_type_opt.unwrap());
        Ok(())
    }
}
#[allow(non_snake_case)]
impl Clipboard for WindowsClipboard {
    fn init() -> Result<Self, ClipboardError> {
        Ok(WindowsClipboard)
    }
    fn write(&self, data: ClipboardData) -> Result<(), ClipboardError> {
        // Open clipboard (NULL or GetDesktopWindow())
        WindowsClipboard::open()?;
        let res = match data {
            ClipboardData::String((s_type, data)) => WindowsClipboard::write_text(&data, s_type),
            ClipboardData::File((file_name, data)) => {
                WindowsClipboard::write_file(file_name, &data)
            }
        };
        WindowsClipboard::close()?;
        res
    }
    fn read(&self) -> Result<ClipboardData, ClipboardError> {
        // Open clipboard (NULL or GetDesktopWindow())
        WindowsClipboard::open()?;

        if cfg!(debug_assertions) {
            WindowsClipboard::print_cp_types();
        }

        let cp_type = WindowsClipboard::get_clipboard_type()?;

        // Handle different clipboard formats
        let result = match cp_type {
            ClipboardType::TEXT => Self::read_text(),
            ClipboardType::FILE => Self::read_file(),
            ClipboardType::IMAGE => Err(ClipboardError::Read(
                "Clipboard contains an image (DIB)".to_string(),
            )),
        };

        WindowsClipboard::close()?;
        result
    }
}
