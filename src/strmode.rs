// MIT License

// Copyright (c) 2019 Colvin Wellborn

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

// From https://github.com/colvin/strmode

//! An implementation of BSD strmode(3).

/// Returns a `String` describing the file permissions contained in a `u32`,
/// as in the`st_mode` bit field of `struct stat`. It is formatted in the usual
/// UNIX convention, eg `-rw-r--r--`.
///
/// # Examples
///
/// ```
/// use std::fs;
/// use std::os::unix::fs::PermissionsExt;
/// use strmode::strmode;
///
/// fn main() -> std::io::Result<()> {
///     let metadata = fs::metadata("/dev/null")?;
///     let permissions = metadata.permissions();
///     let mode = permissions.mode();
///
///     assert_eq!(strmode(mode), "crw-rw-rw-");
///
///     Ok(())
/// }
/// ```
pub fn strmode(mode: u32) -> String {
    let mut flags = ['-'; 10];

    let perms = [
        (0o000400, 'r'), (0o000200, 'w'), (0o000100, 'x'), // user
        (0o000040, 'r'), (0o000020, 'w'), (0o000010, 'x'), // group
        (0o000004, 'r'), (0o000002, 'w'), (0o000001, 'x'), // other
    ];

    // Permissions
    let s = &mut flags[1..];
    for i in 0..9 {
        if mode & perms[i].0 == perms[i].0 {
            s[i] = perms[i].1;
        }
    }

    // File type
    match mode & 0o170000 {
        0o010000    => { flags[0] = 'p' },  // fifo
        0o020000    => { flags[0] = 'c' },  // character special
        0o040000    => { flags[0] = 'd' },  // directory
        0o060000    => { flags[0] = 'b' },  // block special
        0o100000    => { },                 // regular file
        0o120000    => { flags[0] = 'l' },  // symbolic link
        0o140000    => { flags[0] = 's' },  // socket
        _           => { flags[0] = '?' },  // unknown
    }

    // setuid
    let xusr_setuid = mode & (0o000100 | 0o004000);
    if xusr_setuid == 0o004000 {
        flags[3] = 'S';
    } else if xusr_setuid == (0o000100 | 0o004000) {
         flags[3] = 's';
    }

    // setgid
    let xgrp_setgid = mode & (0o000010 | 0o002000);
    if xgrp_setgid == 0o002000 {
        flags[6] = 'S';
    } else if xgrp_setgid == (0o000010 | 0o002000) {
        flags[6] = 's';
    }

    // sticky
    let xoth_sticky = mode & (0o000001 | 0o001000);
    if xoth_sticky == 0o001000 {
        flags[9] = 'T';
    } else if xoth_sticky == (0o000001 | 0o001000) {
        flags[9] = 't';
    }

    return flags.iter().collect();
}

#[test]
fn test_strmode() {
    let tests = [
        (0o100644, "-rw-r--r--", "file, 644"),
        (0o100600, "-rw-------", "file, 600"),
        (0o100777, "-rwxrwxrwx", "file, 777"),
        (0o040755, "drwxr-xr-x", "directory, 755"),
        (0o040711, "drwx--x--x", "directory, 711"),
        (0o020660, "crw-rw----", "character special, 660"),
        (0o060660, "brw-rw----", "block special, 660"),
        (0o120777, "lrwxrwxrwx", "symbolic link, 777"),
        (0o010600, "prw-------", "fifo, 600"),
        (0o140755, "srwxr-xr-x", "socket ,755"),
        (0o104555, "-r-sr-xr-x", "file, 755 with setuid"),
        (0o104644, "-rwSr--r--", "file, 644 with setuid"),
        (0o044755, "drwsr-xr-x", "directory, 755 with setuid"),
        (0o044666, "drwSrw-rw-", "directory, 666 with setuid"),
        (0o102755, "-rwxr-sr-x", "file, 755 with setgid"),
        (0o102644, "-rw-r-Sr--", "file, 644 with setgid"),
        (0o042755, "drwxr-sr-x", "directory, 755 with setgid"),
        (0o042644, "drw-r-Sr--", "directory, 644 with setgid"),
        (0o041755, "drwxr-xr-t", "directory, 755 with sticky"),
        (0o041644, "drw-r--r-T", "directory, 644 with sticky"),
        (0o104471, "-r-Srwx--x", "file, 471 with setuid"),
        (0o106471, "-r-Srws--x", "file, 471 with setuid and setgid"),
        (0o044471, "dr-Srwx--x", "directory, 471 with setuid"),
        (0o046471, "dr-Srws--x", "directory, 471 with setuid and setgid"),
        (0o045471, "dr-Srwx--t", "directory, 471 with setuid and sticky"),
        (0o047471, "dr-Srws--t", "directory, 471 with setuid, setgid, and sticky"),
        (0o047470, "dr-Srws--T", "directory, 470 with setuid, setgid, and sticky"),
    ];

    for t in &tests {
        assert_eq!(t.1, strmode(t.0), "{}: {:o}", t.2, t.0);
    }
}
