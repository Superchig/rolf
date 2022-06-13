use std::env;
use std::path::PathBuf;

pub fn config_dir(project_name: &str) -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap())
        .join("Library/Application Support")
        .join(project_name)
}
