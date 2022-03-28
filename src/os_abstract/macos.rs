use std::path::PathBuf;
use std::env;

pub fn config_dir(project_name: &str) -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap()).join("Library/Application Support").join(project_name)
}

pub fn data_dir(project_name: &str) -> PathBuf {
    // NOTE(Chris): On macOS, we should use the same directory for storing data and configuration
    // files
    config_dir(project_name)
}
