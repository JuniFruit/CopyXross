use std::{
    ffi::{c_char, CStr},
    fs,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::Duration,
};

const KX: u32 = 123456789;
const KY: u32 = 362436069;
const KZ: u32 = 521288629;
const KW: u32 = 88675123;

pub type Filename = String;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Rand {
    x: u32,
    y: u32,
    z: u32,
    w: u32,
}

#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        if ::std::cfg!(debug_assertions) {
            ::std::print!("{}\r\n", format!($($arg)*));
        }
    }
}

#[allow(dead_code)]
impl Rand {
    pub fn new(seed: u32) -> Rand {
        Rand {
            x: KX ^ seed,
            y: KY ^ seed,
            z: KZ,
            w: KW,
        }
    }

    // Xorshift 128, taken from German Wikipedia
    pub fn rand(&mut self) -> u32 {
        let t = self.x ^ self.x.wrapping_shl(11);
        self.x = self.y;
        self.y = self.z;
        self.z = self.w;
        self.w ^= self.w.wrapping_shr(19) ^ t ^ t.wrapping_shr(8);
        self.w
    }

    pub fn rand_range(&mut self, a: i32, b: i32) -> i32 {
        let m = (b - a + 1) as u32;
        a + (self.rand() % m) as i32
    }

    pub fn rand_float(&mut self) -> f64 {
        (self.rand() as f64) / (<u32>::MAX as f64)
    }
}

pub fn open_file(path: &str) -> Result<Vec<u8>> {
    let file = fs::read(path)?;

    Ok(file)
}

pub fn create_file(file: &[u8], path: &str) -> Result<()> {
    fs::write(path, file)?;
    Ok(())
}

pub fn format_bytes_size(size: usize) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else {
        let mbs: f32 = size as f32 / (1024.0 * 1024.0);
        format!("{:.2} MB", mbs)
    }
}

pub fn write_progress(curr: usize, total: usize) {
    println!(
        "Transfering: {}/{}",
        format_bytes_size(curr),
        format_bytes_size(total)
    )
}

/// Return plain string from html. If html is invalid returns empty string
pub fn extract_plain_str_from_html(html: &str) -> String {
    let mut is_tag = false;
    let mut result = String::new();
    for c in html.chars() {
        if c == '<' {
            is_tag = true;
            continue;
        }

        if c == '>' {
            is_tag = false;
            continue;
        }
        if !is_tag {
            result.push(c);
        }
    }
    result
}

pub fn attempt_get_lock<T, F>(p: &Arc<Mutex<T>>, op: F)
where
    F: FnOnce(MutexGuard<T>),
{
    let mut attempts = 0;
    let max_attempts = 5;

    loop {
        match p.lock() {
            Ok(p_l) => {
                op(p_l);

                break; // Success, exit loop
            }
            Err(_) => {
                attempts += 1;
                if attempts >= max_attempts {
                    debug_println!(
                        "Could not acquire lock after {} attempts. Giving up.",
                        max_attempts
                    );
                    break;
                }

                let delay = Duration::from_millis(100 * (2_u64.pow(attempts))); // Exponential backoff
                debug_println!("Data is locked. Retrying in {:?}...", delay);
                thread::sleep(delay);
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn logger(err: *mut c_char, reason: *mut c_char) {
    unsafe {
        let err = CStr::from_ptr(err).to_string_lossy();
        let reason = CStr::from_ptr(reason).to_string_lossy();

        println!("{:?}, {:?}", err, reason);
    }
}

#[cfg(target_os = "macos")]
#[allow(improper_ctypes)]
extern "C" {
    pub fn catch_and_log_exception(rust_callback: fn(*mut c_char, *mut c_char), block: fn());
}

// pub fn from_u8_to_u16(bytes: &[u8]) -> std::result::Result<Vec<u16>, ParseErrors> {
//     unsafe {
//         let my_u16_vec_bis: Vec<u16> = (bytes.align_to::<u16>().1)
//             .to_vec()
//             .iter()
//             .map(|e| e >> 8 | (e & 0xff) << 8)
//             .collect();
//
//         if my_u16_vec_bis.len() != bytes.len() / 2 {
//             return Err(ParseErrors::InvalidStructure);
//         }
//
//         Ok(my_u16_vec_bis)
//     }
// }
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_plain_str_from_html() {
        // Test case: basic HTML
        assert_eq!(extract_plain_str_from_html("<b>Hello</b>"), "Hello");

        // Test case: nested tags
        assert_eq!(
            extract_plain_str_from_html("<div><p>Test</p></div>"),
            "Test"
        );

        // Test case: text with multiple tags
        assert_eq!(
            extract_plain_str_from_html("<h1>Title</h1> <p>Paragraph</p>"),
            "Title Paragraph"
        );

        // Test case: HTML with attributes
        assert_eq!(
            extract_plain_str_from_html("<a href='https://example.com'>Link</a>"),
            "Link"
        );

        // Test case: empty string
        assert_eq!(extract_plain_str_from_html(""), "");

        // Test case: plain text without tags
        assert_eq!(extract_plain_str_from_html("Just text"), "Just text");

        // Test case: incorrectly formatted HTML
        assert_eq!(extract_plain_str_from_html("<b>Bold text"), "Bold text");

        // Test case: multiple consecutive tags
        assert_eq!(
            extract_plain_str_from_html("<i><b>Styled</b></i>"),
            "Styled"
        );
    }
}
