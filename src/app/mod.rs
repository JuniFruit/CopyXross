#[cfg(target_os = "macos")]
mod macos;

#[allow(dead_code)]
#[derive(Debug)]
pub enum TaskMenuError {
    Init(String),
    Unexpected(String),
}

type CallbackFn = Box<dyn Fn()>;

pub trait TaskMenuOperations: Sized + Sync + Send {
    fn init() -> Result<Self, TaskMenuError>;
    fn set_menu_items(&self, items: Vec<String>) -> Result<(), TaskMenuError>;
    fn add_menu_item(&self, btn_title: String, on_click: CallbackFn) -> Result<(), TaskMenuError>;
    fn set_quit_button(&self) -> Result<(), TaskMenuError>;
    fn run(&self) -> Result<(), TaskMenuError>;
}

#[cfg(target_os = "macos")]
use macos::TaskMenuBar as PlatformTaskBar;

pub fn init_taskmenu() -> Result<impl TaskMenuOperations, TaskMenuError> {
    let bar = PlatformTaskBar::init()?;
    bar.set_quit_button()?;
    bar.add_menu_item(
        "About".to_string(),
        Box::new(move || println!("Clicked on about")),
    )?;
    Ok(bar)
}
