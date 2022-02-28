use windows::Win32::Foundation::{BOOL, FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::FileTimeToSystemTime;

use crate::WindowPixels;
use std::io;

use std::fs::Metadata;
use std::mem::MaybeUninit;
use std::os::windows::fs::MetadataExt;

use super::ExtraPermissions;

pub fn get_extra_perms(metadata: &Metadata) -> ExtraPermissions {
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

    // Converted from Windows FILETIME struct automatically
    let last_write_time = metadata.last_write_time();

    // https://docs.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime
    // https://docs.microsoft.com/en-us/windows/win32/api/timezoneapi/nf-timezoneapi-filetimetosystemtime
    // https://docs.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-systemtime
    let file_time = unsafe {
        let mut result: SYSTEMTIME = MaybeUninit::zeroed().assume_init();
        let err =
            FileTimeToSystemTime(&last_write_time as *const _ as *const FILETIME, &mut result);

        if err == BOOL(0) {
            panic!(
                "Failed to convert file time to system time with error code: {}",
                err.0
            );
        } else {
            result
        }
    };

    let modify_date_time = format!(
        "{} {} {:2} {:>2}:{:0>2}:{:0>2} {}",
        week_day(file_time.wDayOfWeek),
        month(file_time.wMonth),
        file_time.wDay,
        file_time.wHour,
        file_time.wMinute,
        file_time.wSecond,
        file_time.wYear,
    );

    ExtraPermissions {
        mode,
        user_name: None,
        group_name: None,
        hard_link_count: None,
        size: None,
        modify_date_time: Some(modify_date_time),
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

fn week_day(day: u16) -> &'static str {
    match day {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => unreachable!(),
    }
}

fn month(month_val: u16) -> &'static str {
    match month_val {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => unreachable!(),
    }
}
