use crate::cli::UninstallArgs;

pub(crate) fn run(args: &UninstallArgs) {
    if args.plugins.is_empty() {
        eprintln!("No plugins specified");
        return;
    }

    for plugin in &args.plugins {
        uninstall(plugin);
    }
}

pub(crate) fn uninstall(plugin: &str) {
    println!("Uninstalling plugin: {}", plugin);
}
