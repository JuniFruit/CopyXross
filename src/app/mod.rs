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

struct ButtonFullData {
    handler: CallbackFn,
    btn_data: ButtonData,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ButtonData {
    pub btn_title: String,
    pub attrs_str: Option<String>,
    pub is_static: bool,
    pub index: Option<usize>,
}
impl ButtonData {
    pub fn from_str_static(input: &str) -> Self {
        ButtonData {
            btn_title: input.to_string(),
            attrs_str: None,
            is_static: true,
            index: Some(0),
        }
    }
    pub fn from_str_dyn(input: &str) -> Self {
        ButtonData {
            btn_title: input.to_string(),
            attrs_str: None,
            is_static: false,
            index: Some(0),
        }
    }
}
pub type Event<'a> = Option<&'a ButtonData>;
pub type CallbackFn = Box<dyn Fn(Event)>;

pub trait TaskMenuOperations: Sized + Sync + Send {
    fn init() -> Result<Self, TaskMenuError>;
    fn add_menu_item(
        &self,
        btn_data: ButtonData,
        on_click: CallbackFn,
    ) -> Result<(), TaskMenuError>;
    fn set_quit_button(&self) -> Result<(), TaskMenuError>;
    fn remove_menu_item(&self, btn_data: ButtonData) -> Result<(), TaskMenuError>;
    fn remove_all_dyn(&self) -> Result<(), TaskMenuError>;
    fn stop(&self) -> Result<(), TaskMenuError>;
    fn run(&self) -> Result<(), TaskMenuError>;
    fn set_autorun_button(&self) -> Result<(), TaskMenuError>;
}

#[cfg(target_os = "macos")]
use macos::TaskMenuBar as PlatformTaskBar;
#[cfg(target_os = "windows")]
use windows::TaskMenuBar as PlatformTaskBar;

pub fn init_taskmenu() -> Result<impl TaskMenuOperations, TaskMenuError> {
    let bar = PlatformTaskBar::init()?;
    bar.set_autorun_button()?;
    bar.set_quit_button()?;
    Ok(bar)
}
