# pez

A lockfile-backed plugin manager for fish.

pez installs fish plugins from Git repositories or local paths, copies their
fish assets into the standard config directories, and records the exact
installed state in `pez-lock.toml`.

Status: experimental. Back up your fish config before migrating, and verify the
result with `pez doctor` before removing another plugin manager.

## Why pez?

- Lockfile state for installed commits and copied files
- GitHub shorthand, host-prefixed repos, full Git URLs, and local plugin paths
- Run-level duplicate destination checks for copied plugin files
- `doctor`, `files`, `list --outdated`, `upgrade`, and `prune` commands for routine maintenance
- A guided migration path from fisher

## Install

Use a prebuilt binary from [GitHub Releases](https://github.com/tetzng/pez/releases)
when one is available.

With Cargo:

```sh
cargo install pez
```

From this checkout:

```sh
cargo install --path .
```

With Nix flakes:

```sh
nix run github:tetzng/pez -- --version
```

More options: [docs/install.md](docs/install.md)

## Quick Start

Create the config file:

```fish
pez init
```

Add a plugin to `pez.toml`:

```toml
[[plugins]]
repo = "owner/repo"
```

Install and inspect:

```fish
pez install
pez list --format table
pez doctor
```

Install a single plugin directly:

```fish
pez install owner/repo
pez install gitlab.com/owner/repo@branch:main
pez install ./local-plugin
```

Enable hooks in the current shell when plugins rely on `conf.d` events:

```fish
pez activate fish | source
```

Persist that activation from `~/.config/fish/config.fish` inside an
interactive-shell guard.

## Migrate from fisher

Use the low-risk path:

```fish
pez activate fish | source
pez migrate
pez install
pez list --format table
pez doctor
```

Only remove fisher after the migrated setup looks correct.

Details: [docs/migrate-from-fisher.md](docs/migrate-from-fisher.md)

## Docs

- [Getting started](docs/getting-started.md)
- [Command reference](docs/commands.md)
- [Configuration and lockfile](docs/configuration.md)
- [Install and build](docs/install.md)
- [Migrate from fisher](docs/migrate-from-fisher.md)
- [FAQ](docs/faq.md)
- [Architecture](docs/architecture.md)

## Security

pez installs code from third-party repositories into your fish configuration.
It does not verify signatures or sandbox plugin code. Install plugins you trust,
review migration changes before removing fisher, and keep `pez doctor` in your
verification flow.

## License

[MIT](./LICENSE)
