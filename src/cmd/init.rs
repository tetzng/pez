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

    let contents = r#"# Plugins managed by pez.
#
# Use exactly one source per plugin: repo, url, or path.
# Remote sources may use one selector: version, branch, tag, or commit.

# [[plugins]]
# repo = "owner/repo"        # GitHub shorthand
# # version = "v3"          # Branch-or-tag selector
# # name = "custom-name"    # Optional display name

# [[plugins]]
# repo = "gitlab.com/owner/repo"
# # branch = "main"

# [[plugins]]
# url = "https://example.com/owner/repo.git"
# # tag = "v1.2.3"

# [[plugins]]
# path = "~/plugins/local-plugin"
"#;
    fs::write(&config_path, contents)?;
    info!("Created {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::log::env_lock;
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

    #[test]
    fn test_create_config_creates_missing_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join("nested");
        let config_path = config_dir.join("pez.toml");

        let result = create_config(&config_dir);
        assert!(result.is_ok());
        assert!(config_dir.exists());
        assert!(config_path.exists());
    }

    #[test]
    fn test_run_creates_config_in_env_dir() {
        let _lock = env_lock().lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join("pez-config");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &config_dir);
        }

        let result = run();

        unsafe {
            if let Some(v) = prev_pc {
                std::env::set_var("PEZ_CONFIG_DIR", v);
            } else {
                std::env::remove_var("PEZ_CONFIG_DIR");
            }
        }

        assert!(result.is_ok());
        assert!(config_dir.join("pez.toml").exists());
    }
}
