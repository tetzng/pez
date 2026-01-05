use crate::cli;
use clap::CommandFactory;
use std::io::{self, Write};

const FISH_DYNAMIC_COMPLETIONS: &str = r#"
# Dynamic completions for installed plugins
function __pez_installed_plugins
    command pez list --format plain 2>/dev/null
end

complete -c pez -n '__fish_seen_subcommand_from uninstall upgrade' -f -a '(__pez_installed_plugins)'
"#;

pub(crate) fn generate_fish_completion() -> anyhow::Result<Vec<u8>> {
    let buffer = build_fish_completion();
    let mut stdout = io::stdout();
    stdout.write_all(&buffer)?;
    Ok(buffer)
}

fn build_fish_completion() -> Vec<u8> {
    let mut cmd = cli::Cli::command();
    let mut buffer = Vec::new();
    clap_complete::generate(clap_complete::aot::Fish, &mut cmd, "pez", &mut buffer);
    append_dynamic_completions(buffer)
}

fn append_dynamic_completions(mut buffer: Vec<u8>) -> Vec<u8> {
    if !buffer.ends_with(b"\n") {
        buffer.push(b'\n');
    }
    let dynamic = FISH_DYNAMIC_COMPLETIONS.trim_start_matches('\n');
    buffer.extend_from_slice(dynamic.as_bytes());
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_fish_completion_returns_output() {
        let buffer = generate_fish_completion().unwrap();
        assert!(!buffer.is_empty());
    }

    #[test]
    fn build_fish_completion_emits_dynamic_section() {
        let buffer = build_fish_completion();
        let output = String::from_utf8_lossy(&buffer);
        assert!(output.contains("# Dynamic completions for installed plugins"));
        assert!(output.contains("__pez_installed_plugins"));
    }

    #[test]
    fn append_dynamic_completions_inserts_single_newline() {
        let buffer = append_dynamic_completions(b"static".to_vec());
        let output = String::from_utf8_lossy(&buffer);
        let marker = "# Dynamic completions for installed plugins";
        let idx = output
            .find(marker)
            .expect("missing dynamic completions marker");
        let prefix = &output[..idx];
        assert!(prefix.ends_with('\n'));
        assert!(!prefix.ends_with("\n\n"));
    }

    #[test]
    fn append_dynamic_completions_skips_duplicate_newline() {
        let buffer = append_dynamic_completions(b"static\n".to_vec());
        let output = String::from_utf8_lossy(&buffer);
        assert!(output.starts_with("static\n# Dynamic completions"));
    }
}
