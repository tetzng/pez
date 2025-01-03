use std::{env, path::PathBuf};

pub(crate) fn resolve_fish_config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("__fish_config_dir") {
        return PathBuf::from(dir);
    }

    if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("fish");
    }

    let home = env::var("HOME").unwrap();
    PathBuf::from(home).join(".config/fish")
}

pub(crate) fn resolve_pez_config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("PEZ_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    resolve_fish_config_dir()
}

pub(crate) fn resolve_lock_file_dir() -> PathBuf {
    crate::utils::resolve_pez_config_dir()
}

pub(crate) fn resolve_fish_data_dir() -> PathBuf {
    if let Some(dir) = env::var_os("__fish_user_data_dir") {
        return PathBuf::from(dir);
    }

    if let Some(dir) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("fish");
    }

    let home = env::var("HOME").unwrap();
    PathBuf::from(home).join(".local/share/fish")
}

pub(crate) fn resolve_pez_data_dir() -> PathBuf {
    if let Some(dir) = env::var_os("PEZ_DATA_DIR") {
        return PathBuf::from(dir);
    }

    let fish_data_dir = resolve_fish_data_dir();
    fish_data_dir.join("pez")
}

pub(crate) fn ensure_config() -> (crate::config::Config, PathBuf) {
    let config_dir = crate::utils::resolve_pez_config_dir();
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).unwrap();
    }
    let config_path = config_dir.join("pez.toml");
    let config = if config_path.exists() {
        crate::config::load(&config_path)
    } else {
        crate::config::init()
    };
    (config, config_path)
}

pub(crate) fn ensure_lock_file() -> (crate::lock_file::LockFile, PathBuf) {
    let lock_file_dir = crate::utils::resolve_lock_file_dir();
    if !lock_file_dir.exists() {
        std::fs::create_dir_all(&lock_file_dir).unwrap();
    }
    let lock_file_path = lock_file_dir.join("pez-lock.toml");
    let lock_file = if lock_file_path.exists() {
        crate::lock_file::load(&lock_file_path)
    } else {
        crate::lock_file::init()
    };
    (lock_file, lock_file_path)
}

pub(crate) fn copy_files_to_config(
    repo_path: &std::path::Path,
    plugin: &mut crate::lock_file::Plugin,
) {
    let config_dir = resolve_fish_config_dir();
    let target_dirs = crate::models::TargetDir::all();
    let mut has_target_file = false;

    println!("📂 Copying files:");
    for target_dir in target_dirs {
        let target_path = repo_path.join(target_dir.as_str());
        if !target_path.exists() {
            continue;
        }
        if !has_target_file {
            has_target_file = true;
        }
        let dest_path = config_dir.join(target_dir.as_str());
        if !dest_path.exists() {
            std::fs::create_dir_all(&dest_path).unwrap();
        }
        let files = std::fs::read_dir(target_path).unwrap();
        for file in files {
            let file = file.unwrap();
            if file.file_type().unwrap().is_dir() {
                continue;
            }
            let file_name = file.file_name();
            let file_path = file.path();
            let dest_file_path = dest_path.join(&file_name);
            println!("   - {}", dest_file_path.display());
            std::fs::copy(&file_path, &dest_file_path).unwrap();

            let plugin_file = crate::lock_file::PluginFile {
                dir: target_dir.clone(),
                name: file_name.to_string_lossy().to_string(),
            };
            plugin.files.push(plugin_file);
        }
    }
    if !has_target_file {
        println!("Warning: No valid files found in the repository.");
        println!("Ensure that it contains at least one file in 'functions', 'completions', 'conf.d', or 'themes'.");
    }
}
