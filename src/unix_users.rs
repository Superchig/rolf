use std::mem;
use std::ptr;
use std::ffi::CStr;

// Taken from https://users.rust-lang.org/t/using-libc-to-get-username-from-user-id/6849/3
pub fn get_unix_username(uid: u32) -> Option<String> {
    unsafe {
        let mut result = ptr::null_mut();
        let amt = match libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) {
            n if n < 0 => 512_usize,
            n => n as usize,
        };
        let mut buf = Vec::with_capacity(amt);
        let mut passwd: libc::passwd = mem::zeroed();

        match libc::getpwuid_r(uid, &mut passwd, buf.as_mut_ptr(),
                              buf.capacity() as libc::size_t,
                              &mut result) {
           0 if !result.is_null() => {
               let ptr = passwd.pw_name as *const _;
               let username = CStr::from_ptr(ptr).to_str().unwrap().to_owned();
               Some(username)
           },
           _ => None
        }
    }
}

// A modified version of get_unix_username
// Relevant man pages are getgrid and unistd.h
pub fn get_unix_groupname(gid: u32) -> Option<String> {
    unsafe {
        let mut result = ptr::null_mut();
        let amt = match libc::sysconf(libc::_SC_GETGR_R_SIZE_MAX) {
            n if n < 0 => 512_usize,
            n => n as usize,
        };
        let mut buf = Vec::with_capacity(amt);
        let mut group: libc::group = mem::zeroed();

        match libc::getgrgid_r(gid, &mut group, buf.as_mut_ptr(),
            buf.capacity() as libc::size_t,
            &mut result) {
            0 if !result.is_null() => {
                let ptr = group.gr_name as *const _;
                let groupname = CStr::from_ptr(ptr).to_str().unwrap().to_owned();
                Some(groupname)
            },
            _ => None
        }
    }
}
