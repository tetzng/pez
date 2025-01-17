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

pub(crate) fn load(path: &path::PathBuf) -> anyhow::Result<Config> {
    let content = fs::read_to_string(path)?;
    let config = toml::from_str(&content)?;

    Ok(config)
}

impl Config {
    pub(crate) fn save(&self, path: &path::PathBuf) -> anyhow::Result<()> {
        let contents = toml::to_string(self)?;
        fs::write(path, contents)?;

        Ok(())
    }
}

impl PluginSpec {
    pub(crate) fn get_name(&self) -> anyhow::Result<String> {
        if self.name.is_none() {
            let parts: Vec<&str> = self.repo.split("/").collect();
            Ok(parts[parts.len() - 1].to_owned())
        } else {
            let name = self
                .name
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Name not found"))?;

            Ok(name)
        }
    }
}
