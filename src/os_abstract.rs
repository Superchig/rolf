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

use std::{env::{self, VarError}, path::{PathBuf, Path}};

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

fn env_or_dir<K: AsRef<Path>>(env_var: &str, alt_env_base: &str, alt_join_path: K) -> PathBuf {
    match env::var(env_var) {
        Ok(data_dir) => PathBuf::from(data_dir),
        Err(VarError::NotPresent) => {
            let mut result = PathBuf::from(env::var(alt_env_base).unwrap());
            result.push(alt_join_path);
            result
        },
        Err(_) => panic!("Unable to read data directory"),
    }
}
