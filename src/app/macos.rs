use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::ffi::CString;
use std::path::PathBuf;
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
use crate::utils::get_asset_path;
use crate::utils::macos::catch_and_log_exception;
use crate::utils::macos::get_error;
use crate::utils::macos::ObjectId;

use super::ButtonData;
use super::ButtonFullData;
use super::CallbackFn;
use super::TaskMenuError;
use super::TaskMenuOperations;

#[allow(dead_code)]
pub struct TaskMenuBar {
    menu_ref: ObjectId,
    app_ref: ObjectId,
    handlers: Rc<Mutex<HashMap<usize, ButtonFullData>>>,
    id_counter: RefCell<usize>,
}

#[allow(unexpected_cfgs)]
impl TaskMenuBar {
    #[allow(unexpected_cfgs)]
    extern "C" fn menu_item_clicked(this: &Object, _cmd: Sel, sender: ObjectId) {
        unsafe {
            autoreleasepool(|| {
                if sender.is_null() {
                    debug_println!("Sender is null");
                    return;
                }
                let id_ns_str: ObjectId = msg_send![sender, representedObject];
                let cstr: *const i8 = msg_send![id_ns_str, UTF8String];

                if cstr.is_null() {
                    return;
                }
                let handlers_ptr = *this.get_ivar::<ObjectId>("handlers_ptr");

                if handlers_ptr.is_null() {
                    debug_println!("Handlers pointer is null");
                    return;
                }

                let handlers_ptr: &Mutex<HashMap<usize, ButtonFullData>> =
                    &*(handlers_ptr as *mut Mutex<HashMap<usize, ButtonFullData>>);
                let key = &CStr::from_ptr(cstr)
                    .to_string_lossy()
                    .to_string()
                    .parse::<usize>();

                if let Ok(key) = key {
                    if let Ok(h_map) = attempt_get_lock(handlers_ptr) {
                        let f_data = h_map.get(key);
                        if let Some(data) = f_data {
                            let handler = data.handler.as_ref();
                            handler(Some(&data.btn_data));
                        }
                    };
                }
            })
        }
    }

    extern "C" fn menu_will_open(_: &Object, _: Sel, menu: ObjectId) {
        unsafe {
            // pass menu ref
            // it contains app delegate, so it will be enough to access all pointers
            let res = catch_and_log_exception(
                |args| {
                    let menu = args as ObjectId;

                    let delegate: &Object = msg_send![menu, delegate];
                    let handlers_ptr: ObjectId = *delegate.get_ivar("handlers_ptr");
                    let handlers = &*(handlers_ptr as *mut Mutex<HashMap<usize, ButtonFullData>>);

                    if let Ok(handlers) = attempt_get_lock(handlers) {
                        let entries = handlers.keys();

                        for item in entries {
                            let data = handlers.get(item).unwrap();
                            let btn_title = &data.btn_data.btn_title;
                            let item_title = TaskMenuBar::string_to_nsstring(btn_title);
                            let item_id_nsstr = TaskMenuBar::string_to_nsstring(&item.to_string());
                            let menu_item: ObjectId = msg_send![class!(NSMenuItem), new];

                            let _: () = msg_send![menu_item, setTitle: item_title];
                            let _: () = msg_send![menu_item, setRepresentedObject: item_id_nsstr];

                            // Set target & action

                            let _: () = msg_send![menu_item, setTarget: delegate];
                            let _: () = msg_send![menu_item, setAction: sel!(menu_item_clicked:)];

                            // Add item to menu
                            if data.btn_data.is_static {
                                let _: () = msg_send![menu, addItem: menu_item];
                            } else {
                                let _: () = msg_send![menu, insertItem: menu_item atIndex: 0];
                            }
                        }
                    }

                    ptr::null_mut::<std::ffi::c_void>()
                },
                menu as *mut _,
            );

            if !res.error.is_null() {
                debug_println!("{:?}", get_error(res.error as ObjectId));
            }
        }
    }

    extern "C" fn menu_did_close(_: &Object, _: Sel, menu: ObjectId) {
        unsafe {
            let res = catch_and_log_exception(
                |args| {
                    autoreleasepool(|| {
                        let menu = args as ObjectId;
                        let _: () = msg_send![menu, removeAllItems];
                        ptr::null_mut()
                    })
                },
                menu as *mut _,
            );
            if !res.error.is_null() {
                debug_println!("{:?}", get_error(res.error as ObjectId));
            }
        }
    }

    fn string_to_nsstring(str: &str) -> ObjectId {
        unsafe {
            let c_str = CString::new(str).unwrap();
            let res: ObjectId = msg_send![class!(NSString), stringWithUTF8String: c_str.as_ptr()];
            res
        }
    }

    fn find_btn_id_to_remove(&self, btn_data: ButtonData) -> Result<Option<usize>, TaskMenuError> {
        let h_map = attempt_get_lock(&self.handlers)
            .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;
        let mut id: Option<usize> = None;
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

    fn increment_btn_id(&self) -> Result<usize, TaskMenuError> {
        let new_id_counter = *self.id_counter.try_borrow().map_err(|err| {
            TaskMenuError::Unexpected(format!("Cant increment btn_id: {:?}", err))
        })? + 1;
        self.id_counter.replace(new_id_counter);
        Ok(new_id_counter)
    }
    fn create_app_delegate(
        handlers: Rc<Mutex<HashMap<usize, ButtonFullData>>>,
    ) -> Result<ObjectId, TaskMenuError> {
        unsafe {
            let raw_handlers = Rc::into_raw(handlers) as *mut std::ffi::c_void;
            let result = catch_and_log_exception(
                |args| {
                    let handlers_ptr = args as *mut Mutex<HashMap<usize, ButtonFullData>>;
                    let superclass = class!(NSObject);
                    let cls_name = "AppDelegate";
                    let decl = objc::declare::ClassDecl::new(cls_name, superclass);

                    let mut decl = decl.unwrap();

                    decl.add_ivar::<ObjectId>("handlers_ptr");

                    // Dynamic handler for menu clicks
                    decl.add_method(
                        sel!(menu_item_clicked:),
                        TaskMenuBar::menu_item_clicked as extern "C" fn(&Object, Sel, *mut Object),
                    );
                    decl.add_method(
                        sel!(menuWillOpen:),
                        TaskMenuBar::menu_will_open as extern "C" fn(&Object, Sel, *mut Object),
                    );
                    decl.add_method(
                        sel!(menuDidClose:),
                        TaskMenuBar::menu_did_close as extern "C" fn(&Object, Sel, *mut Object),
                    );

                    let new_class = decl.register();
                    let delegate_obj: ObjectId = msg_send![new_class, new]; // Create instance of AppDelegate

                    (*delegate_obj).set_ivar("handlers_ptr", handlers_ptr as ObjectId);
                    delegate_obj as *mut std::ffi::c_void
                },
                raw_handlers as *mut _,
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
        let app_ref = self.app_ref;
        self.add_menu_item(
            ButtonData::from_str_static("Quit"),
            Box::new(move |_| unsafe {
                let res = catch_and_log_exception(
                    |args| {
                        let app = args as ObjectId;
                        let _: () = msg_send![app, stop: app];
                        ptr::null_mut::<std::ffi::c_void>()
                    },
                    app_ref as *mut _,
                );
                if !res.error.is_null() {
                    panic!("{:?}", get_error(res.error as ObjectId));
                }
            }),
        )
    }

    fn add_menu_item(
        &self,
        btn_data: ButtonData,
        on_click: CallbackFn,
    ) -> Result<(), TaskMenuError> {
        let btn_id = self.increment_btn_id()?;

        let mut h_map = attempt_get_lock(&self.handlers)
            .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;
        h_map.insert(
            btn_id,
            ButtonFullData {
                handler: on_click,
                btn_data,
            },
        );

        Ok(())
    }

    fn init() -> Result<Self, TaskMenuError> {
        unsafe {
            let handlers: Rc<Mutex<HashMap<usize, ButtonFullData>>> =
                Rc::new(Mutex::new(HashMap::new()));
            let app_delegate = TaskMenuBar::create_app_delegate(handlers.clone())?;

            let result = catch_and_log_exception(
                |args| {
                    let delegate = args as ObjectId;
                    let app: ObjectId = msg_send![class!(NSApplication), sharedApplication];
                    let _: () = msg_send![app, setActivationPolicy: 2]; // Hide Dock icon (Accessory mode)
                                                                        //

                    let system_status_bar: ObjectId =
                        msg_send![class!(NSStatusBar), systemStatusBar];

                    let status_item: ObjectId =
                        msg_send![system_status_bar, statusItemWithLength: -1.0];
                    let path = get_asset_path("24.png").unwrap_or(PathBuf::from(""));
                    let image_path = TaskMenuBar::string_to_nsstring(path.to_str().unwrap());
                    let image: ObjectId = msg_send![class!(NSImage), alloc];
                    let image: ObjectId = msg_send![image, initWithContentsOfFile: image_path];

                    let button: ObjectId = msg_send![status_item, button];
                    let _: () = msg_send![button, setImagePosition: 2];
                    let _: () = msg_send![button, setBordered: true];
                    let _: () = msg_send![button, setImage: image];

                    // Create a dynamic menu
                    let menu: ObjectId = msg_send![class!(NSMenu), new];

                    // Attach the menu to the status item
                    let _: () = msg_send![status_item, setMenu: menu];
                    let _: () = msg_send![menu, setDelegate: delegate];

                    let boxed: Box<[ObjectId; 2]> = Box::new([app, menu]);

                    Box::into_raw(boxed) as *mut std::ffi::c_void
                },
                app_delegate as *mut _,
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

            Ok(TaskMenuBar {
                menu_ref,
                app_ref,
                handlers,
                id_counter: RefCell::new(0),
            })
        }
    }
    fn remove_menu_item(&self, btn_data: ButtonData) -> Result<(), TaskMenuError> {
        let id = self.find_btn_id_to_remove(btn_data)?;
        let mut h_map = attempt_get_lock(&self.handlers)
            .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;

        if let Some(id) = id {
            h_map.remove(&id);

            Ok(())
        } else {
            Err(TaskMenuError::Unexpected(
                "Could not find button to delete!".to_string(),
            ))
        }
    }

    fn remove_all_dyn(&self) -> Result<(), TaskMenuError> {
        let mut h_map = attempt_get_lock(&self.handlers)
            .map_err(|err| TaskMenuError::Unexpected(format!("{:?}", err)))?;
        h_map.retain(|_, v| v.btn_data.is_static);

        Ok(())
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
    fn stop(&self) -> Result<(), TaskMenuError> {
        unsafe {
            let res = catch_and_log_exception(
                |app| {
                    let app = app as ObjectId;
                    let _: () = msg_send![app, stop: app];
                    ptr::null_mut()
                },
                self.app_ref as *mut _,
            );
            if !res.error.is_null() {
                Err(TaskMenuError::Unexpected(get_error(res.error as ObjectId)))
            } else {
                Ok(())
            }
        }
    }
    fn set_autorun_button(&self) -> Result<(), TaskMenuError> {
        Ok(())
    }
}

// SAFETY: We manually implement Send/Sync because object pointers will
// only be used on one thread at a time.
unsafe impl Send for TaskMenuBar {}
unsafe impl Sync for TaskMenuBar {}
