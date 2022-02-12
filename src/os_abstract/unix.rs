// This module implements functions that should work on both macOS and Linux

use chrono::{DateTime, Local, NaiveDateTime, TimeZone};

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
