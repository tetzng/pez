pub(crate) fn run() {
    let pez_config_dir = crate::utils::resolve_pez_config_dir();
    if !pez_config_dir.exists() {
        std::fs::create_dir_all(&pez_config_dir).unwrap();
    }

    let pez_toml_path = pez_config_dir.join("pez.toml");
    if std::fs::metadata(&pez_toml_path).is_ok() {
        println!("{} already exists", pez_toml_path.display());
        return;
    }

    let contents = r#"
# This file defines the plugins to be installed by pez.

# Example of a plugin:
# [[plugins]]
# package = "owner/repo"                    # The package identifier in the format <owner>/<repo>
# version = "1.0.0"                         # Optional: Specify a version. Defaults to the latest.
# name = "repo"                             # Optional: Set a custom name for the package. Defaults to the repository name.
# source = "https://github.com/owner/repo"  # Optional: Specify a custom source URL.

# Add more plugins below by copying the [[plugins]] block.
"#;
    std::fs::write(&pez_toml_path, contents).unwrap();
    println!("Created {}", pez_toml_path.display());
}
