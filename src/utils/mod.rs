#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::get_host_name as get_pc_name;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
use windows::get_host_name as get_pc_name;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use std::{env, fs};

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

pub fn get_asset_path(file: &str) -> Result<PathBuf> {
    let mut curr_dir = env::current_dir()?;
    curr_dir.push("assets");
    curr_dir.push(file);
    Ok(curr_dir)
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

pub fn attempt_get_lock<T>(p: &Mutex<T>) -> std::result::Result<MutexGuard<T>, ()> {
    let mut attempts = 0;
    let max_attempts = 5;

    loop {
        if let Ok(p_l) = p.lock() {
            return Ok(p_l);
        } else {
            attempts += 1;
            if attempts >= max_attempts {
                println!(
                    "Could not acquire lock after {} attempts. Giving up.",
                    max_attempts
                );
                return Err(());
            }

            let delay = Duration::from_millis(100 * (2_u64.pow(attempts))); // Exponential backoff
            debug_println!("Data is locked. Retrying in {:?}...", delay);
            thread::sleep(delay);
            continue;
        }
    }
}

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
