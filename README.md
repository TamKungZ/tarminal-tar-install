
## Workspace split

This project is split into two Cargo packages:

```text
tar-install  = library/core package
tarminal     = CLI frontend that depends on tar-install
```

Build them separately:

```bash
cargo build -p tar-install --lib
cargo build -p tar-install --bin tar-install
cargo build -p tarminal
```

`tar-install` also includes a tiny hint command. If someone runs `tar-install ...`, it tells them to install/use `tarminal`.

# tar-install + Tarminal

`tar-install` is the library core. `tarminal` is the CLI front-end.

Goal: make Linux app tarballs behave like installed desktop apps instead of “extract and run manually”.

It installs a tarball into the right places, creates a command, creates a menu entry, installs an icon, remembers state, and can remove the app cleanly.

## Current scope

Supported archive formats in this prototype:

- `.tar.xz`, `.txz`
- `.tar.gz`, `.tgz`
- `.tar.bz2`, `.tbz2`
- `.tar`

Install targets:

### User install, default

```text
App files:   ~/.local/share/tarapp/apps/<app-id>/
Command:     ~/.local/bin/<command>
Desktop:     ~/.local/share/applications/<app-id>.desktop
Icon:        ~/.local/share/icons/hicolor/256x256/apps/<app-id>.*
State DB:    ~/.local/state/tarapp/apps.json
```

### System install

```text
App files:   /opt/<app-id>/
Command:     /usr/local/bin/<command>
Desktop:     /usr/share/applications/<app-id>.desktop
Icon:        /usr/share/icons/hicolor/256x256/apps/<app-id>.*
State DB:    /var/lib/tarapp/apps.json
```

## Build

```bash
cargo build --release
```

CLI binary:

```text
target/release/tarminal
```

Library crate:

```rust
use tar_install::install_archive;
```

## Usage

Inspect a tarball:

```bash
tarminal inspect ./myapp-1.2.0-linux-x64.tar.xz
```

Install with automatic detection:

```bash
tarminal install ./myapp-1.2.0-linux-x64.tar.xz
```

Install and answer questions manually:

```bash
tarminal install ./myapp.tar.xz --config
```

Install using a community recipe:

```bash
tarminal install ./myapp.tar.xz --recipe ./examples/myapp.tarapp.yml
```

System install:

```bash
sudo tarminal install ./myapp.tar.xz --system
```

List:

```bash
tarminal list
```

Remove:

```bash
tarminal remove myapp
```

Doctor:

```bash
tarminal doctor myapp
```

## Recipe format

```yaml
id: com.example.myapp
name: My App
version: 1.0.0
exec: MyApp
command: myapp
icon: assets/icon.png
desktop:
  categories:
    - Utility
  terminal: false
```

A recipe can be external, or embedded inside the archive as one of:

```text
tarapp.yml
tarapp.yaml
.tarapp.yml
.tarapp.yaml
manifest.yml
manifest.yaml
```

## Safety behavior

This prototype refuses to install archives with:

- absolute paths
- `..` path traversal
- symlink or hard-link entries during extraction

The symlink rule is intentionally strict for now. Later versions can support safe symlinks that resolve inside the install directory.

## Project shape

```text
src/lib.rs              library exports
src/main.rs             Tarminal CLI
src/archive.rs          inspect tarball, detect binary/icon/manifest
src/filename.rs         parse <app>-<version>-<os>-<arch>
src/install.rs          install/remove/doctor logic
src/desktop.rs          .desktop generation
src/paths.rs            user/system install targets
src/recipe.rs           manifest/recipe schema
src/state.rs            installed apps database
```

## Notes

This is a starter implementation designed for iteration. The important architecture is already separated:

- `tar-install` can be reused by other GUIs/tools
- `tarminal` is only a CLI wrapper
- recipes are optional but first-class
- `--config` lets users fix bad filename/archive detection
