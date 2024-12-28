use tabled::{Table, Tabled};

use crate::cli::ListArgs;

#[derive(Debug, Tabled)]
struct PluginRow {
    name: String,
    repo: String,
    source: String,
    commit: String,
}

pub(crate) fn run(args: &ListArgs) {
    if args.outdated {
        list_outdated();
    } else {
        list();
    }
}

fn list() {
    // lock fileからpluginsを取得して、tableで表示

    // | name | sorce | commit |
    // |------|-------|--------|
    // | plugin.get_name()| plugin.source | plugin.commit_sha |
    let lock_file_path = crate::utils::resolve_lock_file_path();
    if lock_file_path.exists() {
        let lock_file = crate::lock_file::load(&lock_file_path);
        // tabledを使って表示
        let plugins = lock_file
            .plugins
            .iter()
            .map(|p| PluginRow {
                name: p.get_name(),
                repo: p.source.clone(),
                source: p.source.clone(),
                commit: p.commit_sha.clone(),
            })
            .collect::<Vec<PluginRow>>();
        let table = Table::new(&plugins);
        println!("{table}");
    } else {
        println!("No plugins installed");
    }
}

fn list_outdated() {
    println!("Listing outdated plugins");
}
