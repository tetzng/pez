use serde_derive::{Deserialize, Serialize};
use std::{fs, path};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) plugins: Option<Vec<PluginSpec>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PluginSpec {
    pub(crate) repo: String,
    pub(crate) name: Option<String>,
    pub(crate) source: Option<String>,
}

pub(crate) fn init() -> Config {
    Config { plugins: None }
}

pub(crate) fn load(path: &path::PathBuf) -> Config {
    let content = fs::read_to_string(path).unwrap();
    toml::from_str(&content).unwrap()
}

impl Config {
    pub(crate) fn save(&self, path: &path::PathBuf) {
        let contents = toml::to_string(self).unwrap();
        fs::write(path, contents).unwrap();
    }
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
