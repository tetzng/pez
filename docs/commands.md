# Command Reference

```text
pez [OPTIONS] <COMMAND>
```

Global options:

| Option | Meaning |
| --- | --- |
| `-v`, `--verbose` | Increase log verbosity. Use `-vv` for debug logs. |
| `--jobs <N>` | Override parallel work for explicit install clones, `upgrade`, `uninstall`, and `prune`. Must be at least 1. |
| `-h`, `--help` | Print help. |
| `-V`, `--version` | Print version. |

`PEZ_JOBS` provides the default job count when `--jobs` is not set. The fallback
is 4.

## init

```sh
pez init
```

Creates `pez.toml` in the config directory. The command fails if the file
already exists.

## install

```sh
pez install [OPTIONS] [PLUGINS]...
```

Installs plugins from CLI targets or, when no targets are provided, from
`pez.toml`.

Accepted CLI targets:

- `owner/repo[@ref]`
- `host/owner/repo[@ref]`
- Full Git URLs such as `https://example.com/owner/repo.git`
- Local paths beginning with `/`, `~/`, `./`, or `../`

Options:

| Option | Meaning |
| --- | --- |
| `-f`, `--force` | Reinstall even when the plugin already exists. |
| `-p`, `--prune` | After a config-driven install, remove lockfile entries no longer listed in `pez.toml`. Cannot be used with explicit plugin targets. |

Selector syntax for shorthand and host-prefixed targets:

| Selector | Meaning |
| --- | --- |
| `owner/repo@v3` | Version selector. Branches are preferred over tags with the same name. |
| `owner/repo@branch:main` | Branch selector. |
| `owner/repo@tag:v1.2.3` | Tag selector. |
| `owner/repo@commit:<sha>` | Commit selector. |
| `owner/repo@latest` | Remote default branch. |

Full URLs are treated literally and are not split on `@ref`. Use `pez.toml` to
pin URL sources.

Install behavior:

- CLI targets are appended to `pez.toml` if they are not already present.
- CLI relative paths and `~/` paths are normalized before they are saved.
- Explicit remote targets clone concurrently, bounded by `--jobs` or `PEZ_JOBS`.
- Config-driven installs run sequentially.
- File copying is sequential so duplicate destinations can be detected
  consistently.
- If two plugins would copy to the same destination, the later plugin is
  skipped and is not recorded in the lockfile.

## uninstall

```sh
pez uninstall [OPTIONS] [PLUGINS]...
```

Removes plugin files recorded in `pez-lock.toml`, removes the cached repository
when present, and removes the matching entry from `pez.toml`.

Options:

| Option | Meaning |
| --- | --- |
| `-f`, `--force` | Remove recorded files even if the cached repository is missing. |
| `--stdin` | Read plugin repos from standard input, one per line. Blank lines and comments are ignored. |

Example:

```sh
printf "owner/a\nowner/b\n" | pez uninstall --stdin
```

## upgrade

```sh
pez upgrade [PLUGINS]...
```

Updates remote plugins. With no arguments, upgrades plugins listed in
`pez.toml`. With arguments, upgrades the named repos and adds missing repos to
`pez.toml` so future installs stay in sync.

Rules:

- Local path plugins are skipped.
- `version`, `branch`, `tag`, and `commit` selectors in `pez.toml` are
  respected.
- Plugins without selectors update to the latest commit on the remote default
  branch.
- Work is bounded by `--jobs` or `PEZ_JOBS`.

## list

```sh
pez list [OPTIONS]
```

Shows plugins recorded in `pez-lock.toml`.

Options:

| Option | Values | Meaning |
| --- | --- | --- |
| `--format` | `plain`, `table`, `json` | Output format. |
| `--outdated` | | Compare remote plugins with their latest selected revision. |
| `--filter` | `all`, `local`, `remote` | Filter by source kind. |

Output fields:

- Normal table/json: `name`, `repo`, `source`, `selector`, `commit`
- Outdated table/json: `name`, `repo`, `source`, `current`, `latest`

## prune

```sh
pez prune [OPTIONS]
```

Removes plugins that exist in `pez-lock.toml` but are no longer listed in
`pez.toml`.

Options:

| Option | Meaning |
| --- | --- |
| `-f`, `--force` | Remove recorded files even if the cached repository is missing. |
| `--dry-run` | Print what would be removed. |
| `-y`, `--yes` | Confirm prompts. |

If `pez.toml` has no plugin entries, `prune` asks for confirmation unless
`--yes` is set.

## doctor

```sh
pez doctor [--format json]
```

Checks setup state and reports common problems.

Checks include config and lockfile readability, fish config/data directories,
activation readiness, install layout, missing cached repos, missing target
files, duplicate destinations, and theme assets.

## files

```sh
pez files [OPTIONS] [PLUGINS]...
```

Lists installed files recorded in `pez-lock.toml`.

Options:

| Option | Values | Meaning |
| --- | --- | --- |
| `--all` | | List files for all installed plugins. |
| `--dir` | `all`, `conf.d` | Filter by destination directory. Default: `all`. |
| `--format` | `paths`, `json` | Output format. Default: `paths`. |
| `--from` | `install`, `update`, `upgrade`, `uninstall`, `remove` | Parse plugin targets from another command's argv. |

Examples:

```sh
pez files --all
pez files owner/repo --dir conf.d
pez files --from install -- owner/repo@v3
printf "owner/a\n" | pez files --from uninstall -- --stdin
```

## completions

```sh
pez completions fish > ~/.config/fish/completions/pez.fish
```

Generates fish completions. Other shells are not supported.

## activate

```fish
pez activate fish | source
```

Prints a fish wrapper for `pez` so install, upgrade, and uninstall hooks can run
in the current shell.

When active:

- `install` and `upgrade` source affected `conf.d` files and emit
  `<stem>_install` or `<stem>_update`.
- `uninstall` emits `<stem>_uninstall` before running the raw uninstall command.
- The wrapper sets `PEZ_SUPPRESS_EMIT=1` to avoid duplicate out-of-process
  emits.

Out-of-process emits only run for safe stems made from `A-Z`, `a-z`, `0-9`,
`_`, `-`, and `.`.

## migrate

```sh
pez migrate [OPTIONS]
```

Imports fisher's `fish_plugins` into `pez.toml`.

Options:

| Option | Meaning |
| --- | --- |
| `--dry-run` | Print the planned config changes without writing files. |
| `--force` | Replace the current plugin list instead of merging into it. |
| `--install` | Install migrated plugins immediately after writing `pez.toml`. |

Migration skips `jorgebucaran/fisher`, ignores comments and blank lines, and
preserves supported `@ref` suffixes from `fish_plugins`.
