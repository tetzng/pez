<h1 align="center">pez</h1>

<p align="center"><strong>A Rust-based plugin manager for <a href="https://fishshell.com/">fish</a></strong></p>

<p align="center">
  <em>Experimental</em> — use at your own risk.
</p>

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/tetzng/pez)

## Overview

pez is a Rust-based plugin manager for fish. It installs plugins by cloning
repositories, copying fish assets into the standard directories, and tracking
state in a lockfile.

## Features

- GitHub shorthand and non-GitHub hosts (URL or `host/owner/repo`)
- Lockfile with exact commits and installed file records
- Duplicate destination detection to avoid overwrites
- `upgrade`, `prune`, and `doctor` utilities
- Optional activation wrapper to emit conf.d events in the current shell

## Requirements

- [fish](https://fishshell.com/)
- [Cargo](https://doc.rust-lang.org/stable/cargo/)

## Installation

Ensure you have Rust installed on your system. You can install pez using Cargo:

```sh
# From crates.io (if available)
cargo install pez

# From source (in this repo)
cargo install --path .
```

## Quickstart

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
#    # Note: when specifying a relative path or ~/ at the CLI (e.g., ./plugin), pez normalizes it to an absolute path in pez.toml.

# 3) Install plugins listed in pez.toml
pez install

# 4) Verify installation
pez list --format table

# 5) (Optional) Enable completions for pez itself
pez completions fish > ~/.config/fish/completions/pez.fish

# 6) (Optional) Activate fish shell hooks (emit conf.d events in the current shell)
pez activate fish | source
```

## Shell Completions

```fish
pez completions fish > ~/.config/fish/completions/pez.fish
```

## Shell Activation

```fish
# Enable conf.d events in the current shell for install/upgrade/uninstall
pez activate fish | source
```

For persistence, add it inside an `if status is-interactive ... end` block in `~/.config/fish/config.fish`.

## Docs & FAQ

- [Getting started](docs/getting-started.md)
  - [Quick start](docs/getting-started.md#quick-start)
  - [CLI usage](docs/getting-started.md#cli-usage-examples)
- [Command reference](docs/commands.md)
- [Configuration](docs/configuration.md)
- [Architecture](docs/architecture.md)
- [Install & build](docs/install.md)
- [FAQ](docs/faq.md)

## Usage (overview)

```fish
Usage: pez [OPTIONS] <COMMAND>

Commands:
  init | install | uninstall | upgrade | list | prune | completions | activate | doctor | migrate | files

Options:
  -v, --verbose  Increase output verbosity (-v for info, -vv for debug)
  --jobs <N>     Override parallel job limit (default: 4; overrides PEZ_JOBS)
  -h, --help     Print help
  -V, --version  Print version
```

Common examples

```fish
pez init
pez install                 # install from pez.toml
pez install owner/repo      # install a specific plugin
pez upgrade                 # update non-local plugins to remote HEAD
pez list --outdated --format table
pez prune --dry-run
```

See the full command reference in [docs/commands.md](docs/commands.md).

## Configuration

pez uses `pez.toml` and `pez-lock.toml` under the fish config directory by
default. Configuration file precedence is:
`$PEZ_CONFIG_DIR` > `$__fish_config_dir` > `$XDG_CONFIG_HOME/fish` > `~/.config/fish`.

`PEZ_TARGET_DIR` only affects where plugin files are copied. For schema,
location details, and environment variables, see [docs/configuration.md](docs/configuration.md).

For install/upgrade behavior (selectors, duplicates, concurrency, existing
clones), see [docs/commands.md](docs/commands.md).

## Troubleshooting

- `pez doctor` checks config/lock/data directories and copied files.
- `pez list --format json` shows the current lockfile state.
- `pez files --all` lists installed file paths.

## Security

pez installs plugin files from third-party repositories. If you enable the
activation wrapper, `conf.d` scripts are sourced in the current shell. pez does
not verify signatures or sandbox code. Only install plugins you trust.

## Contributing

There is no formal contributing guide yet. If you want to help, open a PR and
run `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features`,
and `cargo test --all-features`.

## Changelog

No dedicated changelog is maintained yet. Use git history to review changes.

## Acknowledgements

pez is inspired by the following projects:

- [fisher](https://github.com/jorgebucaran/fisher)
- [oh-my-fish](https://github.com/oh-my-fish/oh-my-fish)
- [fundle](https://github.com/danhper/fundle)

## License

[MIT](./LICENSE)

## Author

tetzng
