use crate::utils;
use std::{fs, process};

pub(crate) fn run() -> anyhow::Result<()> {
    let config_dir = utils::load_pez_config_dir()?;
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }

    let config_path = config_dir.join("pez.toml");
    if config_path.exists() {
        eprintln!("{} already exists", config_path.display());
        process::exit(1);
    }

    let contents = r#"# This file defines the plugins to be installed by pez.

# Example of a plugin:
# [[plugins]]
# repo = "owner/repo"  # The package identifier in the format <owner>/<repo>

# Add more plugins below by copying the [[plugins]] block.
"#;
    fs::write(&config_path, contents)?;
    println!("Created {}", config_path.display());
    Ok(())
}
