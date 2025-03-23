#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[allow(dead_code)]
#[derive(Debug)]
pub enum TaskMenuError {
    Init(String),
    Unexpected(String),
}

pub type Event = Option<String>;
pub type CallbackFn = Box<dyn Fn(Event)>;

pub trait TaskMenuOperations: Sized + Sync + Send {
    fn init() -> Result<Self, TaskMenuError>;
    fn add_menu_item(&self, btn_title: String, on_click: CallbackFn) -> Result<(), TaskMenuError>;
    fn set_quit_button(&self) -> Result<(), TaskMenuError>;
    fn remove_menu_item(&self, btn_title: String) -> Result<(), TaskMenuError>;
    fn run(&self) -> Result<(), TaskMenuError>;
}

#[cfg(target_os = "macos")]
use macos::TaskMenuBar as PlatformTaskBar;
#[cfg(target_os = "windows")]
use windows::TaskMenuBar as PlatformTaskBar;

pub fn init_taskmenu() -> Result<impl TaskMenuOperations, TaskMenuError> {
    let bar = PlatformTaskBar::init()?;
    bar.add_menu_item(
        "About".to_string(),
        Box::new(move |e| println!("Clicked on {:?}", e)),
    )?;
    bar.set_quit_button()?;
    Ok(bar)
}
