# Packaging layout

This repository is a Cargo workspace with two packages:

- `tar-install` — library/core package. It also ships a tiny `tar-install` binary that only tells users to install/use `tarminal`.
- `tarminal` — real CLI frontend. It depends on `tar-install`.
- `pylib-tar-install` — Python bindings that bundle or locate `libtar_install.so`.

## Build separately

```bash
cargo build -p tar-install --lib
cargo build -p tar-install --bin tar-install
cargo build -p tarminal
```

Release builds:

```bash
cargo build --release -p tar-install --lib
cargo build --release -p tar-install --bin tar-install
cargo build --release -p tarminal
```

The Rust library package also produces a shared library artifact for bindings:

```text
target/debug/libtar_install.so
target/release/libtar_install.so
```

## Suggested package split

For distro packaging, the clean split is:

```text
tar-install
  /usr/bin/tar-install        # tiny hint command
  libtar_install.so           # only if the distro package is meant to expose bindings
  /usr/share/doc/tar-install  # docs/license

# Optional if you expose Rust library sources or -dev package:
tar-install-dev
  Rust crate source / metadata, if your packaging target supports that style

tarminal
  /usr/bin/tarminal
  Depends: tar-install
```

Note: normal Rust binaries statically link Rust crate dependencies, so `tarminal` does not technically need `tar-install` at runtime unless you intentionally package `tar-install` as a user-visible core/helper package. If you want `apt install tarminal` to also install `tar-install`, set a package dependency anyway.

## Local package builds

Install helper tools:

```bash
cargo install cargo-deb cargo-generate-rpm
```

Build Debian packages:

```bash
cargo deb -p tar-install
cargo deb -p tarminal
```

Build RPM packages:

```bash
cargo build --release -p tar-install --bin tar-install
cargo build --release -p tarminal
cargo generate-rpm -p crates/tar-install
cargo generate-rpm -p crates/tarminal
```

The release workflow also builds Alpine `.apk`, Void `.xbps`, Arch
`.pkg.tar.zst`, Snap, Flatpak, and standalone `.tar.xz` binary archives.

## Publish targets

Tagged GitHub releases publish several outputs when the matching secrets are
configured:

- GitHub Release assets
- package repositories under `https://packages.tamkungz.me/`
- OBS sources for openSUSE Tumbleweed
- AUR `PKGBUILD` updates
- `tar-install` on crates.io
- `tar-install` Python bindings on PyPI through the `pylib-tar-install` workflow

Required or optional secrets are documented by the workflow names in
`.github/workflows/release.yml`. Missing optional publish credentials cause
that publish target to be skipped where the workflow supports skipping.

## User-facing behavior

If someone runs:

```bash
tar-install ...
```

They will see:

```text
tar-install is the library/core package for Tarminal.
To install and manage .tar/.tar.xz apps, install and run: tarminal
```
