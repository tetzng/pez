use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("gen-config-schema") => gen_config_schema(),
        Some(command) => anyhow::bail!("unknown xtask command: {command}"),
        None => anyhow::bail!("missing xtask command"),
    }
}

fn gen_config_schema() -> anyhow::Result<()> {
    let output_path = workspace_root().join("config.schema.json");
    pez::schema::write_config_schema(&output_path)?;

    println!("Wrote {}", output_path.display());
    Ok(())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest should be under workspace root")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct RestoreFile {
        path: PathBuf,
        original: String,
    }

    impl Drop for RestoreFile {
        fn drop(&mut self) {
            let _ = fs::write(&self.path, &self.original);
        }
    }

    #[test]
    fn gen_config_schema_overwrites_schema_file_with_generated_content() {
        let output_path = workspace_root().join("config.schema.json");
        let original = fs::read_to_string(&output_path).expect("read existing schema");
        let _guard = RestoreFile {
            path: output_path.clone(),
            original,
        };

        fs::write(&output_path, "not a schema").expect("seed schema content");
        gen_config_schema().expect("run schema generator");

        let generated = fs::read_to_string(&output_path).expect("read generated schema");
        assert_ne!(generated, "not a schema");
        assert!(generated.contains("\"$schema\""));
        assert!(generated.contains("\"plugins\""));
    }
}
