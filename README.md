# pez

A Rust-Based Plugin Manager for fish

## Installation

Ensure you have Rust installed on your system. You can install pez using Cargo:

```sh
cargo install pez
```

## Completions

```fish
pez completions fish > ~/.config/fish/completions/pez.fish
```

## Usage

```
Usage: pez <COMMAND>

Commands:
  init       Initialize pez
  install    Install fish plugin(s)
  uninstall  Uninstall fish plugin(s)
  upgrade    Upgrade installed fish plugin(s)
  list       List installed fish plugins
  prune      Prune uninstalled plugins
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### init

```fish
# Initialize pez
pez init
```

### install

```fish
# Install plugins listed in pez.toml.
pez install

# Force reinstalling plugins even if they are already installed
pez install --force

# Install a specific plugin
pez install owner/package1

# Install multiple plugins at once
pez install owner/package1 owner/package2
```

### uninstall

```fish
# Uninstall a single plugin
pez uninstall owner/package1

# Uninstall multiple plugins
pez uninstall owner/package1 owner/package
```

### upgrade

```fish
# Upgrade all installed plugins
pez upgrade

# Upgrade a specific plugin
pez upgrade owner/package1

# Upgrade multiple plugins at once
pez upgrade owner/package1 owner/package2
```

### list

```fish
# List all installed plugins
pez list

# List only outdated plugins
pez list --outdated
```

### prune

```fish
# Prune uninstalled plugins
pez prune

# Dry run to see what would be pruned
pez prune --dry-run
```

## Configuration

pez uses two main configuration files: `pez.toml` and `pez-lock.toml.` By default, these files are created in the fish configuration directory, but you can specify a different location using environment variables.

Configuration File Locations

The configuration files are located based on the following priority: `$PEZ_CONFIG_DIR` > `$__fish_config_dir` > `$XDG_CONFIG_HOME/fish` > `~/.config/fish`

### pez.toml

`pez.toml` is the primary configuration file where you define the plugins you want to manage. Below is an example structure:

```toml
[[plugins]]
repo = "owner/repo" # The plugin repository in the format <owner>/<repo>

# Add additional plugins by copying the [[plugins]] block.
```

### pez-lock.toml

`pez-lock.toml` is automatically generated and maintained by pez. It records detailed information about the installed plugins, including their source repositories and specific commit SHAs. Do not edit this file manually.

## Data Directory

pez clones plugin repositories into a designated data directory, prioritized as follows: `$PEZ_DATA_DIR` > `$__fish_user_data_dir/pez` > `$XDG_DATA_HOME/fish/pez` > `~/.local/share/fish/pez`

When you install a plugin, pez clones its repository into pez_data_dir. If the directory doesn’t exist, pez will create it. If the repository is already cloned, pez will notify you and skip cloning unless you use the --force option to re-clone it.

After cloning, if the repository contains functions, completions, conf.d, or themes directories, pez will copy the files from these directories to the corresponding fish configuration directories:

- `~/.config/fish/functions`
- `~/.config/fish/completions`
- `~/.config/fish/conf.d`
- `~/.config/fish/themes`

If a file with the same name already exists in the destination, pez will overwrite it.

The destination fish configuration directory can be overridden using the following environment variables: `$__fish_config_dir` > `$XDG_CONFIG_HOME/fish` > `~/.config/fish`

Additionally, `pez-lock.toml` records information about the installed packages and the files copied. It is created in the same directory as `pez.toml` and will append information if it already exists.

## License

MIT

## Author

tetzng
