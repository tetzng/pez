pub(crate) fn run() {
    let pez_config_dir = crate::utils::resolve_pez_config_dir();
    if !pez_config_dir.exists() {
        std::fs::create_dir_all(&pez_config_dir).unwrap();
    }

    let pez_toml_path = pez_config_dir.join("pez.toml");
    if pez_toml_path.exists() {
        println!("{} already exists", pez_toml_path.display());
        return;
    }

    let contents = r#"# This file defines the plugins to be installed by pez.

# Example of a plugin:
# [[plugins]]
# repo = "owner/repo"  # The package identifier in the format <owner>/<repo>

# Add more plugins below by copying the [[plugins]] block.
"#;
    std::fs::write(&pez_toml_path, contents).unwrap();
    println!("Created {}", pez_toml_path.display());
}
