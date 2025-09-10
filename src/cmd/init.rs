use tracing::info;

use crate::utils;
use std::{fs, path};

pub(crate) fn run() -> anyhow::Result<()> {
    let config_dir = utils::load_pez_config_dir()?;
    create_config(&config_dir)
}

fn create_config(config_dir: &path::Path) -> anyhow::Result<()> {
    if !config_dir.exists() {
        fs::create_dir_all(config_dir)?;
    }

    let config_path = config_dir.join("pez.toml");
    if config_path.exists() {
        anyhow::bail!("{} already exists", config_path.display());
    }

    let contents = r#"# This file defines the plugins to be installed by pez.

# Examples of plugins:
# [[plugins]]
# repo = "owner/repo"      # GitHub shorthand
# # version = "v3"        # Or: tag = "...", branch = "...", commit = "..."
# # name = "custom-name"   # Optional display name

# [[plugins]]
# url = "https://gitlab.com/owner/repo"  # Any Git host URL
# # branch = "main"

# [[plugins]]
# path = "~/path/to/local/plugin"       # Local directory

# Add more plugins by copying the [[plugins]] block.
"#;
    fs::write(&config_path, contents)?;
    info!("Created {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_create_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path();
        let config_path = config_dir.join("pez.toml");
        let result = create_config(config_dir);

        assert!(result.is_ok());
        assert!(config_path.exists());

        let contents = fs::read_to_string(&config_path).unwrap();
        assert!(contents.contains("[[plugins]]"));
        assert!(contents.contains("repo = \"owner/repo\""));
    }

    #[test]
    fn test_create_config_already_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path();
        let config_path = config_dir.join("pez.toml");
        fs::write(&config_path, "").unwrap();

        let result = create_config(config_dir);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            format!("{} already exists", config_path.display())
        );
    }
}
