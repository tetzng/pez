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

5) Optional: enable fish in-shell hooks (conf.d events)

```shell
pez activate fish | source
```

To persist, add it inside an interactive block in `~/.config/fish/config.fish`.

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
| `pez activate fish` | Enable fish in-shell hooks | `pez activate fish | source` |
| `pez files --all` | List installed files | `pez files --all` |

Key flag: `-v/--verbose` increases logging (`-vv` enables debug).

### Notes

- Selectors (version/branch/tag/commit) are honored across installs and by `upgrade`. When no selector is set, `upgrade` updates to the latest commit on the remote default branch (remote HEAD).
- Duplicate destination paths are detected for both CLI targets and installs from `pez.toml`. Conflicting plugins are skipped with a warning to avoid overwriting existing files.
