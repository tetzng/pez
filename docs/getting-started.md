## Getting started

### Quick start

1) Initialize configuration (creates `pez.toml`)

```shell
pez init
```

2) Add a plugin to `pez.toml` (choose one of repo/url/path)

```toml
[[plugins]]
repo = "owner/repo"      # GitHub shorthand
# version = "v3"        # Or: tag = "...", branch = "...", commit = "..."

## Or a full Git URL
# [[plugins]]
# url = "https://gitlab.com/owner/repo"
# branch = "main"

## Or a local directory (absolute or ~/ only)
# [[plugins]]
# path = "~/path/to/local/plugin"
```

3) Install and list

```shell
pez install
pez list --format table
```

4) Optional: enable completions for pez itself

```shell
pez completions fish > ~/.config/fish/completions/pez.fish
```

### CLI usage (examples)

| Command | Purpose | Example |
| --- | --- | --- |
| `pez init` | Create `pez.toml` | `pez init` |
| `pez install` | Install from `pez.toml` | `pez install` |
| `pez install <target>` | Install a specific plugin | `pez install owner/repo@v3` |
| `pez uninstall <repo>` | Uninstall a plugin | `pez uninstall owner/repo` |
| `pez upgrade` | Update non‑local plugins to remote HEAD | `pez upgrade` |
| `pez list --outdated` | Show outdated plugins | `pez list --outdated --format json` |
| `pez doctor` | Run diagnostics | `pez doctor --format json` |

Key flag: `-v/--verbose` increases logging (`-vv` enables debug).

### Notes

- Selectors (version/branch/tag/commit) are honored on fresh installs and on `install --force`; they are not applied by `upgrade`.
- When installing explicit targets on the CLI, duplicate destination paths are skipped with a warning. When installing from `pez.toml` (no targets), existing files are overwritten.
