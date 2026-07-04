# Packaging layout

This repository is a Cargo workspace with two packages:

- `tar-install` — library/core package. It also ships a tiny `tar-install` binary that only tells users to install/use `tarminal`.
- `tarminal` — real CLI frontend. It depends on `tar-install`.

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

## Suggested package split

For distro packaging, the clean split is:

```text
tar-install
  /usr/bin/tar-install        # tiny hint command
  /usr/share/doc/tar-install  # docs/license

# Optional if you expose Rust library sources or -dev package:
tar-install-dev
  Rust crate source / metadata, if your packaging target supports that style

tarminal
  /usr/bin/tarminal
  Depends: tar-install
```

Note: normal Rust binaries statically link Rust crate dependencies, so `tarminal` does not technically need `tar-install` at runtime unless you intentionally package `tar-install` as a user-visible core/helper package. If you want `apt install tarminal` to also install `tar-install`, set a package dependency anyway.

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
