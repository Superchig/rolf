// This module implements functions that should work on both macOS and Linux

use crate::strmode;
use crate::unix_users;
use std::fs::Metadata;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::MetadataExt;

use super::ExtraPermissions;

pub fn get_strmode(metadata: &Metadata) -> String {
    let permissions = metadata.permissions();

    strmode(permissions.mode())
}

pub fn get_extra_perms(metadata: &Metadata) -> ExtraPermissions {
    ExtraPermissions {
        mode: get_strmode(metadata),
        user_name: unix_users::get_unix_groupname(metadata.gid()),
        group_name: unix_users::get_unix_username(metadata.uid()),
        hard_link_count: Some(metadata.nlink()),
        size: Some(metadata.size()),
    }
}
