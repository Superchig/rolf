#[cfg(unix)]
mod unix;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
// pub use self::unix::get_strmode;
pub use self::unix::*;
#[cfg(target_os = "linux")]
pub use self::linux::*;
#[cfg(target_os = "macos")]
pub use self::macos::*;
#[cfg(windows)]
pub use self::windows::*;

pub struct ExtraPermissions {
    pub mode: String, // The mode string "drwxr-xr-x"
    pub user_name: Option<String>,
    pub group_name: Option<String>,
    pub hard_link_count: Option<u64>,
    pub size: Option<u64>,
    pub modify_date_time: Option<String>
}

#[derive(Debug, Clone, Copy)]
pub struct WindowPixels {
    pub width: u16,
    pub height: u16,
}
