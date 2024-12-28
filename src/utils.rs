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

pub(crate) fn resolve_pez_data_dir() -> PathBuf {
    if let Some(dir) = env::var_os("PEZ_DATA_DIR") {
        return PathBuf::from(dir);
    }

    let fish_data_dir = resolve_fish_data_dir();
    fish_data_dir.join("pez")
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

pub(crate) fn resolve_lock_file_path() -> PathBuf {
    let pez_config_dir = crate::utils::resolve_pez_config_dir();
    if !pez_config_dir.exists() {
        std::fs::create_dir_all(&pez_config_dir).unwrap();
    }
    pez_config_dir.join("pez-lock.toml")
}

pub(crate) fn get_latest_commit_sha(repo: git2::Repository) -> Result<String, git2::Error> {
    let commit = repo.head()?.peel_to_commit()?;

    Ok(commit.id().to_string())
}

pub(crate) fn format_git_url(plugin: &str) -> String {
    format!("https://github.com/{plugin}")
}

pub(crate) fn copy_files_to_config(repo_dir: &std::path::Path, plugin: &mut crate::models::Plugin) {
    let config_dir = resolve_fish_config_dir();
    let target_dirs = crate::models::TargetDir::all();
    let mut has_target_file = false;

    for target_dir in target_dirs {
        let target_path = repo_dir.join(target_dir.as_str());
        if !target_path.exists() {
            continue;
        }
        if !has_target_file {
            has_target_file = true;
        }
        let dest_path = config_dir.join(target_path.file_name().unwrap());
        if !dest_path.exists() {
            std::fs::create_dir_all(&dest_path).unwrap();
            println!("Created directory: {}", dest_path.display());
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
            std::fs::copy(&file_path, &dest_file_path).unwrap();
            println!("Copied: {}", dest_file_path.display());
            let plugin_file = crate::models::PluginFile {
                dir: target_dir.clone(),
                name: file_name.to_string_lossy().to_string(),
            };
            plugin.files.push(plugin_file);
        }
    }
    if !has_target_file {
        println!("No target files found");
    }
}
