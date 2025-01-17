use crate::{
    config,
    lock_file::{self, LockFile, Plugin, PluginFile},
    models::TargetDir,
};
use console::Emoji;
use std::{env, fs, path};

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
    println!("{}Copying files:", Emoji("📂 ", ""));
    let file_count = copy_plugin_target_dirs(repo_path, plugin)?;
    if file_count == 0 {
        warn_no_plugin_files();
    }
    Ok(())
}

fn copy_plugin_target_dirs(repo_path: &path::Path, plugin: &mut Plugin) -> anyhow::Result<usize> {
    let config_dir = load_fish_config_dir()?;
    let target_dirs = TargetDir::all();
    let mut file_count = 0;
    for target_dir in target_dirs {
        let target_path = repo_path.join(target_dir.as_str());
        if !target_path.exists() {
            continue;
        }
        let dest_path = config_dir.join(target_dir.as_str());
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
        println!("   - {}", dest_file_path.display());
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

fn warn_no_plugin_files() {
    println!(
        "{} No valid files found in the repository.",
        console::style("Warning:").yellow()
    );
    println!("Ensure that it contains at least one file in 'functions', 'completions', 'conf.d', or 'themes'.");
}
