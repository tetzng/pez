use serde_derive::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LockFile {
    pub(crate) version: u32,
    pub(crate) plugins: Vec<crate::models::Plugin>,
}

pub(crate) fn init() -> LockFile {
    LockFile {
        version: 1,
        plugins: vec![],
    }
}

pub(crate) fn load(path: &PathBuf) -> LockFile {
    let content = std::fs::read_to_string(path).unwrap();
    toml::from_str(&content).unwrap()
}

impl LockFile {
    pub(crate) fn add_plugin(&mut self, plugin: crate::models::Plugin) {
        if self
            .plugins
            .iter()
            .any(|p| p.source == plugin.source || p.name == plugin.name)
        {
            eprintln!(
                "Plugin already exists: name={}, source={}",
                plugin.name, plugin.source
            );
            return;
        }
        self.plugins.push(plugin);
    }

    pub(crate) fn remove_plugin(&mut self, source: &str) {
        self.plugins.retain(|p| p.source != source);
    }

    pub(crate) fn get_plugin(&self, source: &str) -> Option<&crate::models::Plugin> {
        self.plugins.iter().find(|p| p.source == source)
    }

    pub(crate) fn update_plugin(&mut self, plugin: crate::models::Plugin) {
        self.remove_plugin(&plugin.source);
        self.add_plugin(plugin);
    }
}
