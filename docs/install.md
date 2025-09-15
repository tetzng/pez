## Install & build

### System requirements

- Fish shell (for using the installed plugins)
- Git (for cloning plugin repositories)
- Rust toolchain (stable), Cargo available

### Install

Install with Cargo (from crates.io if available):

```shell
cargo install pez
```

From source (this repo):

```shell
cargo install --path .
```

### Build from source

```shell
cargo build --release
./target/release/pez -V
```

### Shell completions

```shell
pez completions fish > ~/.config/fish/completions/pez.fish
```
