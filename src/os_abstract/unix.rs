// This module implements functions that should work on both macOS and Linux

use crate::WindowPixels;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use std::io;

use crate::strmode;
use crate::unix_users;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;

use super::ExtraPermissions;

use libc::c_int;

pub fn get_strmode(metadata: &Metadata) -> String {
    let permissions = metadata.permissions();

    strmode(permissions.mode())
}

pub fn get_extra_perms(metadata: &Metadata) -> ExtraPermissions {
    let naive = NaiveDateTime::from_timestamp(
        metadata.mtime(),
        27, // Apparently 27 leap seconds have passed since 1972
    );

    let date_time: DateTime<Local> =
        DateTime::from_utc(naive, Local.offset_from_local_datetime(&naive).unwrap());

    ExtraPermissions {
        mode: get_strmode(metadata),
        user_name: unix_users::get_unix_groupname(metadata.gid()),
        group_name: unix_users::get_unix_username(metadata.uid()),
        hard_link_count: Some(metadata.nlink()),
        size: Some(metadata.size()),
        modify_date_time: Some(date_time.format("%c").to_string()),
    }
}

// A possibly-safe wrapper around an ioctl call with TIOCGWINSZ.
// Gets the width and height of the terminal in pixels.
pub fn get_win_pixels() -> std::result::Result<WindowPixels, io::Error> {
    let win_pixels = unsafe {
        let mut winsize = libc::winsize {
            ws_col: 0,
            ws_row: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // NOTE(Chris): From Linux's man ioctl_tty
        const TIOCGWINSZ: u64 = libc::TIOCGWINSZ;

        // NOTE(Chris): This only works if stdin is a tty. If it is not (e.g. zsh widgets), then
        // you may have to redirect the tty to stdin.
        // Example:
        // rf() { rolf < $TTY }
        let err = libc::ioctl(libc::STDIN_FILENO, TIOCGWINSZ, &mut winsize);
        if err != 0 {
            let errno = errno();

            return Err(io::Error::from_raw_os_error(errno));

            // panic!("Failed to get the size of terminal window in pixels.");
        }

        WindowPixels {
            width: winsize.ws_xpixel,
            height: winsize.ws_ypixel,
        }
    };

    Ok(win_pixels)
}

pub fn get_home_name() -> String {
    std::env::var("HOME").unwrap()
}

unsafe fn errno() -> i32 {
    let errno_location = errno_location();
    (*errno_location) as i32
}

extern "C" {
    #[cfg_attr(target_os = "macos", link_name = "__error")]
    #[cfg_attr(target_os = "linux", link_name = "__errno_location")]
    fn errno_location() -> *mut c_int;
}
