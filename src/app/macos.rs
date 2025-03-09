use std::collections::HashMap;
use std::ffi::c_void;
use std::ffi::CStr;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::Mutex;

use objc::class;
use objc::msg_send;
use objc::runtime::Object;
use objc::runtime::Sel;
use objc::sel;
use objc::sel_impl;

use crate::utils::attempt_get_lock;
use crate::utils::catch_and_log_exception;
use crate::utils::logger;

use super::CallbackFn;
use super::TaskMenuError;
use super::TaskMenuOperations;

static mut MENU_PTR: Option<Mutex<ObjectId>> = None;
static mut APP_PTR: Option<Mutex<ObjectId>> = None;

type ObjectId = *mut Object;
pub type ClickEvent = ObjectId;

#[allow(unexpected_cfgs)]
extern "C" fn menu_item_clicked(this: &Object, _cmd: Sel, sender: ObjectId) {
    println!("Test");
    // unsafe {
    //     let title: ObjectId = msg_send![sender, title];
    //     let cstr: *const i8 = msg_send![title, UTF8String];
    //
    //     if !title.is_null() && !cstr.is_null() {
    //         let handlers_ptr = *this.get_ivar::<*mut c_void>("handlers_ptr")
    //             as *mut Arc<Mutex<HashMap<String, CallbackFn>>>;
    //         let key = CStr::from_ptr(cstr).to_string_lossy().to_string();
    //         if !handlers_ptr.is_null() {
    //             let h_map = handlers_ptr.as_ref();
    //
    //             if let Some(handlers) = h_map {
    //                 attempt_get_lock(handlers, |m| {
    //                     let handler = m.get(&key);
    //                     if let Some(cb) = handler {
    //                         cb()
    //                     }
    //                 });
    //             }
    //         }
    //     }
    // }
}

pub struct TaskMenuBar {
    menu_ref: ObjectId,
    app_ref: ObjectId,
    app_delegate: ObjectId,
    // cp_submenu_ref: ObjectId,
    handlers: Arc<Mutex<HashMap<String, CallbackFn>>>,
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
        handlers: Arc<Mutex<HashMap<String, CallbackFn>>>,
    ) -> Result<ObjectId, TaskMenuError> {
        unsafe {
            let superclass = class!(NSObject);
            let cls_name = "AppDelegate";
            let decl = objc::declare::ClassDecl::new(cls_name, superclass);

            if decl.is_none() {
                return Err(TaskMenuError::Init(
                    "Failed to declare AppDelegate class".to_string(),
                ));
            }

            let mut decl = decl.unwrap();

            decl.add_ivar::<ObjectId>("handlers_ptr");

            // Dynamic handler for menu clicks
            decl.add_method(
                sel!(menu_item_clicked:),
                menu_item_clicked as extern "C" fn(&Object, Sel, *mut Object),
            );

            let new_class = decl.register();
            let delegate_obj: ObjectId = msg_send![new_class, new]; // Create instance of AppDelegate

            if delegate_obj.is_null() {
                return Err(TaskMenuError::Init(
                    "Failed to instantiate AppDelegate".to_string(),
                ));
            }
            let handlers_ptr: *mut Mutex<HashMap<String, CallbackFn>> =
                Arc::into_raw(handlers.clone()) as *mut _;
            (*delegate_obj).set_ivar("handlers_ptr", handlers_ptr as ObjectId);
            Ok(delegate_obj)
        }
    }
}

#[allow(unexpected_cfgs, static_mut_refs)]
impl TaskMenuOperations for TaskMenuBar {
    fn set_menu_items(&self, items: Vec<String>) -> Result<(), TaskMenuError> {
        todo!()
    }

    fn set_quit_button(&self) -> Result<(), TaskMenuError> {
        unsafe {
            let p = self.app_ref;
            let menu = self.menu_ref;
            if p.is_null() || menu.is_null() {
                return Err(TaskMenuError::Unexpected(
                    "Failed to set quit btn: Pointer is null".to_string(),
                ));
            }
            let quit_item: ObjectId = msg_send![class!(NSMenuItem), new];
            if quit_item.is_null() {
                return Err(TaskMenuError::Unexpected(
                    "Menu item was not instantiated".to_string(),
                ));
            }
            let quit_title = TaskMenuBar::string_to_nsstring("Quit")?;

            let _: () = msg_send![quit_item, setTitle: quit_title];

            // Set the quit action
            let _: () = msg_send![quit_item, setAction: sel!(terminate:)];
            let _: () = msg_send![menu, addItem: quit_item];
            Ok(())
        }
    }

    fn add_menu_item(&self, btn_title: String, on_click: CallbackFn) -> Result<(), TaskMenuError> {
        unsafe {
            let menu = self.menu_ref;
            if menu.is_null() {
                return Err(TaskMenuError::Unexpected("Menu ref is null".to_string()));
            }

            let item_title = TaskMenuBar::string_to_nsstring(&btn_title)?;
            let menu_item: ObjectId = msg_send![class!(NSMenuItem), new];

            if menu_item.is_null() {
                return Err(TaskMenuError::Init(
                    "Menu item failed to instantiate".to_string(),
                ));
            }

            let _: () = msg_send![menu_item, setTitle: item_title];

            // Store callback in HANDLERS map
            // HANDLERS.lock().unwrap().insert(title.to_string(), callback);

            // let cb_pointer: fn() = Box::into_raw(Box::new(on_click));
            // let handlers = self.handlers.insert(btn_title, cb_pointer);

            // Set target & action
            let app_delegate = self.app_delegate;
            if app_delegate.is_null() {
                return Err(TaskMenuError::Unexpected("AppDelegate is null".to_string()));
            }

            let _: () = msg_send![menu_item, setTarget: app_delegate];
            let _: () = msg_send![menu_item, setAction: sel!(menu_item_clicked:)];

            // Add item to menu
            let _: () = msg_send![menu, addItem: menu_item];

            Ok(())
        }
    }

    fn init() -> Result<Self, TaskMenuError> {
        unsafe {
            catch_and_log_exception(logger, move || {
                let app: ObjectId = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, setActivationPolicy: 2]; // Hide Dock icon (Accessory mode)
                                                                    //

                let system_status_bar: ObjectId = msg_send![class!(NSStatusBar), systemStatusBar];

                let status_item: ObjectId =
                    msg_send![system_status_bar, statusItemWithLength: -1.0];
                let title = TaskMenuBar::string_to_nsstring("🔔").unwrap();

                let button: ObjectId = msg_send![status_item, button];
                let _: () = msg_send![button, setTitle: title];

                // Create a dynamic menu
                let menu: ObjectId = msg_send![class!(NSMenu), new];

                // Attach the menu to the status item
                let _: () = msg_send![status_item, setMenu: menu];

                let mutex = &mut MENU_PTR;

                *mutex = Some(Mutex::new(menu));
                let app_mutex = &mut APP_PTR;
                *app_mutex = Some(Mutex::new(app));
            });

            if APP_PTR.is_none() {
                return Err(TaskMenuError::Init("App was not instantiated".to_string()));
            }

            let app_ref = APP_PTR.as_ref().unwrap();
            let app_ref = app_ref
                .lock()
                .map_err(|err| TaskMenuError::Init(format!("App lock failed: {:?}", err)))?;
            let app_ref = app_ref.cast() as ObjectId;

            if MENU_PTR.is_none() {
                return Err(TaskMenuError::Init("Menu was not instantiated".to_string()));
            }
            let menu_ref = MENU_PTR.as_ref().unwrap();
            let menu_ref = menu_ref
                .lock()
                .map_err(|err| TaskMenuError::Init(format!("Menu lock failed: {:?}", err)))?;
            let menu_ref = menu_ref.cast() as ObjectId;

            let handlers_map = Arc::new(Mutex::new(HashMap::new()));

            let app_delegate = TaskMenuBar::create_app_delegate(handlers_map.clone())?;

            Ok(TaskMenuBar {
                menu_ref,
                app_ref,
                app_delegate,
                handlers: handlers_map,
            })
        }
    }

    fn run(&self) -> Result<(), TaskMenuError> {
        unsafe {
            if !self.app_ref.is_null() {
                let _: () = msg_send![self.app_ref, run];
                Ok(())
            } else {
                Err(TaskMenuError::Init(
                    "Failed to run app. Pointer is null".to_string(),
                ))
            }
        }
    }
}

// SAFETY: We manually implement Send/Sync because object pointers will
// only be used on one thread at a time.
unsafe impl Send for TaskMenuBar {}
unsafe impl Sync for TaskMenuBar {}
