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

pub(crate) fn generate_fish_completion() -> anyhow::Result<()> {
    let mut cmd = cli::Cli::command();
    let mut buffer = Vec::new();
    clap_complete::generate(clap_complete::aot::Fish, &mut cmd, "pez", &mut buffer);

    let mut stdout = io::stdout();
    stdout.write_all(&buffer)?;
    if !buffer.ends_with(b"\n") {
        stdout.write_all(b"\n")?;
    }
    stdout.write_all(FISH_DYNAMIC_COMPLETIONS.as_bytes())?;
    Ok(())
}
