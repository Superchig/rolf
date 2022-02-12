#[cfg(unix)]
mod unix;

#[cfg(unix)]
// pub use self::unix::get_strmode;
pub use self::unix::*;
