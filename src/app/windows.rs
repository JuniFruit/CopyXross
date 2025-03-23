use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null;
use std::ptr::null_mut;

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
use winapi::um::shellapi::NOTIFYICONDATAW;
use winapi::um::winuser::AppendMenuW;
use winapi::um::winuser::CreatePopupMenu;
use winapi::um::winuser::CreateWindowExW;
use winapi::um::winuser::DefWindowProcW;
use winapi::um::winuser::DispatchMessageW;
use winapi::um::winuser::GetCursorPos;
use winapi::um::winuser::GetMessageW;
use winapi::um::winuser::LoadImageW;
use winapi::um::winuser::PostMessageW;
use winapi::um::winuser::PostQuitMessage;
use winapi::um::winuser::RegisterClassW;
use winapi::um::winuser::RemoveMenu;
use winapi::um::winuser::SetForegroundWindow;
use winapi::um::winuser::TrackPopupMenu;
use winapi::um::winuser::TranslateMessage;
use winapi::um::winuser::IMAGE_ICON;
use winapi::um::winuser::LR_DEFAULTSIZE;
use winapi::um::winuser::LR_LOADFROMFILE;
use winapi::um::winuser::MF_BYCOMMAND;
use winapi::um::winuser::MF_STRING;
use winapi::um::winuser::MSG;
use winapi::um::winuser::TPM_RETURNCMD;
use winapi::um::winuser::TPM_RIGHTBUTTON;
use winapi::um::winuser::WM_COMMAND;
use winapi::um::winuser::WM_DESTROY;
use winapi::um::winuser::WM_RBUTTONDOWN;
use winapi::um::winuser::WM_RBUTTONUP;
use winapi::um::winuser::WNDCLASSW;

use crate::debug_println;
use crate::utils::get_asset_path;
use crate::utils::windows::WindowsError;

use super::CallbackFn;
use super::TaskMenuError;
use super::TaskMenuOperations;

pub struct TaskMenuBar {
    window_ptr: HWND,
    menu_ptr: HMENU,
    handlers: RefCell<HashMap<u32, CallbackFn>>,
    item_id_to_title: RefCell<HashMap<u32, String>>,
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
                    _ => {}
                }
            }
            _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
        }
        0
    }

    fn add_tray_btn(&self) -> Result<(), TaskMenuError> {
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
                hWnd: self.window_ptr,
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
            Ok(())
        }
    }
    fn register_menu_item(&self, btn_title: String, item_id: usize) -> Result<(), TaskMenuError> {
        unsafe {
            let menu_title = btn_title
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>();
            let res = AppendMenuW(self.menu_ptr, MF_STRING, item_id, menu_title.as_ptr());
            if res == 0 {
                let err = WindowsError::from_last_error();
                return Err(TaskMenuError::Init(format!(
                    "Failed to add button: {:?}",
                    err
                )));
            }
        }
        Ok(())
    }
    fn handle_click(&self, msg_id: u32) -> bool {
        let handler = self.handlers.borrow();
        let mut handler: Option<&Box<dyn Fn(Option<String>)>> = handler.get(&msg_id);
        if let Some(cb) = handler.take() {
            let item_id_to_title = self.item_id_to_title.borrow();
            let btn_title = item_id_to_title.get(&msg_id);
            if btn_title.is_some() {
                cb(Some(btn_title.unwrap().to_owned()));
            } else {
                cb(None);
            }
            true
        } else {
            false
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

            let cmd = TrackPopupMenu(
                self.menu_ptr,
                TPM_RIGHTBUTTON | TPM_RETURNCMD,
                cursor_pos.x,
                cursor_pos.y,
                0,
                hwnd,
                null_mut(),
            );

            if cmd != 0 {
                PostMessageW(hwnd, WM_COMMAND, cmd as usize, 0);
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
                    WM_TRAYICON => {
                        if msg.lParam as u32 == WM_RBUTTONUP {
                            self.show_tray_menu()?;
                        }
                    }
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
}

impl TaskMenuOperations for TaskMenuBar {
    fn add_menu_item(&self, btn_title: String, on_click: CallbackFn) -> Result<(), TaskMenuError> {
        if self.menu_ptr.is_null() {
            return Err(TaskMenuError::Unexpected(
                "Menu pointer is null".to_string(),
            ));
        };
        let item_id_counter = *self.item_id_counter.borrow() + 1;
        self.item_id_counter.replace(item_id_counter);
        self.register_menu_item(btn_title, item_id_counter as usize)?;
        self.handlers.borrow_mut().insert(item_id_counter, on_click);

        Ok(())
    }
    fn remove_menu_item(&self, btn_title: String) -> Result<(), TaskMenuError> {
        unsafe {
            let mut item_id_to_title = self.item_id_to_title.borrow_mut();
            let mut handlers = self.handlers.borrow_mut();
            let mut iter = item_id_to_title.iter();
            let item_entry = iter.find(|item| item.1 == &btn_title);
            let mut item_id: u32 = 0;
            if item_entry.is_some() {
                item_id = item_entry.unwrap().0.to_owned();
                let res = RemoveMenu(self.menu_ptr, item_id, MF_BYCOMMAND);
                if res == 0 {
                    return Err(TaskMenuError::Unexpected(format!(
                        "{:?}",
                        WindowsError::from_last_error()
                    )));
                }
            }
            item_id_to_title.remove(&item_id);
            handlers.remove(&item_id);
        }
        Ok(())
    }
    fn init() -> Result<Self, TaskMenuError> {
        unsafe {
            let h_instance = GetModuleHandleW(null());
            if h_instance.is_null() {
                let err = WindowsError::from_last_error();
                return Err(TaskMenuError::Init(format!("{:?}", err)));
            }
            let class_name = "CopyXrossApp\0".encode_utf16().collect::<Vec<u16>>();
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
                handlers: RefCell::new(HashMap::new()),
                item_id_to_title: RefCell::new(HashMap::new()),
                item_id_counter: RefCell::new(5),
            })
        }
    }
    fn run(&self) -> Result<(), TaskMenuError> {
        self.add_tray_btn()?;
        self.event_loop()?;
        Ok(())
    }
    fn set_quit_button(&self) -> Result<(), TaskMenuError> {
        self.register_menu_item("Quit".to_string(), ID_MENU_EXIT as usize)?;
        Ok(())
    }
}
// SAFETY: We manually implement Send/Sync because object pointers will
// only be used on one thread at a time.
unsafe impl Send for TaskMenuBar {}
unsafe impl Sync for TaskMenuBar {}
