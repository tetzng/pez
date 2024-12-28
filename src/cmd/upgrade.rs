use crate::cli::UpgradeArgs;

pub(crate) fn run(args: &UpgradeArgs) {
    if let Some(plugins) = &args.plugins {
        for plugin in plugins {
            upgrade(plugin);
        }
    } else {
        upgrade_all();
    }
}

fn upgrade(plugin: &str) {
    println!("Upgrading plugin: {}", plugin);
}

fn upgrade_all() {
    println!("Upgrading all plugins");
}
