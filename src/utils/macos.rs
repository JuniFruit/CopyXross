use std::env;
use std::ffi::CStr;
use std::path::PathBuf;
use std::ptr;

use dirs_next::home_dir;
use objc::class;
use objc::msg_send;
use objc::runtime::Object;
use objc::sel;
use objc::sel_impl;

use super::log_into_file;

#[allow(unexpected_cfgs)]
pub fn get_error(exception: ObjectId) -> String {
    unsafe {
        let err: ObjectId = msg_send![exception, name];
        let err = msg_send![err, UTF8String];
        let err = CStr::from_ptr(err).to_string_lossy();
        let reason: ObjectId = msg_send![exception, reason];
        let reason = msg_send![reason, UTF8String];
        let reason = CStr::from_ptr(reason).to_string_lossy();
        format!("ObjC err: {}. Reason: {}", err, reason)
    }
}

pub type ObjectId = *mut Object;

#[allow(unexpected_cfgs)]
pub fn get_host_name() -> String {
    unsafe {
        let res = catch_and_log_exception(
            |_| {
                let host: ObjectId = msg_send![class!(NSHost), currentHost];
                let host_name: ObjectId = msg_send![host, localizedName];
                let name: *const i8 = msg_send![host_name, UTF8String];
                name as *mut _
            },
            ptr::null_mut() as *mut _,
        );

        if !res.error.is_null() {
            let _ = log_into_file(format!("{:?}", get_error(res.error as ObjectId)).as_str());
        }
        if !res.result.is_null() {
            std::ffi::CStr::from_ptr(res.result as *const i8)
                .to_string_lossy()
                .into_owned()
        } else {
            String::from("Unknown MACOS Machine")
        }
    }
}

#[allow(improper_ctypes)]
#[link(name = "objc_exception_wrapper", kind = "static")]
extern "C" {
    pub fn catch_and_log_exception(
        block: fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void,
        args: *mut std::ffi::c_void,
    ) -> MacOsOperationResult;
}

pub struct MacOsOperationResult {
    pub result: *mut std::ffi::c_void,
    pub error: *mut std::ffi::c_void,
}
pub fn get_log_path() -> PathBuf {
    let mut home = home_dir().unwrap_or(PathBuf::from(""));

    home.push("Library");
    home.push("Logs");
    home
}
pub fn get_asset(filename: &str) -> PathBuf {
    let exe_path = env::current_exe().unwrap_or(PathBuf::from(""));
    let bundle_path = exe_path
        .parent()
        .expect("Failed to get asset path")
        .parent()
        .expect("Failed to get asset path");
    bundle_path.join("Resources").join("assets").join(filename)
}
