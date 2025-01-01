pub(crate) fn run() {
    let config_dir = crate::utils::resolve_pez_config_dir();
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).unwrap();
    }

    let config_path = config_dir.join("pez.toml");
    if config_path.exists() {
        println!("{} already exists", config_path.display());
        return;
    }

    let contents = r#"# This file defines the plugins to be installed by pez.

# Example of a plugin:
# [[plugins]]
# repo = "owner/repo"  # The package identifier in the format <owner>/<repo>

# Add more plugins below by copying the [[plugins]] block.
"#;
    std::fs::write(&config_path, contents).unwrap();
    println!("Created {}", config_path.display());
}
