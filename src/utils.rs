use crate::{
    config,
    lock_file::{self, LockFile, Plugin, PluginFile},
    models::TargetDir,
};
use anyhow::Context;
use console::Emoji;
use std::{collections::HashSet, env, fmt, fs, path};
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;

fn home_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("HOME") {
        return Ok(path::PathBuf::from(dir));
    }

    Err(anyhow::anyhow!("Could not determine home directory"))
}

pub(crate) fn load_fish_config_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("PEZ_TARGET_DIR") {
        return Ok(path::PathBuf::from(dir));
    }
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

pub(crate) fn load_jobs() -> usize {
    if let Ok(val) = env::var("PEZ_JOBS")
        && let Ok(n) = val.parse::<usize>()
    {
        return n.max(1);
    }
    4
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
    let outcome = copy_plugin_files(repo_path, &fish_config_dir, plugin, None, false)?;
    let file_count = outcome.file_count;
    if file_count == 0 {
        warn_no_plugin_files();
    }
    Ok(())
}

#[derive(Debug, Default, Clone)]
pub(crate) struct CopyOutcome {
    pub file_count: usize,
    pub skipped_due_to_duplicate: bool,
}

pub(crate) fn copy_plugin_files(
    repo_path: &path::Path,
    fish_config_dir: &path::Path,
    plugin: &mut Plugin,
    mut dedupe: Option<&mut HashSet<path::PathBuf>>,
    skip_on_duplicate: bool,
) -> anyhow::Result<CopyOutcome> {
    let mut outcome = CopyOutcome::default();
    let target_dirs = TargetDir::all();
    let mut to_copy: Vec<(TargetDir, path::PathBuf)> = Vec::new();

    // Scan phase: gather files and check duplicates early
    for target_dir in &target_dirs {
        let target_path = repo_path.join(target_dir.as_str());
        if !target_path.exists() {
            continue;
        }
        let dest_dir = fish_config_dir.join(target_dir.as_str());
        if !dest_dir.exists() {
            fs::create_dir_all(&dest_dir)?;
        }

        let expected_ext = match target_dir {
            TargetDir::Themes => Some("theme"),
            _ => Some("fish"),
        };
        for entry in WalkDir::new(&target_path)
            .into_iter()
            .filter_map(Result::ok)
        {
            let entry_path = entry.path();
            if entry.file_type().is_dir() {
                continue;
            }
            if let Some(ext) = expected_ext
                && entry_path.extension().and_then(|s| s.to_str()) != Some(ext)
            {
                continue;
            }
            let rel = entry_path.strip_prefix(&target_path).with_context(|| {
                format!(
                    "Failed to strip prefix {} from {}",
                    target_path.display(),
                    entry_path.display()
                )
            })?;
            let dest_path = dest_dir.join(rel);
            if let Some(set) = dedupe.as_deref_mut()
                && set.contains(&dest_path)
                && skip_on_duplicate
            {
                warn!(
                    "{} Duplicate detected. Skipping plugin due to collision: {}",
                    Emoji("🚨 ", ""),
                    dest_path.display()
                );
                outcome.skipped_due_to_duplicate = true;
                return Ok(outcome);
            }
            to_copy.push((target_dir.clone(), rel.to_path_buf()));
        }
    }

    // Copy phase
    for (dir, rel) in to_copy.iter() {
        let src = repo_path.join(dir.as_str()).join(rel);
        let dest = fish_config_dir.join(dir.as_str()).join(rel);
        if let Some(parent) = dest.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }
        info!("   - {}", dest.display());
        fs::copy(&src, &dest)?;
        plugin.files.push(PluginFile {
            dir: dir.clone(),
            name: rel.to_string_lossy().to_string(),
        });
        outcome.file_count += 1;
        if let Some(set) = dedupe.as_deref_mut() {
            set.insert(dest);
        }
    }

    Ok(outcome)
}

#[allow(dead_code)]
fn copy_plugin_files_recursive(
    target_path: &path::Path,
    dest_path: &path::Path,
    target_dir: TargetDir,
    plugin: &mut Plugin,
) -> anyhow::Result<usize> {
    let mut file_count = 0;
    let expected_ext = match target_dir {
        TargetDir::Themes => Some("theme"),
        _ => Some("fish"),
    };

    for entry in WalkDir::new(target_path).into_iter().filter_map(Result::ok) {
        let entry_path = entry.path();
        if entry.file_type().is_dir() {
            continue;
        }
        if let Some(ext) = expected_ext
            && entry_path.extension().and_then(|s| s.to_str()) != Some(ext)
        {
            continue;
        }

        let rel = entry_path.strip_prefix(target_path).with_context(|| {
            format!(
                "Failed to strip prefix {} from {}",
                target_path.display(),
                entry_path.display()
            )
        })?;
        let dest_file_path = dest_path.join(rel);
        if let Some(parent) = dest_file_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }
        info!("   - {}", dest_file_path.display());
        fs::copy(entry_path, &dest_file_path)?;

        let plugin_file = PluginFile {
            dir: target_dir.clone(),
            name: rel.to_string_lossy().to_string(),
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

pub(crate) fn emit_event(file_name_or_path: &str, event: &Event) -> anyhow::Result<()> {
    let stem_opt = path::Path::new(file_name_or_path)
        .file_stem()
        .and_then(|s| s.to_str());
    match stem_opt {
        Some(stem) => {
            let output = std::process::Command::new("fish")
                .arg("-c")
                .arg(format!("emit {stem}_{event}"))
                .spawn()
                .context("Failed to spawn fish to emit event")?
                .wait_with_output()?;
            debug!("Emitted event: {}_{}", stem, event);

            if !output.status.success() {
                error!("Command executed with failing error code");
            }
        }
        None => {
            warn!(
                "Could not extract plugin name from file name: {}",
                file_name_or_path
            );
        }
    }

    Ok(())
}

fn warn_no_plugin_files() {
    warn!(
        "{} No valid files found in the repository.",
        label_warning()
    );
    warn!(
        "Ensure that it contains at least one file in 'functions', 'completions', 'conf.d', or 'themes'."
    );
}

// --- Color-aware labels ----------------------------------------------------
// Colored labels when ANSI is supported; plain otherwise.
pub(crate) fn colors_enabled_for_stderr() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }

    let force_color = |key: &str| match std::env::var(key) {
        Ok(v) => v != "0",
        Err(_) => false,
    };
    if force_color("CLICOLOR_FORCE") || force_color("FORCE_COLOR") {
        return true;
    }

    if matches!(std::env::var("CLICOLOR"), Ok(v) if v == "0") {
        return false;
    }

    if matches!(std::env::var("TERM"), Ok(term) if term == "dumb") {
        return false;
    }

    let term = console::Term::stderr();
    if !term.features().colors_supported() {
        return false;
    }

    term.features().is_attended()
}

pub(crate) fn label_info() -> &'static str {
    "[Info]"
}

pub(crate) fn label_warning() -> &'static str {
    "[Warning]"
}

pub(crate) fn label_error() -> &'static str {
    "[Error]"
}

pub(crate) fn label_notice() -> &'static str {
    "[Notice]"
}

#[cfg(test)]
mod tests {
    use config::{PluginSource, PluginSpec};

    use super::*;
    use crate::models::PluginRepo;
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
                    name: None,
                    source: PluginSource::Repo {
                        repo: PluginRepo {
                            owner: "owner".to_string(),
                            repo: "repo".to_string(),
                        },
                        version: None,
                        branch: None,
                        tag: None,
                        commit: None,
                    },
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
        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
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

        let result = copy_plugin_files_recursive(
            &target_path,
            &dest_path,
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

        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
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

        let result = copy_plugin_files_recursive(
            &target_path,
            &dest_path,
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

    #[test]
    fn test_copy_plugin_files_deep_directories() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        let plugin_files = vec![PluginFile {
            dir: TargetDir::Functions,
            name: "nested/dir/sample.fish".to_string(),
        }];

        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
        fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(&repo, &plugin_files);

        let target_dir = TargetDir::Functions;
        let target_path = test_env
            .data_dir
            .join(repo.as_str())
            .join(target_dir.as_str());
        let dest_path = test_env.fish_config_dir.join(target_dir.as_str());
        fs::create_dir_all(&dest_path).unwrap();

        let result = copy_plugin_files_recursive(
            &target_path,
            &dest_path,
            target_dir.clone(),
            &mut test_data.plugin,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        assert!(
            dest_path.join("nested/dir/sample.fish").exists(),
            "Nested file should be copied to matching path"
        );

        assert!(
            test_data
                .plugin
                .files
                .iter()
                .any(|f| f.dir == TargetDir::Functions && f.name == "nested/dir/sample.fish")
        );
    }

    #[test]
    fn test_copy_plugin_files_dedupe_skip_on_duplicate() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        // Arrange: create a repo with one function file
        let plugin_files = vec![PluginFile {
            dir: TargetDir::Functions,
            name: "sample.fish".to_string(),
        }];
        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
        std::fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(&repo, &plugin_files);

        // Pre-create the destination path and mark it as already occupied in dedupe set
        let dest_dir = test_env.fish_config_dir.join(TargetDir::Functions.as_str());
        std::fs::create_dir_all(&dest_dir).unwrap();
        let existing_dest = dest_dir.join("sample.fish");
        std::fs::File::create(&existing_dest).unwrap();

        let mut dedupe = std::collections::HashSet::new();
        dedupe.insert(existing_dest.clone());

        // Act: copy with dedupe and skip_on_duplicate = true
        let repo_path = test_env.data_dir.join(repo.as_str());
        let outcome = copy_plugin_files(
            &repo_path,
            &test_env.fish_config_dir,
            &mut test_data.plugin,
            Some(&mut dedupe),
            true,
        )
        .expect("copy should not error");

        // Assert: skip flagged, no files recorded/copied beyond pre-existing
        assert!(outcome.skipped_due_to_duplicate);
        assert_eq!(outcome.file_count, 0);
        assert!(test_data.plugin.files.is_empty());
        // Pre-existing file remains
        assert!(std::fs::metadata(&existing_dest).is_ok());
    }
}
