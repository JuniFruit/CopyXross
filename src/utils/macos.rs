use std::ffi::CStr;

use objc::msg_send;
use objc::runtime::Object;
use objc::sel;
use objc::sel_impl;

#[allow(unexpected_cfgs)]
pub fn get_error(exception: *mut Object) -> String {
    unsafe {
        let err: *mut Object = msg_send![exception, name];
        let err = msg_send![err, UTF8String];
        let err = CStr::from_ptr(err).to_string_lossy();
        let reason: *mut Object = msg_send![exception, reason];
        let reason = msg_send![reason, UTF8String];
        let reason = CStr::from_ptr(reason).to_string_lossy();
        format!("ObjC err: {}. Reason: {}", err, reason)
    }
}

#[allow(improper_ctypes)]
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
