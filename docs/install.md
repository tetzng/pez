## Install & build

### System requirements

- No mandatory external tools are required when using a prebuilt `pez` binary.
- Cargo is required only for `cargo install` or local source builds.
- Fish is required only for actually using installed Fish plugins.

### Install

Install with Cargo (from crates.io if available):

```shell
cargo install pez
```

From source (this repo):

```shell
cargo install --path .
```

From GitHub Releases (prebuilt binary, when available):

```shell
# Visit the Releases page and download the asset for your platform.
# Example (Linux x86_64):
curl -fsSL -o pez https://github.com/<owner>/<repo>/releases/download/<tag>/pez-<tag>-linux-amd64
chmod +x pez
./pez -V
```

Notes

- On tagged releases (`v*.*.*`), CI builds, tests, and uploads release artifacts.
- Asset filenames vary by platform; check the release page for the exact names.

### Build from source

```shell
cargo build --release
./target/release/pez -V
```

### Shell completions

```shell
pez completions fish > ~/.config/fish/completions/pez.fish
```

Completions are intentionally Fish-only.

### Shell activation

```shell
pez activate fish | source
```

To persist, add it inside an `if status is-interactive ... end` block in `~/.config/fish/config.fish`.
