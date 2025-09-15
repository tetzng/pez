# pez

> [!WARNING]
> This project is still in development and may not be stable. Use at your own risk.

A Rust-based plugin manager for [fish](https://fishshell.com/)

## Installation

Ensure you have Rust installed on your system. You can install pez using Cargo:

```sh
# From crates.io (if available)
cargo install pez

# From source (in this repo)
cargo install --path .
```

## Quick Start

```fish
# 1) Initialize configuration (creates pez.toml)
pez init

# 2) Add a plugin to pez.toml (choose one of repo/url/path)
#    [[plugins]]
#    repo = "owner/repo"      # GitHub shorthand
#    # version = "v3"        # Or: tag = "...", branch = "...", commit = "..."
#
#    # [[plugins]]
#    # url = "https://gitlab.com/owner/repo"  # Any Git host URL
#    # branch = "main"
#
#    # [[plugins]]
#    # path = "~/path/to/local/plugin"       # Local directory (absolute or ~/ only)
#    # Note: when specifying a relative path at the CLI (e.g., ./plugin), pez normalizes it to an absolute path in pez.toml.

# 3) Install plugins listed in pez.toml
pez install

# 4) Verify installation
pez list --format table

# 5) (Optional) Enable completions for pez itself
pez completions fish > ~/.config/fish/completions/pez.fish
```

## Shell Completions

```fish
pez completions fish > ~/.config/fish/completions/pez.fish
```

## Usage

```fish
Usage: pez [OPTIONS] <COMMAND>

Commands:
init         Initialize pez
install      Install fish plugin(s)
uninstall    Uninstall fish plugin(s)
upgrade      Upgrade installed fish plugin(s)
list         List installed fish plugins
prune        Prune uninstalled plugins
completions  Generate shell completion scripts
doctor       Diagnose common setup issues
migrate      Migrate from fisher (reads fish_plugins)
help         Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Increase output verbosity (-v for info, -vv for debug)
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

# Remove plugins present in pez-lock.toml but not in pez.toml
pez install --prune

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
pez uninstall owner/package1 owner/package2

# Remove orphaned files even if repo dir is missing
pez uninstall --force owner/package1
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

# Output format: plain (default), table, json
pez list --format table
pez list --outdated --format json

# Filter by regex (matches name/repo/source)
pez list --filter 'tide|fzf'
 
# Note: table/json output includes a `selector` column derived from pez.toml
# (version:/branch:/tag:/commit:/latest or local/pinned).
```

### prune

```fish
# Prune uninstalled plugins
pez prune

# Dry run to see what would be pruned
pez prune --dry-run

# Skip confirmation when pez.toml is empty
pez prune --yes
```

### doctor

```fish
# Run diagnostics (checks config, lockfile, data dir, target files)
pez doctor

# JSON output
pez doctor --format json
```

### migrate (from fisher)

```fish
# Reads fish_plugins and updates pez.toml
pez migrate

# Write changes and immediately install migrated plugins
pez migrate --install

# Preview changes without writing files
pez migrate --dry-run

# Overwrite existing pez.toml plugin list instead of merging
pez migrate --force
```

## Configuration

pez uses two main configuration files: `pez.toml` and `pez-lock.toml.`
By default, these files are created in the fish configuration directory,
but you can specify a different location using environment variables.

Configuration File Locations

The configuration files are located based on the following priority:
`$PEZ_CONFIG_DIR` > `$__fish_config_dir` > `$XDG_CONFIG_HOME/fish` > `~/.config/fish`

### pez.toml (Schema)

`pez.toml` is the primary configuration file where you define the plugins
you want to manage. Below is an example structure:

```toml
# GitHub shorthand
[[plugins]]
repo = "owner/repo"
# version = "latest"   # default if omitted
# version = "v3"       # branch or tag name; branches preferred over tags
# branch  = "develop"
# tag     = "v1.0.0"
# commit  = "<sha>"    # 7+ chars recommended (unique per repo)

# Generic Git host URL
[[plugins]]
url = "https://gitlab.com/owner/repo"
# branch = "main"

# Local path (absolute or ~/ only)
[[plugins]]
path = "~/path/to/local/plugin"
```

### pez-lock.toml

`pez-lock.toml` is automatically generated and maintained by pez.
It records detailed information about the installed plugins,
including their source repositories and specific commit SHAs.
Do not edit this file manually.

### Custom Target Directory

By default, pez copies files into your Fish config directory. To override the
destination, set `PEZ_TARGET_DIR` to a custom base directory. For example:

```sh
export PEZ_TARGET_DIR="$HOME/.local/share/my-fish"
```

Themes are discovered by `fish_config` under `$__fish_config_dir/themes`. If you
customize `PEZ_TARGET_DIR`, consider symlinking the `themes` directory into your
Fish config dir so themes appear in `fish_config`.

## Data Directory

pez clones plugin repositories into a designated data directory,
prioritized as follows:
`$PEZ_DATA_DIR` > `$__fish_user_data_dir/pez` > `$XDG_DATA_HOME/fish/pez` > `~/.local/share/fish/pez`

When you install a plugin, pez clones its repository into `pez_data_dir`.
If the directory doesn’t exist, pez will create it.
If the repository is already cloned, pez will notify you and skip cloning
unless you use the --force option to re-clone it.

After cloning, if the repository contains functions, completions, conf.d,
or themes directories, pez will recursively copy files from these directories
to the corresponding fish configuration directories:

- `~/.config/fish/functions`
- `~/.config/fish/completions`
- `~/.config/fish/conf.d`
- `~/.config/fish/themes`

If a file with the same name already exists in the destination,
pez will overwrite it.

The destination fish configuration directory can be overridden
using the following environment variables:
`$__fish_config_dir` > `$XDG_CONFIG_HOME/fish` > `~/.config/fish`

Additionally, `pez-lock.toml` records information about the installed packages
and the files copied. It is created in the same directory as `pez.toml`
and will append information if it already exists.

### Concurrency

Control job parallelism for installs/uninstalls with `PEZ_JOBS` (default: 4).

## Acknowledgements

pez is inspired by the following projects:

- [fisher](https://github.com/jorgebucaran/fisher)
- [oh-my-fish](https://github.com/oh-my-fish/oh-my-fish)
- [fundle](https://github.com/danhper/fundle)

## License

MIT

## Author

tetzng
