use std::ffi::c_void;
use std::process::Command;
use std::ptr::null_mut;

use block::Block;
use block::ConcreteBlock;

use crate::utils::macos::catch_and_log_exception;
use crate::utils::macos::get_error;
use crate::utils::macos::ObjectId;

use super::NetworkError;
use super::NetworkListener;

#[link(name = "Network", kind = "framework")]
extern "C" {
    fn nw_path_monitor_create() -> *mut c_void;
    fn nw_path_monitor_set_update_handler(
        monitor: *mut c_void,
        handler: &Block<(*mut c_void,), ()>,
    );
    fn nw_path_monitor_start(monitor: *mut c_void);
}

extern "C" {
    static _NSConcreteGlobalBlock: *const c_void;
}

pub struct Network {
    nw_monitor: *mut c_void,
}

#[allow(unexpected_cfgs)]
impl NetworkListener for Network {
    fn init(cb: Option<Box<dyn Fn()>>) -> Result<Self, super::NetworkError> {
        unsafe {
            let res = catch_and_log_exception(|_| nw_path_monitor_create(), null_mut() as *mut _);
            if !res.error.is_null() {
                return Err(NetworkError::Init(get_error(res.error as ObjectId)));
            }
            let block = ConcreteBlock::new(move |_path: *mut c_void| {
                if let Some(cb) = cb.as_ref() {
                    cb()
                }
            });

            let block = block.copy();
            nw_path_monitor_set_update_handler(res.result, &block);

            Ok(Network {
                nw_monitor: res.result,
            })
        }
    }
    fn start_listen(&self) -> Result<(), super::NetworkError> {
        unsafe {
            let res = catch_and_log_exception(
                |monitor| {
                    nw_path_monitor_start(monitor);
                    null_mut()
                },
                self.nw_monitor,
            );
            if !res.error.is_null() {
                return Err(NetworkError::Init(get_error(res.error as ObjectId)));
            }

            Ok(())
        }
    }
    // fn cancel_listen(&self) -> Result<(), NetworkError> {
    //     unsafe {
    //         let res = catch_and_log_exception(
    //             |monitor| {
    //                 nw_path_monitor_cancel(monitor);
    //                 null_mut()
    //             },
    //             self.nw_monitor,
    //         );
    //         if !res.error.is_null() {
    //             return Err(NetworkError::Init(get_error(res.error as ObjectId)));
    //         }
    //
    //         Ok(())
    //     }
    // }
    fn is_en0_connected() -> bool {
        if let Ok(output) = Command::new("ifconfig")
            .arg("en0") // Wi-Fi interface on macOS
            .output()
        {
            if let Ok(result) = String::from_utf8(output.stdout) {
                return result.contains("status: active");
            }
        }
        false
    }
}
