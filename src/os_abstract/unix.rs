use std::os::unix::fs::PermissionsExt;
use std::fs::Metadata;
use crate::strmode;

pub fn get_strmode(metadata: &Metadata) -> String {
    let permissions = metadata.permissions();

    strmode(permissions.mode())
}

