use clap::CommandFactory;

pub(crate) fn generate_completion<G: clap_complete::Generator>(gen: G) {
    let mut cmd = crate::cli::Cli::command();
    clap_complete::generate(gen, &mut cmd, "pez", &mut std::io::stdout());
}
