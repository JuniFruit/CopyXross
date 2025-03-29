use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null;
use std::ptr::null_mut;
use std::rc::Rc;
use std::sync::Mutex;

use winapi::shared::minwindef::LPARAM;
use winapi::shared::minwindef::LRESULT;
use winapi::shared::minwindef::WPARAM;
use winapi::shared::ntdef::{FALSE, TRUE};
use winapi::shared::windef::HMENU;
use winapi::shared::windef::HWND;
use winapi::shared::windef::POINT;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::shellapi::Shell_NotifyIconW;
use winapi::um::shellapi::NIF_ICON;
use winapi::um::shellapi::NIF_MESSAGE;
use winapi::um::shellapi::NIF_TIP;
use winapi::um::shellapi::NIM_ADD;
use winapi::um::shellapi::NIM_DELETE;
use winapi::um::shellapi::NOTIFYICONDATAW;
use winapi::um::winuser::AppendMenuW;
use winapi::um::winuser::CreatePopupMenu;
use winapi::um::winuser::CreateWindowExW;
use winapi::um::winuser::DefWindowProcW;
use winapi::um::winuser::DispatchMessageW;
use winapi::um::winuser::FindWindowW;
use winapi::um::winuser::GetCursorPos;
use winapi::um::winuser::GetMessageW;
use winapi::um::winuser::InsertMenuItemW;
use winapi::um::winuser::LoadImageW;
use winapi::um::winuser::PostMessageW;
use winapi::um::winuser::RegisterClassW;
use winapi::um::winuser::RemoveMenu;
use winapi::um::winuser::SetForegroundWindow;
use winapi::um::winuser::TrackPopupMenu;
use winapi::um::winuser::TranslateMessage;
use winapi::um::winuser::IMAGE_ICON;
use winapi::um::winuser::LR_DEFAULTSIZE;
use winapi::um::winuser::LR_LOADFROMFILE;
use winapi::um::winuser::MENUITEMINFOW;
use winapi::um::winuser::MF_BYCOMMAND;
use winapi::um::winuser::MF_STRING;
use winapi::um::winuser::MIIM_ID;
use winapi::um::winuser::MIIM_STRING;
use winapi::um::winuser::MSG;
use winapi::um::winuser::TPM_HORNEGANIMATION;
use winapi::um::winuser::TPM_RETURNCMD;
use winapi::um::winuser::TPM_RIGHTBUTTON;
use winapi::um::winuser::WM_CANCELMODE;
use winapi::um::winuser::WM_COMMAND;
use winapi::um::winuser::WM_RBUTTONDOWN;
use winapi::um::winuser::WM_RBUTTONUP;
use winapi::um::winuser::WNDCLASSW;

use crate::debug_println;
use crate::utils::attempt_get_lock;
use crate::utils::get_asset_path;
use crate::utils::windows::WindowsError;

use super::ButtonData;
use super::ButtonFullData;
use super::CallbackFn;
use super::TaskMenuError;
use super::TaskMenuOperations;

pub struct TaskMenuBar {
    window_ptr: HWND,
    menu_ptr: HMENU,
    handlers: Rc<Mutex<HashMap<u32, ButtonFullData>>>,
    item_id_counter: RefCell<u32>,
}
const WM_TRAYICON: u32 = 2;
const ID_MENU_EXIT: u32 = 1;
impl TaskMenuBar {
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TRAYICON => {
                let event = lparam as u32; // Extract event type

                match event {
                    WM_RBUTTONUP => {
                        PostMessageW(hwnd, WM_TRAYICON, 0, WM_RBUTTONUP as isize);
                    }
                    WM_RBUTTONDOWN => {
                        PostMessageW(hwnd, WM_TRAYICON, 0, WM_RBUTTONDOWN as isize);
                    }
                    _ => {}
                }
            }
            _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
        }
        0
    }

    fn check_single_instance(class_name: *const u16) -> Result<(), TaskMenuError> {
        unsafe {
            let res = FindWindowW(class_name, null());

            if !res.is_null() {
                Err(TaskMenuError::Init(
                    "App has already been started!".to_string(),
                ))
            } else {
                Ok(())
            }
        }
    }

    fn add_tray_btn(window_ptr: HWND) -> Result<NOTIFYICONDATAW, TaskMenuError> {
        unsafe {
            let icon_path = get_asset_path("tray_16.ico").map_err(|err| {
                TaskMenuError::Init(format!("Failed to init taskmenu: {:?}", err))
            })?;
            let c_icon_path = OsStr::new(&icon_path)
                .encode_wide()
                .chain(std::iter::once(0)) // Append null terminator
                .collect::<Vec<u16>>();

            let h_icon = LoadImageW(
                null_mut(),           // No module handle
                c_icon_path.as_ptr(), // Path to your icon file
                IMAGE_ICON,           // Load as an icon
                0,
                0,                                // Use default size
                LR_LOADFROMFILE | LR_DEFAULTSIZE, // Load from file, keep default size
            );

            if h_icon.is_null() {
                let err = WindowsError::from_last_error();
                return Err(TaskMenuError::Init(format!(
                    "Failed to load tray icon! {:?}",
                    err
                )));
            }
            let tooltip = "CopyXross App\0".encode_utf16().collect::<Vec<u16>>();
            let mut sz_tip: [u16; 128] = [0; 128];
            for (ind, item) in tooltip.iter().enumerate() {
                sz_tip[ind] = item.to_owned();
            }

            let mut notify_id = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: window_ptr,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                szTip: sz_tip,
                hIcon: h_icon as *mut _,
                uCallbackMessage: WM_TRAYICON,
                ..std::mem::zeroed()
            };
            let res = Shell_NotifyIconW(NIM_ADD, &mut notify_id);
            if res == FALSE.into() {
                return Err(TaskMenuError::Init(format!(
                    "Tray icon failed to init: {:?}",
                    WindowsError::from_last_error()
                )));
            }
            Ok(notify_id)
        }
    }
    fn remove_tray_icon(notify_icon_ptr: *mut NOTIFYICONDATAW) -> Result<(), TaskMenuError> {
        unsafe {
            let res = Shell_NotifyIconW(NIM_DELETE, notify_icon_ptr);
            if res == FALSE.into() {
                return Err(TaskMenuError::Init(format!(
                    "Tray icon failed to init: {:?}",
                    WindowsError::from_last_error()
                )));
            }
            Ok(())
        }
    }
    fn register_menu_item(
        &self,
        item_id: usize,
        btn_data: &ButtonData,
    ) -> Result<(), TaskMenuError> {
        unsafe {
            let btn_title = &btn_data.btn_title;
            let mut menu_title = btn_title
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>();
            if btn_data.is_static {
                let res = AppendMenuW(self.menu_ptr, MF_STRING, item_id, menu_title.as_ptr());
                if res == 0 {
                    let err = WindowsError::from_last_error();
                    return Err(TaskMenuError::Init(format!(
                        "Failed to add button: {:?}",
                        err
                    )));
                }
            } else {
                let mut menu_item = MENUITEMINFOW {
                    cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_ID | MIIM_STRING,
                    wID: item_id as u32,
                    dwItemData: item_id,                 // Command ID
                    dwTypeData: menu_title.as_mut_ptr(), // Pointer to the title
                    ..std::mem::zeroed()
                };
                let res = InsertMenuItemW(
                    self.menu_ptr,
                    btn_data.index.unwrap_or(0) as u32,
                    TRUE.into(),
                    &mut menu_item,
                );
                if res == 0 {
                    let err = WindowsError::from_last_error();
                    return Err(TaskMenuError::Init(format!(
                        "Failed to add button: {:?}",
                        err
                    )));
                }
            }
        }
        Ok(())
    }
    fn handle_click(&self, msg_id: u32) -> bool {
        if let Ok(handler) = attempt_get_lock(&self.handlers) {
            let data = handler.get(&msg_id);
            if let Some(data) = data {
                let cb = &data.handler;
                cb(Some(&data.btn_data));
                true
            } else {
                false
            }
        } else {
            false
        }
    }
    fn close_tray_menu(&self) {
        unsafe {
            PostMessageW(self.window_ptr, WM_CANCELMODE, 0, 0);
        }
    }
    fn show_tray_menu(&self) -> Result<(), TaskMenuError> {
        unsafe {
            let hwnd = self.window_ptr;
            let mut cursor_pos = POINT { x: 0, y: 0 };
            let res = GetCursorPos(&mut cursor_pos);
            if res == 0 {
                return Err(TaskMenuError::Unexpected(format!(
                    "Could not get cursor pos: {:?}",
                    WindowsError::from_last_error()
                )));
            }
            let res = SetForegroundWindow(hwnd);
            if res == 0 {
                return Err(TaskMenuError::Unexpected(
                    "Failed to show popup menu: Foreground window failed to set".to_string(),
                ));
            }
            let cmd = TrackPopupMenu(
                self.menu_ptr,
                TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_HORNEGANIMATION,
                cursor_pos.x,
                cursor_pos.y,
                0,
                hwnd,
                null_mut(),
            );
            if cmd != 0 {
                PostMessageW(hwnd, WM_COMMAND, cmd as usize, 0);
            } else {
                self.close_tray_menu();
            }
            Ok(())
        }
    }
    fn event_loop(&self) -> Result<(), TaskMenuError> {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            let mut res: i32 = 1;
            while res != -1 {
                res = GetMessageW(&mut msg, self.window_ptr, 0, 0);
                if res < 0 {
                    return Err(TaskMenuError::Unexpected(format!(
                        "{:?}",
                        WindowsError::from_last_error()
                    )));
                }
                TranslateMessage(&msg);
                match msg.message {
                    WM_TRAYICON => match msg.lParam as u32 {
                        // WM_RBUTTONDOWN => {
                        //     self.close_tray_menu();
                        // }
                        WM_RBUTTONUP => {
                            if let Err(err) = self.show_tray_menu() {
                                println!("{:?}", err);
                            }
                        }
                        _ => {}
                    },
                    WM_COMMAND => match msg.wParam as u32 {
                        ID_MENU_EXIT => {
                            return Ok(());
                        }
                        _ => {
                            let is_handled = self.handle_click(msg.wParam as u32);
                            if !is_handled {
                                debug_println!("Handle: {} is not handled", msg.wParam);
                            }
                        }
                    },
                    _ => {
                        DispatchMessageW(&msg);
                    }
                }
            }
            Ok(())
        }
    }
    fn find_btn_id_to_remove(&self, btn_data: ButtonData) -> Result<Option<u32>, TaskMenuError> {
        let h_map = attempt_get_lock(&self.handlers)
            .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;
        let mut id: Option<u32> = None;
        let target_attrs = &btn_data.attrs_str;
        let keys = h_map.keys();
        for item_key in keys {
            let item = h_map.get(item_key).unwrap();
            let is_title_eq = btn_data.btn_title == item.btn_data.btn_title;
            let mut is_attrs_eq = btn_data.attrs_str.is_none() && item.btn_data.attrs_str.is_none();
            if target_attrs.is_some() && item.btn_data.attrs_str.is_some() {
                is_attrs_eq =
                    target_attrs.as_ref().unwrap() == item.btn_data.attrs_str.as_ref().unwrap();
            }
            if is_title_eq && is_attrs_eq {
                id = Some(*item_key);
                break;
            }
        }
        Ok(id)
    }

    fn increment_btn_id(&self) -> Result<u32, TaskMenuError> {
        let new_id_counter = *self.item_id_counter.try_borrow().map_err(|err| {
            TaskMenuError::Unexpected(format!("Cant increment btn_id: {:?}", err))
        })? + 1;
        self.item_id_counter.replace(new_id_counter);
        Ok(new_id_counter)
    }
    fn remove_menu_item(&self, id: u32) -> Result<(), TaskMenuError> {
        unsafe {
            let res = RemoveMenu(self.menu_ptr, id, MF_BYCOMMAND);
            if res == 0 {
                return Err(TaskMenuError::Unexpected(format!(
                    "{:?}",
                    WindowsError::from_last_error()
                )));
            };
            Ok(())
        }
    }
}

impl TaskMenuOperations for TaskMenuBar {
    fn add_menu_item(
        &self,
        btn_data: ButtonData,
        on_click: CallbackFn,
    ) -> Result<(), TaskMenuError> {
        if self.menu_ptr.is_null() {
            return Err(TaskMenuError::Unexpected(
                "Menu pointer is null".to_string(),
            ));
        };
        let item_id = self.increment_btn_id()?;
        self.register_menu_item(item_id as usize, &btn_data)?;
        let mut h_map = attempt_get_lock(&self.handlers)
            .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;

        h_map.insert(
            item_id,
            ButtonFullData {
                handler: on_click,
                btn_data,
            },
        );

        Ok(())
    }
    fn remove_all_dyn(&self) -> Result<(), TaskMenuError> {
        {
            let h_map = attempt_get_lock(&self.handlers)
                .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;

            for key in h_map.keys() {
                if !h_map.get(key).unwrap().btn_data.is_static {
                    self.remove_menu_item(*key)?;
                }
            }
        }
        {
            let mut h_map = attempt_get_lock(&self.handlers)
                .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;

            h_map.retain(|_, v| v.btn_data.is_static);
        }

        Ok(())
    }
    fn remove_menu_item(&self, btn_data: ButtonData) -> Result<(), TaskMenuError> {
        let id = self.find_btn_id_to_remove(btn_data)?;
        let mut h_map = attempt_get_lock(&self.handlers)
            .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;

        if let Some(id) = id {
            self.remove_menu_item(id)?;
            h_map.remove(&id);

            Ok(())
        } else {
            Err(TaskMenuError::Unexpected(
                "Could not find button to delete!".to_string(),
            ))
        }
    }
    fn init() -> Result<Self, TaskMenuError> {
        unsafe {
            let class_name = "CopyXrossApp\0".encode_utf16().collect::<Vec<u16>>();
            TaskMenuBar::check_single_instance(class_name.as_ptr())?;
            let h_instance = GetModuleHandleW(null());
            if h_instance.is_null() {
                let err = WindowsError::from_last_error();
                return Err(TaskMenuError::Init(format!("{:?}", err)));
            }

            let window_class = WNDCLASSW {
                style: 0,
                cbClsExtra: 0,
                hCursor: null_mut(),
                cbWndExtra: 0,
                hIcon: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpfnWndProc: Some(TaskMenuBar::window_proc),
                hInstance: h_instance,
                lpszClassName: class_name.as_ptr(),
            };
            let res = RegisterClassW(&window_class);
            if res == 0 {
                let err = WindowsError::from_last_error();
                return Err(TaskMenuError::Init(format!("{:?}", err)));
            }
            let window = CreateWindowExW(
                0,
                class_name.as_ptr(),
                null_mut(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                h_instance,
                null_mut(),
            );
            if window.is_null() {
                let err = WindowsError::from_last_error();
                return Err(TaskMenuError::Init(format!("{:?}", err)));
            }

            let menu_ptr = CreatePopupMenu();

            if menu_ptr.is_null() {
                let err = WindowsError::from_last_error();
                return Err(TaskMenuError::Init(format!("{:?}", err)));
            }

            Ok(TaskMenuBar {
                window_ptr: window,
                menu_ptr,
                handlers: Rc::new(Mutex::new(HashMap::new())),
                item_id_counter: RefCell::new(5),
            })
        }
    }
    fn run(&self) -> Result<(), TaskMenuError> {
        let mut notify_icon_ptr = TaskMenuBar::add_tray_btn(self.window_ptr)?;
        self.event_loop()?;
        TaskMenuBar::remove_tray_icon(&mut notify_icon_ptr)?;
        Ok(())
    }

    fn set_quit_button(&self) -> Result<(), TaskMenuError> {
        self.register_menu_item(ID_MENU_EXIT as usize, &ButtonData::from_str_static("Quit"))?;
        Ok(())
    }
}
// SAFETY: We manually implement Send/Sync because object pointers will
// only be used on one thread at a time.
unsafe impl Send for TaskMenuBar {}
unsafe impl Sync for TaskMenuBar {}
