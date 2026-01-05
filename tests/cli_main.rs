use std::{
    fs,
    process::{Command, Output},
};

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
