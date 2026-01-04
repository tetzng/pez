use crate::{
    config,
    lock_file::{self, LockFile, Plugin, PluginFile},
    models::TargetDir,
};
use anyhow::Context;
use console::Emoji;
use std::{
    collections::HashSet,
    env, fmt, fs, path,
    sync::{Mutex, OnceLock},
};
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;

fn home_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("HOME") {
        return Ok(path::PathBuf::from(dir));
    }

    Err(anyhow::anyhow!("Could not determine home directory"))
}

fn load_default_fish_config_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("__fish_config_dir") {
        return Ok(path::PathBuf::from(dir));
    }

    if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(path::PathBuf::from(dir).join("fish"));
    }

    let home = home_dir()?;
    Ok(home.join(".config").join("fish"))
}

fn load_base_config_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("PEZ_CONFIG_DIR") {
        return Ok(path::PathBuf::from(dir));
    }

    load_default_fish_config_dir()
}

pub(crate) fn load_fish_config_dir() -> anyhow::Result<path::PathBuf> {
    if let Some(dir) = env::var_os("PEZ_TARGET_DIR") {
        return Ok(path::PathBuf::from(dir));
    }

    load_default_fish_config_dir()
}

pub(crate) fn load_pez_config_dir() -> anyhow::Result<path::PathBuf> {
    load_base_config_dir()
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
    if let Some(override_jobs) = cli_jobs_override().lock().unwrap().as_ref().copied() {
        return override_jobs;
    }
    if let Ok(val) = env::var("PEZ_JOBS")
        && let Ok(n) = val.parse::<usize>()
    {
        return n.max(1);
    }
    4
}

pub(crate) fn set_cli_jobs_override(value: Option<usize>) {
    *cli_jobs_override().lock().unwrap() = value;
}

fn cli_jobs_override() -> &'static Mutex<Option<usize>> {
    static JOBS_OVERRIDE: OnceLock<Mutex<Option<usize>>> = OnceLock::new();
    JOBS_OVERRIDE.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
pub(crate) fn clear_cli_jobs_override_for_tests() {
    *cli_jobs_override().lock().unwrap() = None;
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
    // Allow callers (e.g., fish wrapper) to suppress out-of-process emits to
    // avoid duplicate hooks when the shell itself handles events in-process.
    if std::env::var_os("PEZ_SUPPRESS_EMIT").is_some() {
        return Ok(());
    }

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
    colors_enabled_for(&console::Term::stderr())
}

fn colors_enabled_for(term: &console::Term) -> bool {
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
    use crate::tests_support::log::{capture_logs, env_lock};
    use std::ffi::OsString;

    struct EnvGuard {
        vars: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvGuard {
        fn capture(keys: &[&'static str]) -> Self {
            let vars = keys
                .iter()
                .map(|&key| (key, std::env::var_os(key)))
                .collect();
            Self { vars }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.vars {
                match value {
                    Some(val) => unsafe { std::env::set_var(key, val.clone()) },
                    None => unsafe { std::env::remove_var(key) },
                }
            }
        }
    }

    #[test]
    fn load_pez_config_dir_prefers_config_dir_over_target_dir() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "PEZ_CONFIG_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
        ]);

        let temp = tempfile::tempdir().unwrap();
        let config_dir = temp.path().join("config");
        let target_dir = temp.path().join("target");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &config_dir);
            std::env::set_var("PEZ_TARGET_DIR", &target_dir);
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", temp.path());
        }

        let resolved = load_pez_config_dir().expect("config dir should resolve");
        assert_eq!(resolved, config_dir);
    }

    #[test]
    fn load_pez_config_dir_ignores_target_dir_when_unset() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "PEZ_CONFIG_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
        ]);

        let temp = tempfile::tempdir().unwrap();
        let fish_config_dir = temp.path().join("fish_config");
        let target_dir = temp.path().join("target");
        std::fs::create_dir_all(&fish_config_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        unsafe {
            std::env::remove_var("PEZ_CONFIG_DIR");
            std::env::set_var("PEZ_TARGET_DIR", &target_dir);
            std::env::set_var("__fish_config_dir", &fish_config_dir);
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", temp.path());
        }

        let resolved = load_pez_config_dir().expect("config dir should resolve");
        assert_eq!(resolved, fish_config_dir);
    }

    #[test]
    fn load_fish_config_dir_honors_target_dir() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "PEZ_CONFIG_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
        ]);

        let temp = tempfile::tempdir().unwrap();
        let target_dir = temp.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();

        unsafe {
            std::env::remove_var("PEZ_CONFIG_DIR");
            std::env::set_var("PEZ_TARGET_DIR", &target_dir);
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", temp.path());
        }

        let resolved = load_fish_config_dir().expect("fish config dir should resolve");
        assert_eq!(resolved, target_dir);
    }

    #[test]
    fn load_jobs_prefers_cli_override() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["PEZ_JOBS"]);
        clear_cli_jobs_override_for_tests();
        unsafe {
            std::env::set_var("PEZ_JOBS", "8");
        }
        set_cli_jobs_override(Some(2));
        assert_eq!(load_jobs(), 2);
        clear_cli_jobs_override_for_tests();
    }

    #[test]
    fn load_jobs_falls_back_to_env_when_override_absent() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["PEZ_JOBS"]);
        clear_cli_jobs_override_for_tests();
        unsafe {
            std::env::set_var("PEZ_JOBS", "6");
        }
        assert_eq!(load_jobs(), 6);
        unsafe {
            std::env::remove_var("PEZ_JOBS");
        }
        assert_eq!(load_jobs(), 4);
    }

    #[test]
    fn home_dir_uses_home_env() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["HOME"]);
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let resolved = home_dir().expect("home dir should resolve");
        assert_eq!(resolved, temp.path());
    }

    #[test]
    fn load_fish_data_dir_prefers_fish_user_data_dir() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["__fish_user_data_dir", "XDG_DATA_HOME", "HOME"]);

        let temp = tempfile::tempdir().unwrap();
        let fish_data = temp.path().join("fish_data");
        let xdg_data = temp.path().join("xdg_data");
        std::fs::create_dir_all(&fish_data).unwrap();
        std::fs::create_dir_all(&xdg_data).unwrap();

        unsafe {
            std::env::set_var("__fish_user_data_dir", &fish_data);
            std::env::set_var("XDG_DATA_HOME", &xdg_data);
            std::env::set_var("HOME", temp.path());
        }

        let resolved = load_fish_data_dir().expect("fish data dir should resolve");
        assert_eq!(resolved, fish_data);
    }

    #[test]
    fn load_fish_data_dir_uses_xdg_data_home() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["__fish_user_data_dir", "XDG_DATA_HOME", "HOME"]);

        let temp = tempfile::tempdir().unwrap();
        let xdg_data = temp.path().join("xdg_data");
        std::fs::create_dir_all(&xdg_data).unwrap();

        unsafe {
            std::env::remove_var("__fish_user_data_dir");
            std::env::set_var("XDG_DATA_HOME", &xdg_data);
            std::env::set_var("HOME", temp.path());
        }

        let resolved = load_fish_data_dir().expect("fish data dir should resolve");
        assert_eq!(resolved, xdg_data.join("fish"));
    }

    #[test]
    fn load_fish_data_dir_falls_back_to_home() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["__fish_user_data_dir", "XDG_DATA_HOME", "HOME"]);

        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::remove_var("__fish_user_data_dir");
            std::env::remove_var("XDG_DATA_HOME");
            std::env::set_var("HOME", temp.path());
        }

        let resolved = load_fish_data_dir().expect("fish data dir should resolve");
        assert_eq!(resolved, temp.path().join(".local/share/fish"));
    }

    #[test]
    fn load_or_create_config_creates_missing_dir() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "PEZ_CONFIG_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
        ]);

        let temp = tempfile::tempdir().unwrap();
        let config_dir = temp.path().join("config_root");
        assert!(!config_dir.exists());

        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &config_dir);
            std::env::remove_var("PEZ_TARGET_DIR");
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", temp.path());
        }

        let (_config, path) = load_or_create_config().expect("config should load");
        assert_eq!(path, config_dir.join("pez.toml"));
        assert!(config_dir.exists());
    }

    #[test]
    fn load_or_create_lock_file_creates_missing_dir() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "PEZ_CONFIG_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
        ]);

        let temp = tempfile::tempdir().unwrap();
        let config_dir = temp.path().join("lock_root");
        assert!(!config_dir.exists());

        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &config_dir);
            std::env::remove_var("PEZ_TARGET_DIR");
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", temp.path());
        }

        let (_lock_file, path) = load_or_create_lock_file().expect("lock file should load");
        assert_eq!(path, config_dir.join("pez-lock.toml"));
        assert!(config_dir.exists());
    }

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
                        host: None,
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
                            host: None,
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

    #[test]
    fn copy_plugin_files_from_repo_warns_when_empty() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
        ]);

        let test_env = TestEnvironmentSetup::new();
        let repo = PluginRepo {
            host: None,
            owner: "owner".to_string(),
            repo: "repo".to_string(),
        };
        let repo_path = test_env.data_dir.join(repo.as_str());
        std::fs::create_dir_all(&repo_path).unwrap();

        unsafe {
            std::env::set_var("PEZ_TARGET_DIR", &test_env.fish_config_dir);
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", test_env._temp_dir.path());
        }

        let mut plugin = Plugin {
            name: "repo".to_string(),
            repo,
            source: "https://example.com/owner/repo".to_string(),
            commit_sha: "sha".to_string(),
            files: vec![],
        };

        let (logs, result) = capture_logs(|| copy_plugin_files_from_repo(&repo_path, &mut plugin));
        assert!(result.is_ok());
        assert!(plugin.files.is_empty());
        assert!(logs.iter().any(|msg| msg.contains("No valid files found")));
    }

    #[test]
    fn copy_plugin_files_from_repo_copies_files_without_warning() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
        ]);

        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();
        let plugin_files = vec![PluginFile {
            dir: TargetDir::Functions,
            name: "file.fish".to_string(),
        }];
        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
        std::fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(&repo, &plugin_files);

        unsafe {
            std::env::set_var("PEZ_TARGET_DIR", &test_env.fish_config_dir);
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", test_env._temp_dir.path());
        }

        let repo_path = test_env.data_dir.join(repo.as_str());
        let (logs, result) =
            capture_logs(|| copy_plugin_files_from_repo(&repo_path, &mut test_data.plugin));
        assert!(result.is_ok());
        assert_eq!(test_data.plugin.files.len(), 1);
        assert!(
            test_env
                .fish_config_dir
                .join("functions")
                .join("file.fish")
                .exists()
        );
        assert!(!logs.iter().any(|msg| msg.contains("No valid files found")));
    }

    #[test]
    fn copy_plugin_files_creates_target_dir_when_empty() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
        std::fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(
            &repo,
            &[PluginFile {
                dir: TargetDir::Functions,
                name: "readme.txt".to_string(),
            }],
        );

        let repo_path = test_env.data_dir.join(repo.as_str());
        let outcome = copy_plugin_files(
            &repo_path,
            &test_env.fish_config_dir,
            &mut test_data.plugin,
            None,
            false,
        )
        .expect("copy should succeed");

        assert_eq!(outcome.file_count, 0);
        assert!(test_env.fish_config_dir.join("functions").exists());
    }

    #[test]
    fn copy_plugin_files_includes_themes_and_counts() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        let plugin_files = vec![
            PluginFile {
                dir: TargetDir::Functions,
                name: "tool.fish".to_string(),
            },
            PluginFile {
                dir: TargetDir::Themes,
                name: "dark.theme".to_string(),
            },
        ];
        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
        std::fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(&repo, &plugin_files);

        let repo_path = test_env.data_dir.join(repo.as_str());
        let outcome = copy_plugin_files(
            &repo_path,
            &test_env.fish_config_dir,
            &mut test_data.plugin,
            None,
            false,
        )
        .expect("copy should succeed");

        assert_eq!(outcome.file_count, 2);
        assert!(
            test_env
                .fish_config_dir
                .join("themes")
                .join("dark.theme")
                .exists()
        );
        assert!(
            test_data
                .plugin
                .files
                .iter()
                .any(|f| f.dir == TargetDir::Themes && f.name == "dark.theme")
        );
    }

    #[test]
    fn copy_plugin_files_creates_nested_directories() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        let plugin_files = vec![PluginFile {
            dir: TargetDir::Functions,
            name: "nested/dir/tool.fish".to_string(),
        }];
        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
        std::fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(&repo, &plugin_files);

        let repo_path = test_env.data_dir.join(repo.as_str());
        let outcome = copy_plugin_files(
            &repo_path,
            &test_env.fish_config_dir,
            &mut test_data.plugin,
            None,
            false,
        )
        .expect("copy should succeed");

        assert_eq!(outcome.file_count, 1);
        assert!(
            test_env
                .fish_config_dir
                .join("functions")
                .join("nested/dir/tool.fish")
                .exists()
        );
    }

    #[test]
    fn copy_plugin_files_recursive_copies_theme_files() {
        let test_env = TestEnvironmentSetup::new();
        let mut test_data = TestDataBuilder::new().build();

        let plugin_files = vec![PluginFile {
            dir: TargetDir::Themes,
            name: "bright.theme".to_string(),
        }];
        let repo = test_data.plugin_spec.get_plugin_repo().unwrap();
        std::fs::create_dir_all(test_env.data_dir.join(repo.as_str())).unwrap();
        test_env.add_plugin_files_to_repo(&repo, &plugin_files);

        let target_dir = TargetDir::Themes;
        let target_path = test_env
            .data_dir
            .join(repo.as_str())
            .join(target_dir.as_str());
        let dest_path = test_env.fish_config_dir.join(target_dir.as_str());
        std::fs::create_dir_all(&dest_path).unwrap();

        let result = copy_plugin_files_recursive(
            &target_path,
            &dest_path,
            target_dir,
            &mut test_data.plugin,
        )
        .expect("copy should succeed");
        assert_eq!(result, 1);
        assert!(dest_path.join("bright.theme").exists());
    }

    #[test]
    fn colors_enabled_for_stderr_respects_no_color() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "NO_COLOR",
            "CLICOLOR_FORCE",
            "FORCE_COLOR",
            "CLICOLOR",
            "TERM",
        ]);

        unsafe {
            std::env::set_var("NO_COLOR", "1");
            std::env::set_var("CLICOLOR_FORCE", "1");
            std::env::remove_var("FORCE_COLOR");
            std::env::remove_var("CLICOLOR");
            std::env::set_var("TERM", "xterm-256color");
        }

        assert!(!colors_enabled_for_stderr());
    }

    #[test]
    fn colors_enabled_for_stderr_force_color_overrides_term() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "NO_COLOR",
            "CLICOLOR_FORCE",
            "FORCE_COLOR",
            "CLICOLOR",
            "TERM",
        ]);

        unsafe {
            std::env::remove_var("NO_COLOR");
            std::env::set_var("CLICOLOR_FORCE", "1");
            std::env::remove_var("FORCE_COLOR");
            std::env::remove_var("CLICOLOR");
            std::env::set_var("TERM", "dumb");
        }

        assert!(colors_enabled_for_stderr());
    }

    #[test]
    fn labels_return_expected_strings() {
        assert_eq!(label_error(), "[Error]");
        assert_eq!(label_notice(), "[Notice]");
    }

    #[test]
    fn event_display_formats() {
        assert_eq!(Event::Install.to_string(), "install");
        assert_eq!(Event::Update.to_string(), "update");
        assert_eq!(Event::Uninstall.to_string(), "uninstall");
    }

    #[test]
    fn emit_event_warns_when_stem_missing() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["PEZ_SUPPRESS_EMIT"]);
        unsafe {
            std::env::remove_var("PEZ_SUPPRESS_EMIT");
        }

        let (logs, result) = capture_logs(|| emit_event("", &Event::Install));
        assert!(result.is_ok());
        assert!(
            logs.iter()
                .any(|msg| msg.contains("Could not extract plugin name"))
        );
    }

    #[cfg(unix)]
    fn open_pty() -> std::io::Result<(std::fs::File, std::fs::File)> {
        use std::os::unix::io::FromRawFd;

        unsafe {
            let master_fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            if master_fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::grantpt(master_fd) != 0 {
                let err = std::io::Error::last_os_error();
                libc::close(master_fd);
                return Err(err);
            }
            if libc::unlockpt(master_fd) != 0 {
                let err = std::io::Error::last_os_error();
                libc::close(master_fd);
                return Err(err);
            }
            let name_ptr = libc::ptsname(master_fd);
            if name_ptr.is_null() {
                let err = std::io::Error::last_os_error();
                libc::close(master_fd);
                return Err(err);
            }
            let slave_fd = libc::open(name_ptr, libc::O_RDWR | libc::O_NOCTTY);
            if slave_fd < 0 {
                let err = std::io::Error::last_os_error();
                libc::close(master_fd);
                return Err(err);
            }
            let master = std::fs::File::from_raw_fd(master_fd);
            let slave = std::fs::File::from_raw_fd(slave_fd);
            Ok((master, slave))
        }
    }

    #[cfg(unix)]
    #[test]
    fn colors_enabled_for_term_on_tty() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&[
            "NO_COLOR",
            "CLICOLOR_FORCE",
            "FORCE_COLOR",
            "CLICOLOR",
            "TERM",
        ]);

        unsafe {
            std::env::remove_var("NO_COLOR");
            std::env::remove_var("CLICOLOR_FORCE");
            std::env::remove_var("FORCE_COLOR");
            std::env::remove_var("CLICOLOR");
            std::env::set_var("TERM", "xterm-256color");
        }

        let (_master, slave) = open_pty().expect("open pty");
        let read = slave.try_clone().expect("clone slave");
        let term = console::Term::read_write_pair(read, slave);

        assert!(colors_enabled_for(&term));
    }

    #[cfg(unix)]
    #[test]
    fn emit_event_logs_error_on_failed_command() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::capture(&["PEZ_SUPPRESS_EMIT", "PATH"]);

        let temp = tempfile::tempdir().unwrap();
        let fish_path = temp.path().join("fish");
        std::fs::write(&fish_path, "#!/bin/sh\nexit 1\n").unwrap();
        let mut perms = std::fs::metadata(&fish_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fish_path, perms).unwrap();

        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", temp.path().display(), old_path.to_string_lossy());

        unsafe {
            std::env::remove_var("PEZ_SUPPRESS_EMIT");
            std::env::set_var("PATH", new_path);
        }

        let (logs, result) = capture_logs(|| emit_event("plugin.fish", &Event::Install));
        assert!(result.is_ok());
        assert!(
            logs.iter()
                .any(|msg| msg.contains("Command executed with failing error code"))
        );
    }
}
