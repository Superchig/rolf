use std::{path::PathBuf, env::{self, VarError}};

pub fn data_dir(project_name: &str) -> PathBuf {
    match env::var("XDG_DATA_HOME") {
        Ok(data_dir) => PathBuf::from(data_dir),
        Err(VarError::NotPresent) => {
            let mut result = PathBuf::from(env::var("HOME").unwrap());
            result.push(".local/share");
            result.push(project_name);
            result
        },
        Err(_) => panic!("Unable to read data directory"),
    }
}
