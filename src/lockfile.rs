use serde_derive::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LockFile {
    pub(crate) config: Config,
    pub(crate) plugins: Vec<crate::models::Plugin>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) path: String,
}

pub(crate) fn init_lock_file(config_dir: PathBuf) -> LockFile {
    LockFile {
        config: Config {
            path: config_dir.to_string_lossy().to_string(),
        },
        plugins: vec![],
    }
}

pub(crate) fn load_lock_file(path: &PathBuf) -> LockFile {
    let content = std::fs::read_to_string(path).unwrap();
    toml::from_str(&content).unwrap()
}
