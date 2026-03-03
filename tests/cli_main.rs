use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;
use tempfile::tempdir;

fn pez_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pez"))
}

fn apply_test_env(
    cmd: &mut Command,
    config_dir: &std::path::Path,
    data_dir: &std::path::Path,
    target_dir: &std::path::Path,
) {
    cmd.env("PEZ_CONFIG_DIR", config_dir)
        .env("PEZ_DATA_DIR", data_dir)
        .env("PEZ_TARGET_DIR", target_dir)
        .env("PEZ_SUPPRESS_EMIT", "1")
        .env_remove("RUST_LOG");
}

fn output_text(output: &Output) -> String {
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

fn run_pez(args: &[&str], config_dir: &Path, data_dir: &Path, target_dir: &Path) -> Output {
    let mut cmd = pez_command();
    apply_test_env(&mut cmd, config_dir, data_dir, target_dir);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.output().unwrap()
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed: {}",
        output_text(output)
    );
}

fn make_local_plugin(base: &Path, name: &str) -> PathBuf {
    let plugin_dir = base.join(name);
    let conf_dir = plugin_dir.join("conf.d");
    fs::create_dir_all(&conf_dir).unwrap();
    fs::write(conf_dir.join(format!("{name}.fish")), "echo installed\n").unwrap();
    plugin_dir
}

fn parse_json_stdout(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|err| {
        panic!("failed to parse JSON output: {err}; raw stdout: {stdout}");
    })
}

#[test]
fn cli_init_creates_config() {
    let temp = tempdir().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");
    let target_dir = temp.path().join("fish");

    let mut cmd = pez_command();
    apply_test_env(&mut cmd, &config_dir, &data_dir, &target_dir);
    let output = cmd.arg("init").output().unwrap();

    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(config_dir.join("pez.toml").exists());
}

#[test]
fn cli_install_local_plugin_copies_files() {
    let temp = tempdir().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");
    let target_dir = temp.path().join("fish");
    let plugin_dir = temp.path().join("plugin");
    let conf_dir = plugin_dir.join("conf.d");
    fs::create_dir_all(&conf_dir).unwrap();
    fs::write(conf_dir.join("plugin.fish"), "echo installed").unwrap();

    let mut cmd = pez_command();
    apply_test_env(&mut cmd, &config_dir, &data_dir, &target_dir);
    let output = cmd.arg("install").arg(&plugin_dir).output().unwrap();

    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target_dir.join("conf.d").join("plugin.fish").exists());
    assert!(config_dir.join("pez.toml").exists());
    assert!(config_dir.join("pez-lock.toml").exists());
}

#[test]
fn cli_verbose_flags_keep_info_for_zero_and_one() {
    let temp = tempdir().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");
    let target_dir = temp.path().join("fish");
    let plugin_dir = temp.path().join("plugin");
    let conf_dir = plugin_dir.join("conf.d");
    fs::create_dir_all(&conf_dir).unwrap();
    fs::write(conf_dir.join("plugin.fish"), "echo installed").unwrap();

    let mut seed = pez_command();
    apply_test_env(&mut seed, &config_dir, &data_dir, &target_dir);
    let seed_output = seed.arg("install").arg(&plugin_dir).output().unwrap();
    assert!(
        seed_output.status.success(),
        "seed install failed: {}",
        String::from_utf8_lossy(&seed_output.stderr)
    );

    let mut install = pez_command();
    apply_test_env(&mut install, &config_dir, &data_dir, &target_dir);
    let output = install.arg("install").output().unwrap();
    assert!(
        output.status.success(),
        "install all failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output_text(&output).contains("Install resolved commit"));

    let mut install_verbose = pez_command();
    apply_test_env(&mut install_verbose, &config_dir, &data_dir, &target_dir);
    let output = install_verbose.arg("-v").arg("install").output().unwrap();
    assert!(
        output.status.success(),
        "install all -v failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output_text(&output).contains("Install resolved commit"));

    let mut install_debug = pez_command();
    apply_test_env(&mut install_debug, &config_dir, &data_dir, &target_dir);
    let output = install_debug.arg("-vv").arg("install").output().unwrap();
    assert!(
        output.status.success(),
        "install all -vv failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_text(&output).contains("Install resolved commit"));
}

#[test]
fn cli_migrate_install_list_doctor_flow() {
    let temp = tempdir().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");
    let target_dir = temp.path().join("fish");
    fs::create_dir_all(&target_dir).unwrap();

    let plugin_name = "plugin-migrate-flow";
    let plugin_dir = make_local_plugin(temp.path(), plugin_name);
    fs::write(
        target_dir.join("fish_plugins"),
        format!("{}\n", plugin_dir.display()),
    )
    .unwrap();

    let migrate = run_pez(&["migrate"], &config_dir, &data_dir, &target_dir);
    assert_success(&migrate, "migrate");

    let install = run_pez(&["install"], &config_dir, &data_dir, &target_dir);
    assert_success(&install, "install");

    let list = run_pez(
        &["list", "--format", "plain"],
        &config_dir,
        &data_dir,
        &target_dir,
    );
    assert_success(&list, "list");
    let list_text = String::from_utf8_lossy(&list.stdout);
    assert!(list_text.contains(&format!("local/{plugin_name}")));

    let doctor = run_pez(
        &["doctor", "--format", "json"],
        &config_dir,
        &data_dir,
        &target_dir,
    );
    assert_success(&doctor, "doctor");
    let checks = parse_json_stdout(&doctor);
    let has_error = checks.as_array().is_some_and(|arr| {
        arr.iter()
            .any(|check| check.get("status").and_then(|v| v.as_str()) == Some("error"))
    });
    assert!(!has_error, "doctor reported error: {}", checks);

    assert!(
        target_dir
            .join("conf.d")
            .join(format!("{plugin_name}.fish"))
            .exists()
    );
    assert!(config_dir.join("pez-lock.toml").exists());
}

#[test]
fn cli_migrate_with_install_flag_completes_flow() {
    let temp = tempdir().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");
    let target_dir = temp.path().join("fish");
    fs::create_dir_all(&target_dir).unwrap();

    let plugin_name = "plugin-migrate-install-flag";
    let plugin_dir = make_local_plugin(temp.path(), plugin_name);
    fs::write(
        target_dir.join("fish_plugins"),
        format!("{}\n", plugin_dir.display()),
    )
    .unwrap();

    let migrate_install = run_pez(
        &["migrate", "--install"],
        &config_dir,
        &data_dir,
        &target_dir,
    );
    assert_success(&migrate_install, "migrate --install");

    let list = run_pez(
        &["list", "--format", "plain"],
        &config_dir,
        &data_dir,
        &target_dir,
    );
    assert_success(&list, "list");
    let list_text = String::from_utf8_lossy(&list.stdout);
    assert!(list_text.contains(&format!("local/{plugin_name}")));

    let doctor = run_pez(
        &["doctor", "--format", "json"],
        &config_dir,
        &data_dir,
        &target_dir,
    );
    assert_success(&doctor, "doctor");
    let checks = parse_json_stdout(&doctor);
    let has_error = checks.as_array().is_some_and(|arr| {
        arr.iter()
            .any(|check| check.get("status").and_then(|v| v.as_str()) == Some("error"))
    });
    assert!(!has_error, "doctor reported error: {}", checks);

    assert!(config_dir.join("pez-lock.toml").exists());
    assert!(
        target_dir
            .join("conf.d")
            .join(format!("{plugin_name}.fish"))
            .exists()
    );
}

#[test]
fn cli_migrate_dry_run_install_does_not_mutate() {
    let temp = tempdir().unwrap();
    let config_dir = temp.path().join("config");
    let data_dir = temp.path().join("data");
    let target_dir = temp.path().join("fish");
    fs::create_dir_all(&target_dir).unwrap();

    let plugin_name = "plugin-migrate-dry-run";
    let plugin_dir = make_local_plugin(temp.path(), plugin_name);
    fs::write(
        target_dir.join("fish_plugins"),
        format!("{}\n", plugin_dir.display()),
    )
    .unwrap();

    let dry_run = run_pez(
        &["migrate", "--dry-run", "--install"],
        &config_dir,
        &data_dir,
        &target_dir,
    );
    assert_success(&dry_run, "migrate --dry-run --install");
    let output = output_text(&dry_run);
    assert!(output.contains("Next steps"));

    assert!(!config_dir.join("pez-lock.toml").exists());
    assert!(
        !target_dir
            .join("conf.d")
            .join(format!("{plugin_name}.fish"))
            .exists()
    );
}
