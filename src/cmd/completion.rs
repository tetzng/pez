use crate::cli;
use clap::CommandFactory;
use std::io;

pub(crate) fn generate_completion<G: clap_complete::Generator>(r#gen: G) {
    let mut cmd = cli::Cli::command();
    clap_complete::generate(r#gen, &mut cmd, "pez", &mut io::stdout());
}
