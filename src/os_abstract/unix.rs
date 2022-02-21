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
            let errno_location = libc::__errno_location();
            let errno = (*errno_location) as i32;

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

// Should get the hostname in a POSIX-compliant way.
// Only tested on Linux, however.
pub fn get_hostname() -> io::Result<String> {
    unsafe {
        // NOTE(Chris): HOST_NAME_MAX is defined in bits/local_lim.h on Linux

        let host_name_max: usize = libc::sysconf(libc::_SC_HOST_NAME_MAX) as usize;

        // HOST_NAME_MAX can't be larger than 256 on POSIX systems
        let mut name_buf = [0; 256];

        let err = libc::gethostname(name_buf.as_mut_ptr(), host_name_max);
        match err {
            0 => {
                // Make sure that at least the last character is NUL
                name_buf[host_name_max - 1] = 0;

                let null_position = name_buf.iter().position(|byte| *byte == 0).unwrap();

                let name_u8 = { &*(&mut name_buf[..] as *mut [i8] as *mut [u8]) };

                Ok(std::str::from_utf8(&name_u8[0..null_position])
                    .unwrap()
                    .to_string())
            }
            1 => {
                let errno_location = libc::__errno_location();
                let errno = (*errno_location) as i32;

                Err(io::Error::from_raw_os_error(errno))
            }
            _ => {
                panic!("Invalid libc:gethostname return value: {}", err);
            }
        }
    }
}

pub fn get_home_name() -> String {
    std::env::var("HOME").unwrap()
}

pub fn get_user_name() -> String {
    std::env::var("USER").unwrap()
}
