use serde_derive::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LockFile {
    pub(crate) version: u32,
    pub(crate) plugins: Vec<Plugin>,
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
    pub(crate) fn save(&self, path: &PathBuf) {
        let contents = toml::to_string(self).unwrap();
        std::fs::write(path, AUTO_GENERATED_COMMENT.to_string() + &contents).unwrap();
    }

    pub(crate) fn add_plugin(&mut self, plugin: Plugin) {
        if self
            .plugins
            .iter()
            .any(|p| p.source == plugin.source || p.name == plugin.name)
        {
            eprintln!(
                "Plugin already exists: name={}, source={}",
                plugin.name, plugin.source
            );
            std::process::exit(1);
        }
        self.plugins.push(plugin);
    }

    pub(crate) fn remove_plugin(&mut self, source: &str) {
        self.plugins.retain(|p| p.source != source);
    }

    pub(crate) fn get_plugin(&self, source: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.source == source)
    }

    pub(crate) fn update_plugin(&mut self, plugin: Plugin) {
        self.remove_plugin(&plugin.source);
        self.add_plugin(plugin);
    }
}

pub(crate) const AUTO_GENERATED_COMMENT: &str =
    "# This file is automatically generated by pez. Do not edit it manually.\n";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Plugin {
    pub(crate) name: String,
    pub(crate) repo: String,
    pub(crate) source: String,
    pub(crate) commit_sha: String,
    pub(crate) files: Vec<PluginFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct PluginFile {
    pub(crate) dir: crate::models::TargetDir,
    pub(crate) name: String,
}

impl Plugin {
    pub(crate) fn get_name(&self) -> String {
        if self.name.is_empty() {
            let parts: Vec<&str> = self.source.split("/").collect();
            parts[parts.len() - 1].to_owned()
        } else {
            self.name.clone()
        }
    }
}

impl PluginFile {
    pub(crate) fn get_path(&self, config_dir: &Path) -> PathBuf {
        config_dir.join(self.dir.as_str()).join(&self.name)
    }
}
