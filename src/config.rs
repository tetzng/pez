use std::path::PathBuf;

use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) plugins: Vec<PluginSpec>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PluginSpec {
    pub(crate) repo: String,
    pub(crate) name: Option<String>,
    pub(crate) source: Option<String>,
}

pub(crate) fn init() -> Config {
    Config { plugins: vec![] }
}

pub(crate) fn load(path: &PathBuf) -> Config {
    let content = std::fs::read_to_string(path).unwrap();
    toml::from_str(&content).unwrap()
}

impl PluginSpec {
    pub(crate) fn get_name(&self) -> String {
        if self.name.is_none() {
            let parts: Vec<&str> = self.repo.split("/").collect();
            parts[parts.len() - 1].to_owned()
        } else {
            self.name.clone().unwrap()
        }
    }
}
