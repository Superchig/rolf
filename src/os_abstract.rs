#[cfg(unix)]
mod unix;

#[cfg(unix)]
// pub use self::unix::get_strmode;
pub use self::unix::*;

pub struct ExtraPermissions {
    pub mode: String, // The mode string "drwxr-xr-x"
    pub user_name: Option<String>,
    pub group_name: Option<String>,
    pub hard_link_count: Option<u64>,
    pub size: Option<u64>,
    pub modify_date_time: Option<String>
}
