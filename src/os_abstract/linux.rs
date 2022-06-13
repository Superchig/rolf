use std::path::PathBuf;

use super::env_or_dir;

pub fn config_dir(project_name: &str) -> PathBuf {
    env_or_dir("XDG_CONFIG_HOME", "HOME", ".config").join(project_name)
}
