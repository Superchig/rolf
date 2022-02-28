use crate::WindowPixels;
// use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use std::io;

use std::fs::Metadata;
use std::os::windows::fs::MetadataExt;

use super::ExtraPermissions;

// FIXME(Chris): Actually implement correctly
pub fn get_extra_perms(metadata: &Metadata) -> ExtraPermissions {
    // let naive = NaiveDateTime::from_timestamp(
    //     metadata.mtime(),
    //     27, // Apparently 27 leap seconds have passed since 1972
    // );

    // let date_time: DateTime<Local> =
    //     DateTime::from_utc(naive, Local.offset_from_local_datetime(&naive).unwrap());

    let mode = {
        let mut result = String::new();

        let perms = metadata.permissions();
        let file_attributes = metadata.file_attributes();

        if metadata.is_dir() {
            result.push_str("d-");
        } else if metadata.is_file() {
            result.push_str("-a");
        }

        if perms.readonly() {
            result.push('r');
        } else {
            result.push('-');
        }

        // Check if file is hidden
        // https://docs.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants
        if file_attributes & 2 != 0 {
            result.push('h');
        } else {
            result.push('-');
        }

        // Check if file is used exclusively by operating system
        if file_attributes & 4 != 0 {
            result.push('s');
        } else {
            result.push('-');
        }

        if metadata.is_symlink() {
            result.push('l');
        } else {
            result.push('-');
        }

        result
    };

    ExtraPermissions {
        mode,
        user_name: None,
        group_name: None,
        hard_link_count: None,
        size: None,
        modify_date_time: None,
    }
}

// FIXME(Chris): Actually implement correctly
// A possibly-safe wrapper around an ioctl call with TIOCGWINSZ.
// Gets the width and height of the terminal in pixels.
pub fn get_win_pixels() -> std::result::Result<WindowPixels, io::Error> {
    // let win_pixels = unsafe {
    //     let mut winsize = libc::winsize {
    //         ws_col: 0,
    //         ws_row: 0,
    //         ws_xpixel: 0,
    //         ws_ypixel: 0,
    //     };

    //     // NOTE(Chris): From Linux's man ioctl_tty
    //     const TIOCGWINSZ: u64 = libc::TIOCGWINSZ;

    //     // NOTE(Chris): This only works if stdin is a tty. If it is not (e.g. zsh widgets), then
    //     // you may have to redirect the tty to stdin.
    //     // Example:
    //     // rf() { rolf < $TTY }
    //     let err = libc::ioctl(libc::STDIN_FILENO, TIOCGWINSZ, &mut winsize);
    //     if err != 0 {
    //         let errno_location = libc::__errno_location();
    //         let errno = (*errno_location) as i32;

    //         return Err(io::Error::from_raw_os_error(errno));

    //         // panic!("Failed to get the size of terminal window in pixels.");
    //     }

    //     WindowPixels {
    //         width: winsize.ws_xpixel,
    //         height: winsize.ws_ypixel,
    //     }
    // };

    // Ok(win_pixels)

    Ok(WindowPixels {
        width: 100,
        height: 100,
    })
}

pub fn get_home_name() -> String {
    std::env::var("USERPROFILE").unwrap()
}
