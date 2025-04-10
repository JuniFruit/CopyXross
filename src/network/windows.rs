use std::ptr::null_mut;

use winapi::shared::netioapi::NotifyIpInterfaceChange;
use winapi::shared::netioapi::MIB_NOTIFICATION_TYPE;
use winapi::shared::netioapi::PMIB_IPINTERFACE_ROW;
use winapi::shared::ntdef::HANDLE;
use winapi::shared::ntdef::PVOID;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::shared::winerror::NO_ERROR;
use winapi::shared::ws2def::AF_UNSPEC;
use winapi::um::wlanapi::wlan_intf_opcode_interface_state;
use winapi::um::wlanapi::WlanCloseHandle;
use winapi::um::wlanapi::WlanEnumInterfaces;
use winapi::um::wlanapi::WlanFreeMemory;
use winapi::um::wlanapi::WlanOpenHandle;
use winapi::um::wlanapi::WlanQueryInterface;
use winapi::um::wlanapi::WLAN_INTERFACE_INFO_LIST;
use winapi::um::wlanapi::WLAN_INTERFACE_STATE;

use crate::debug_println;
use crate::utils::log_into_file;
use crate::utils::windows::WindowsError;

use super::NetworkListener;

#[repr(C)]
pub struct Network {
    user_cb: Option<Box<dyn Fn()>>,
    handle: HANDLE,
}

impl Network {
    extern "system" fn on_network_change(
        context: PVOID,
        _row: PMIB_IPINTERFACE_ROW,
        _not_type: MIB_NOTIFICATION_TYPE,
    ) {
        unsafe {
            if context.is_null() {
                return;
            }

            let context_struct = &*(context as *mut Network);

            let cb = &context_struct.user_cb;
            if let Some(cb) = cb {
                cb()
            } else {
                debug_println!("Network has changed, but no user callback was found")
            }
        }
    }
    fn free_wlan(list: PVOID) {
        unsafe {
            if !list.is_null() {
                WlanFreeMemory(list as *mut _);
            }
        }
    }
    fn debug_print_all_ints(list: *mut WLAN_INTERFACE_INFO_LIST) {
        unsafe {
            let interface_list = *list;

            println!(
                "Found {} network interface(s):",
                interface_list.dwNumberOfItems
            );

            for i in 0..interface_list.dwNumberOfItems {
                let interface_info = interface_list.InterfaceInfo[i as usize];
                let interface_name =
                    String::from_utf16_lossy(&interface_info.strInterfaceDescription);
                println!("Interface {}: {}", i, interface_name.trim());

                // Print interface GUID
                let guid = interface_info.InterfaceGuid;
                println!(
                    "   🔹 GUID: {:X}-{:X}-{:X}-{:X?}",
                    guid.Data1, guid.Data2, guid.Data3, guid.Data4
                );
            }
        }
    }
}

impl NetworkListener for Network {
    fn init(cb: Option<Box<dyn Fn()>>) -> Result<Self, super::NetworkError> {
        let mut handle = null_mut();
        let net_data = Box::into_raw(Box::new(Network {
            user_cb: cb,
            handle,
        }));

        unsafe {
            let res = NotifyIpInterfaceChange(
                AF_UNSPEC as u16,
                Some(Network::on_network_change),
                net_data as *mut _,
                0,
                &mut handle,
            );
            if res != NO_ERROR {
                return Err(super::NetworkError::Init(format!(
                    "{:?}",
                    WindowsError::from(res)
                )));
            }
        }
        // workaround to make compiler happy
        // caller does not have access to underline fields anyway, all other methods will be working as expected
        Ok(Network {
            user_cb: None,
            handle,
        })
    }
    fn is_en0_connected() -> bool {
        unsafe {
            let mut handle: HANDLE = null_mut();
            let mut negotiated_version = 0;

            // Open WLAN API handle
            let open_handle_res =
                WlanOpenHandle(2, null_mut(), &mut negotiated_version, &mut handle);
            if open_handle_res != ERROR_SUCCESS {
                let _ = log_into_file(
                    format!(
                        "Failed to open handle: {:?}",
                        WindowsError::from(open_handle_res)
                    )
                    .as_str(),
                );
                return false;
            }

            let mut interface_list_ptr: *mut WLAN_INTERFACE_INFO_LIST = null_mut();

            // Get list of Wi-Fi interfaces
            let wlan_enum_res = WlanEnumInterfaces(handle, null_mut(), &mut interface_list_ptr);
            if wlan_enum_res != ERROR_SUCCESS {
                let _ = log_into_file(
                    format!(
                        "Failed to enum interfaces: {:?}",
                        WindowsError::from(wlan_enum_res)
                    )
                    .as_str(),
                );
                WlanCloseHandle(handle, null_mut());
                return false;
            }

            let interface_list = *interface_list_ptr;
            if interface_list.dwNumberOfItems == 0 {
                WlanCloseHandle(handle, null_mut());
                Network::free_wlan(interface_list_ptr as *mut _);
                return false;
            }

            if cfg!(debug_assertions) {
                Network::debug_print_all_ints(interface_list_ptr);
            }

            let interface_info = interface_list.InterfaceInfo[0]; // Get first Wi-Fi interface

            let mut radio_state_ptr: *mut WLAN_INTERFACE_STATE = null_mut();
            let mut data_size = 0;

            // Query Wi-Fi radio state
            let result = WlanQueryInterface(
                handle,
                &interface_info.InterfaceGuid,
                wlan_intf_opcode_interface_state,
                null_mut(),
                &mut data_size,
                &mut radio_state_ptr as *mut _ as *mut *mut _,
                null_mut(),
            );

            if result != ERROR_SUCCESS {
                let _ = log_into_file(
                    format!("Failed to query wifi btn: {:?}", WindowsError::from(result)).as_str(),
                );
                Network::free_wlan(interface_list_ptr as *mut _);
                Network::free_wlan(radio_state_ptr as *mut _);
                WlanCloseHandle(handle, null_mut());
                return false;
            };

            let wifi_enabled = *radio_state_ptr == 1; // connected state

            // Cleanup
            Network::free_wlan(interface_list_ptr as *mut _);
            Network::free_wlan(radio_state_ptr as *mut _);
            WlanCloseHandle(handle, null_mut());
            return wifi_enabled;
        }
    }
    fn start_listen(&self) -> Result<(), super::NetworkError> {
        if self.handle.is_null() {
            return Err(super::NetworkError::Init(
                "Network listener failed to register.".to_string(),
            ));
        }
        Ok(())
    }
}
