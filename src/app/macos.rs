use std::collections::HashMap;
use std::ffi::CStr;
use std::ffi::CString;
use std::ptr;
use std::rc::Rc;
use std::sync::Mutex;

use objc::class;
use objc::msg_send;
use objc::rc::autoreleasepool;
use objc::runtime::Object;
use objc::runtime::Sel;
use objc::sel;
use objc::sel_impl;

use crate::debug_println;
use crate::utils::attempt_get_lock;
use crate::utils::macos::catch_and_log_exception;
use crate::utils::macos::get_error;

use super::ButtonData;
use super::CallbackFn;
use super::TaskMenuError;
use super::TaskMenuOperations;

type ObjectId = *mut Object;

#[allow(unexpected_cfgs)]
extern "C" fn menu_item_clicked(this: &Object, _cmd: Sel, sender: ObjectId) {
    unsafe {
        autoreleasepool(|| {
            let title: ObjectId = msg_send![sender, title];
            let cstr: *const i8 = msg_send![title, UTF8String];

            if !title.is_null() && !cstr.is_null() {
                let handlers_ptr = *this.get_ivar::<ObjectId>("handlers_ptr");

                if handlers_ptr.is_null() {
                    debug_println!("Handlers pointer is null");
                    return;
                }
                let btn_data_map = *this.get_ivar::<ObjectId>("btn_data_map");
                if btn_data_map.is_null() {
                    debug_println!("Button data pointer is null");
                    return;
                }

                let handlers_ptr: &Mutex<HashMap<String, CallbackFn>> =
                    &*(handlers_ptr as *mut Mutex<HashMap<String, CallbackFn>>);
                let btn_data_map: &Mutex<HashMap<String, ButtonData>> =
                    &*(btn_data_map as *mut Mutex<HashMap<String, ButtonData>>);
                let key = CStr::from_ptr(cstr).to_string_lossy().to_string();
                if let Ok(h_map) = attempt_get_lock(handlers_ptr) {
                    let handler = h_map.get(&key);
                    if let Ok(h_map_btn) = attempt_get_lock(btn_data_map) {
                        let btn_data = h_map_btn.get(&key);

                        if let Some(cb) = handler {
                            if let Some(btn_data) = btn_data {
                                cb(Some(btn_data));
                            } else {
                                cb(None);
                            }
                        }
                    };
                };
            }
        })
    }
}

pub struct TaskMenuBar {
    menu_ref: ObjectId,
    app_ref: ObjectId,
    app_delegate: ObjectId,
    // cp_submenu_ref: ObjectId,
    handlers: Rc<Mutex<HashMap<String, CallbackFn>>>,
    title_to_btn_data: Rc<Mutex<HashMap<String, ButtonData>>>,
}

#[allow(unexpected_cfgs)]
impl TaskMenuBar {
    fn string_to_nsstring(str: &str) -> Result<ObjectId, TaskMenuError> {
        unsafe {
            let c_str =
                CString::new(str).map_err(|err| TaskMenuError::Init(format!("{:?}", err)))?;

            let res: ObjectId = msg_send![class!(NSString), stringWithUTF8String: c_str.as_ptr()];
            if res.is_null() {
                return Err(TaskMenuError::Init("Failed to init NSString".to_string()));
            };

            Ok(res)
        }
    }
    fn create_app_delegate(
        handlers: Rc<Mutex<HashMap<String, CallbackFn>>>,
        title_to_btn_data: Rc<Mutex<HashMap<String, ButtonData>>>,
    ) -> Result<ObjectId, TaskMenuError> {
        unsafe {
            let raw_handlers = Rc::into_raw(handlers) as *mut _;
            let raw_title_to_btn_data = Rc::into_raw(title_to_btn_data) as *mut _;
            #[allow(clippy::type_complexity)]
            let boxed: Box<(
                *mut Mutex<HashMap<String, CallbackFn>>,
                *mut Mutex<HashMap<String, ButtonData>>,
            )> = Box::new((raw_handlers, raw_title_to_btn_data));
            let result = catch_and_log_exception(
                |args| {
                    let converted = Box::from_raw(
                        args as *mut (
                            *mut Mutex<HashMap<String, CallbackFn>>,
                            *mut Mutex<HashMap<String, ButtonData>>,
                        ),
                    );
                    let (handlers_ptr, btn_data_map) = *converted;
                    let superclass = class!(NSObject);
                    let cls_name = "AppDelegate";
                    let decl = objc::declare::ClassDecl::new(cls_name, superclass);

                    let mut decl = decl.unwrap();

                    decl.add_ivar::<ObjectId>("handlers_ptr");
                    decl.add_ivar::<ObjectId>("btn_data_map");

                    // Dynamic handler for menu clicks
                    decl.add_method(
                        sel!(menu_item_clicked:),
                        menu_item_clicked as extern "C" fn(&Object, Sel, *mut Object),
                    );

                    let new_class = decl.register();
                    let delegate_obj: ObjectId = msg_send![new_class, new]; // Create instance of AppDelegate

                    (*delegate_obj).set_ivar("handlers_ptr", handlers_ptr as ObjectId);
                    (*delegate_obj).set_ivar("btn_data_map", btn_data_map as ObjectId);
                    delegate_obj as *mut std::ffi::c_void
                },
                Box::into_raw(boxed) as *mut std::ffi::c_void,
            );

            if result.result.is_null() && !result.error.is_null() {
                let err = get_error(result.error as ObjectId);
                return Err(TaskMenuError::Init(err));
            }

            Ok(result.result as ObjectId)
        }
    }
}

#[allow(unexpected_cfgs, static_mut_refs)]
impl TaskMenuOperations for TaskMenuBar {
    fn set_quit_button(&self) -> Result<(), TaskMenuError> {
        unsafe {
            let result = catch_and_log_exception(
                |args| {
                    let menu = args as ObjectId;
                    let quit_item: ObjectId = msg_send![class!(NSMenuItem), new];
                    let quit_title = TaskMenuBar::string_to_nsstring("Quit").unwrap();

                    let _: () = msg_send![quit_item, setTitle: quit_title];

                    // Set the quit action
                    let _: () = msg_send![quit_item, setAction: sel!(stop:)];
                    let _: () = msg_send![menu, addItem: quit_item];
                    ptr::null_mut::<std::ffi::c_void>()
                },
                self.menu_ref as *mut std::ffi::c_void,
            );
            if !result.error.is_null() {
                let err = get_error(result.error as ObjectId);
                return Err(TaskMenuError::Unexpected(err));
            }
            Ok(())
        }
    }

    fn add_menu_item(
        &self,
        btn_data: ButtonData,
        on_click: CallbackFn,
    ) -> Result<(), TaskMenuError> {
        unsafe {
            let btn_title = btn_data.btn_title.clone();
            let boxed: Box<(String, [ObjectId; 2])> =
                Box::new((btn_title.clone(), [self.menu_ref, self.app_delegate]));

            let result = catch_and_log_exception(
                |args| {
                    let ptr_arr = Box::from_raw(args as *mut (String, [ObjectId; 2]));
                    let btn_title = ptr_arr.0;
                    let menu = ptr_arr.1[0];
                    let app_delegate = ptr_arr.1[1];

                    let item_title = TaskMenuBar::string_to_nsstring(&btn_title).unwrap();
                    let menu_item: ObjectId = msg_send![class!(NSMenuItem), new];

                    let _: () = msg_send![menu_item, setTitle: item_title];

                    // Set target & action

                    let _: () = msg_send![menu_item, setTarget: app_delegate];
                    let _: () = msg_send![menu_item, setAction: sel!(menu_item_clicked:)];

                    // Add item to menu
                    let _: () = msg_send![menu, addItem: menu_item];
                    ptr::null_mut::<std::ffi::c_void>()
                },
                Box::into_raw(boxed) as *mut std::ffi::c_void,
            );
            if !result.error.is_null() {
                let err = get_error(result.error as ObjectId);
                return Err(TaskMenuError::Unexpected(err));
            }

            if let Ok(mut h_map) = attempt_get_lock(&self.handlers) {
                h_map.insert(btn_title.clone(), on_click);
            };
            if let Ok(mut h_map) = attempt_get_lock(&self.title_to_btn_data) {
                h_map.insert(btn_title.clone(), btn_data);
            };

            Ok(())
        }
    }

    fn init() -> Result<Self, TaskMenuError> {
        unsafe {
            let result = catch_and_log_exception(
                |_| {
                    let app: ObjectId = msg_send![class!(NSApplication), sharedApplication];
                    let _: () = msg_send![app, setActivationPolicy: 2]; // Hide Dock icon (Accessory mode)
                                                                        //

                    let system_status_bar: ObjectId =
                        msg_send![class!(NSStatusBar), systemStatusBar];

                    let status_item: ObjectId =
                        msg_send![system_status_bar, statusItemWithLength: -1.0];
                    let title = TaskMenuBar::string_to_nsstring("🔔").unwrap();

                    let button: ObjectId = msg_send![status_item, button];
                    let _: () = msg_send![button, setTitle: title];

                    // Create a dynamic menu
                    let menu: ObjectId = msg_send![class!(NSMenu), new];

                    // Attach the menu to the status item
                    let _: () = msg_send![status_item, setMenu: menu];

                    let boxed: Box<[ObjectId; 2]> = Box::new([app, menu]);

                    Box::into_raw(boxed) as *mut std::ffi::c_void
                },
                ptr::null_mut::<std::ffi::c_void>(),
            );
            if result.result.is_null() && !result.error.is_null() {
                let err = get_error(result.error as ObjectId);
                return Err(TaskMenuError::Init(err));
            }

            let refs_arr = Box::from_raw(result.result as *mut [ObjectId; 2]);
            if refs_arr.is_empty() {
                return Err(TaskMenuError::Init("App was not instantiated".to_string()));
            }
            let app_ref = refs_arr[0];
            let menu_ref = refs_arr[1];

            if app_ref.is_null() {
                return Err(TaskMenuError::Init("App was not instantiated".to_string()));
            }

            if menu_ref.is_null() {
                return Err(TaskMenuError::Init("Menu was not instantiated".to_string()));
            }

            let handlers = Rc::new(Mutex::new(HashMap::new()));
            let title_to_btn_data = Rc::new(Mutex::new(HashMap::new()));
            let app_delegate =
                TaskMenuBar::create_app_delegate(handlers.clone(), title_to_btn_data.clone())?;

            if app_delegate.is_null() {
                return Err(TaskMenuError::Init(
                    "App delegate was not instantiated".to_string(),
                ));
            }

            Ok(TaskMenuBar {
                menu_ref,
                app_ref,
                app_delegate,
                handlers,
                title_to_btn_data,
            })
        }
    }
    fn remove_menu_item(&self, btn_title: String) -> Result<(), TaskMenuError> {
        unsafe {
            let boxed: Box<(String, ObjectId)> = Box::new((btn_title.clone(), self.menu_ref));
            let res = catch_and_log_exception(
                |args| {
                    let args_union = Box::from_raw(args as *mut (String, ObjectId));
                    let (title, menu_ref) = *args_union;
                    let button_item: ObjectId = msg_send![menu_ref, itemWithTitle: title];
                    let _: () = msg_send![menu_ref, removeItem: button_item];
                    args
                },
                Box::into_raw(boxed) as *mut std::ffi::c_void,
            );
            if let Ok(mut h_map) = attempt_get_lock(&self.handlers) {
                h_map.remove(&btn_title);
            };
            if let Ok(mut h_map) = attempt_get_lock(&self.title_to_btn_data) {
                h_map.remove(&btn_title);
            };

            if !res.error.is_null() {
                let err = get_error(res.error as ObjectId);
                Err(TaskMenuError::Init(err))
            } else {
                Ok(())
            }
        }
    }

    fn run(&self) -> Result<(), TaskMenuError> {
        unsafe {
            let res = catch_and_log_exception(
                |app_ref| {
                    let _: () = msg_send![app_ref as ObjectId, run];
                    app_ref
                },
                self.app_ref as *mut std::ffi::c_void,
            );
            if !res.error.is_null() {
                let err = get_error(res.error as ObjectId);
                Err(TaskMenuError::Init(err))
            } else {
                Ok(())
            }
        }
    }
}

// SAFETY: We manually implement Send/Sync because object pointers will
// only be used on one thread at a time.
unsafe impl Send for TaskMenuBar {}
unsafe impl Sync for TaskMenuBar {}
