use crate::{
    config,
    lock_file::{self, LockFile, Plugin, PluginFile},
    models::TargetDir,
};
use console::Emoji;
use std::{env, fmt, fs, path};
use tracing::{debug, error, info, warn};

fn home_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("HOME") {
        return Ok(path::PathBuf::from(dir));
    }

    Err(anyhow::anyhow!("Could not determine home directory"))
}

pub(crate) fn load_fish_config_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("__fish_config_dir") {
        return Ok(path::PathBuf::from(dir));
    }

    if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(path::PathBuf::from(dir).join("fish"));
    }

    let home = home_dir()?;
    Ok(home.join(".config").join("fish"))
}

pub(crate) fn load_pez_config_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("PEZ_CONFIG_DIR") {
        return Ok(path::PathBuf::from(dir));
    }

    load_fish_config_dir()
}

pub(crate) fn load_lock_file_dir() -> anyhow::Result<path::PathBuf> {
    load_pez_config_dir()
}

pub(crate) fn load_fish_data_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("__fish_user_data_dir") {
        return Ok(path::PathBuf::from(dir));
    }

    if let Some(dir) = env::var_os("XDG_DATA_HOME") {
        return Ok(path::PathBuf::from(dir).join("fish"));
    }

    let home = home_dir()?;
    Ok(home.join(".local/share/fish"))
}

pub(crate) fn load_pez_data_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("PEZ_DATA_DIR") {
        return Ok(path::PathBuf::from(dir));
    }

    let fish_data_dir = load_fish_data_dir()?;
    Ok(fish_data_dir.join("pez"))
}

pub(crate) fn load_config() -> anyhow::Result<(config::Config, path::PathBuf)> {
    let config_path = load_pez_config_dir()?.join("pez.toml");

    let config = if config_path.exists() {
        config::load(&config_path)?
    } else {
        return Err(anyhow::anyhow!("Config file not found"));
    };

    Ok((config, config_path))
}

pub(crate) fn load_or_create_config() -> anyhow::Result<(config::Config, path::PathBuf)> {
    let config_dir = load_pez_config_dir()?;
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }
    let config_path = config_dir.join("pez.toml");
    let config = if config_path.exists() {
        config::load(&config_path)?
    } else {
        config::init()
    };

    Ok((config, config_path))
}

pub(crate) fn load_lock_file() -> anyhow::Result<(LockFile, path::PathBuf)> {
    let lock_file_dir = load_lock_file_dir()?;
    let lock_file_path = lock_file_dir.join("pez-lock.toml");
    let lock_file = if lock_file_path.exists() {
        lock_file::load(&lock_file_path)?
    } else {
        return Err(anyhow::anyhow!("Lock file not found"));
    };

    Ok((lock_file, lock_file_path))
}

pub(crate) fn load_or_create_lock_file() -> anyhow::Result<(LockFile, path::PathBuf)> {
    let lock_file_dir = load_lock_file_dir()?;
    if !lock_file_dir.exists() {
        fs::create_dir_all(&lock_file_dir)?;
    }
    let lock_file_path = lock_file_dir.join("pez-lock.toml");
    let lock_file = if lock_file_path.exists() {
        lock_file::load(&lock_file_path)?
    } else {
        lock_file::init()
    };

    Ok((lock_file, lock_file_path))
}

pub(crate) fn copy_plugin_files_from_repo(
    repo_path: &path::Path,
    plugin: &mut Plugin,
) -> anyhow::Result<()> {
    info!("{}Copying files:", Emoji("📂 ", ""));
    let fish_config_dir = load_fish_config_dir()?;
    let file_count = copy_plugin_target_dirs(repo_path, &fish_config_dir, plugin)?;
    if file_count == 0 {
        warn_no_plugin_files();
    }
    Ok(())
}

fn copy_plugin_target_dirs(
    repo_path: &path::Path,
    fish_config_dir: &path::Path,
    plugin: &mut Plugin,
) -> anyhow::Result<usize> {
    let target_dirs = TargetDir::all();
    let mut file_count = 0;
    for target_dir in target_dirs {
        let target_path = repo_path.join(target_dir.as_str());
        if !target_path.exists() {
            continue;
        }
        let dest_path = fish_config_dir.join(target_dir.as_str());
        if !dest_path.exists() {
            fs::create_dir_all(&dest_path)?;
        }
        file_count += copy_plugin_files(target_path, dest_path, target_dir, plugin)?;
    }
    Ok(file_count)
}

fn copy_plugin_files(
    target_path: path::PathBuf,
    dest_path: path::PathBuf,
    target_dir: TargetDir,
    plugin: &mut Plugin,
) -> anyhow::Result<usize> {
    let files = fs::read_dir(target_path)?;
    let mut file_count = 0;

    for file in files {
        let file = file?;
        if file.file_type()?.is_dir() {
            continue;
        }
        let file_name = file.file_name();
        let file_path = file.path();
        let dest_file_path = dest_path.join(&file_name);
        info!("   - {}", dest_file_path.display());
        fs::copy(&file_path, &dest_file_path)?;

        let plugin_file = PluginFile {
            dir: target_dir.clone(),
            name: file_name.to_string_lossy().to_string(),
        };
        plugin.files.push(plugin_file);
        file_count += 1;
    }

    Ok(file_count)
}

pub(crate) enum Event {
    Install,
    Update,
    Uninstall,
}
impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Install => write!(f, "install"),
            Event::Update => write!(f, "update"),
            Event::Uninstall => write!(f, "uninstall"),
        }
    }
}

pub(crate) fn emit_event(file_name: &str, event: &Event) -> anyhow::Result<()> {
    let name = file_name.split('.').next();
    match name {
        Some(name) => {
            let output = std::process::Command::new("fish")
                .arg("-c")
                .arg(format!("emit {name}_{event}"))
                .spawn()
                .expect("Failed to execute process")
                .wait_with_output()?;
            debug!("Emitted event: {}_{}", name, event);

            if !output.status.success() {
                error!("Command executed with failing error code");
            }
        }
        None => {
            warn!(
                "Could not extract plugin name from file name: {}",
                file_name
            );
        }
    }

    Ok(())
}

fn warn_no_plugin_files() {
    warn!(
        "{} No valid files found in the repository.",
        console::style("Warning:").yellow()
    );
    warn!(
        "Ensure that it contains at least one file in 'functions', 'completions', 'conf.d', or 'themes'."
    );
}

#[cfg(test)]
mod tests {
    use config::PluginSpec;

    use super::*;
    use crate::cli::PluginRepo;
    use crate::models::TargetDir;
    use crate::tests_support::env::TestEnvironmentSetup;

    struct TestDataBuilder {
        plugin: Plugin,
        plugin_spec: PluginSpec,
    }

    impl TestDataBuilder {
        fn new() -> Self {
            Self {
                plugin: Plugin {
                    name: "repo".to_string(),
                    repo: PluginRepo {
                        owner: "owner".to_string(),
                        repo: "repo".to_string(),
                    },
                    source: "https://example.com/owner/repo".to_string(),
                    commit_sha: "sha".to_string(),
                    files: vec![],
                },
                plugin_spec: PluginSpec {
                    repo: PluginRepo {
                        owner: "owner".to_string(),
                        repo: "repo".to_string(),
                    },
                    name: None,
                    source: None,
                },
            }
        }
        fn build(self) -> TestData {
            TestData {
                plugin: self.plugin,
                plugin_spec: self.plugin_spec,
            }
        }
    }

    struct TestData {
        plugin: Plugin,
        plugin_spec: PluginSpec,
    }

    #[test]
    fn test_copy_plugin_files() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        let plugin_files = vec![PluginFile {
            dir: TargetDir::Functions,
            name: "file.fish".to_string(),
        }];
        let repo = test_data.plugin_spec.repo;
        fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(&repo, &plugin_files);

        let target_dir = TargetDir::Functions;
        let target_path = test_env
            .data_dir
            .join(repo.as_str())
            .join(target_dir.as_str());
        let dest_path = test_env.fish_config_dir.join(target_dir.as_str());
        fs::create_dir(&dest_path).unwrap();
        assert!(dest_path.exists());

        let repo_file_path = target_path.join(&plugin_files[0].name);
        assert!(fs::read(repo_file_path).is_ok());

        let files = fs::read_dir(&target_path).unwrap();
        assert_eq!(files.count(), plugin_files.len());

        let result = copy_plugin_files(
            target_path.clone(),
            dest_path,
            target_dir.clone(),
            &mut test_data.plugin,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), plugin_files.len());

        let copied_files: Vec<_> = test_env
            .fish_config_dir
            .join(target_dir.as_str())
            .read_dir()
            .unwrap()
            .collect();
        assert_eq!(copied_files.len(), plugin_files.len());

        copied_files.into_iter().for_each(|file| {
            let file = file.unwrap();
            assert_eq!(file.file_name().to_string_lossy(), plugin_files[0].name);
        });
    }

    #[test]
    fn test_copy_plugin_files_skips_non_file_entries() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        let repo = test_data.plugin_spec.repo;
        let target_dir = TargetDir::Functions;

        let target_path = test_env
            .data_dir
            .join(repo.as_str())
            .join(target_dir.as_str());
        let dest_path = test_env.fish_config_dir.join(target_dir.as_str());
        fs::create_dir_all(&dest_path).unwrap();
        assert!(dest_path.exists());

        let not_func_dir = test_env
            .data_dir
            .join(repo.as_str())
            .join(target_dir.as_str())
            .join("dir");
        fs::create_dir_all(&not_func_dir).unwrap();
        assert!(fs::read(not_func_dir).is_err());

        let files = fs::read_dir(&target_path).unwrap();
        assert_eq!(files.count(), 1);

        let result = copy_plugin_files(
            target_path.clone(),
            dest_path,
            target_dir.clone(),
            &mut test_data.plugin,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        let copied_files: Vec<_> = test_env
            .fish_config_dir
            .join(target_dir.as_str())
            .read_dir()
            .unwrap()
            .collect();
        assert_eq!(copied_files.len(), 0);
    }
}
