# Rust API

`tar-install` is the reusable Rust core behind the `tarminal` CLI. Use it when
you want to inspect, install, list, diagnose, or remove tarball-based Linux apps
from another Rust program without spawning the CLI.

Crate page:

```text
https://crates.io/crates/tar-install
```

## Add the crate

```toml
[dependencies]
tar-install = "0.2"
```

The Rust crate name uses an underscore in code:

```rust
use tar_install::{install_archive, InstallInput, InstallScope};
```

## Core concepts

- `InstallScope::User` installs into XDG-style user paths under `~/.local`.
- `InstallScope::System` installs into system paths such as `/opt`,
  `/usr/local/bin`, and `/usr/share/applications`.
- `InstallInput` lets callers override detected metadata such as app id,
  display name, executable path, command name, icon path, version, and recipe.
- `ArchiveInspection` describes detected executables, icons, embedded manifests,
  safety checks, filename guesses, and notes.
- `InstallReport` contains both the resolved install plan and the saved
  installed-app state.

## Inspect an archive

```rust
use std::path::Path;
use tar_install::archive::inspect_archive;

fn main() -> anyhow::Result<()> {
    let inspection = inspect_archive(Path::new("SomeApp-1.2.0-linux-x64.tar.xz"))?;

    if !inspection.safe {
        for entry in inspection.unsafe_entries {
            eprintln!("unsafe entry: {} {:?}", entry.path.display(), entry.unsafe_reason);
        }
        return Ok(());
    }

    for candidate in inspection.executable_candidates {
        println!("{} ({})", candidate.path.display(), candidate.reason);
    }

    Ok(())
}
```

Supported archive formats are `.tar`, `.tar.gz`, `.tgz`, `.tar.xz`, `.txz`,
`.tar.bz2`, and `.tbz2`.

## Install an app

```rust
use std::path::Path;
use tar_install::{install_archive, InstallInput, InstallScope};

fn main() -> anyhow::Result<()> {
    let input = InstallInput {
        id: Some("some-app".to_string()),
        name: Some("Some App".to_string()),
        command: Some("some-app".to_string()),
        force: true,
        ..InstallInput::default()
    };

    let report = install_archive(
        Path::new("SomeApp-1.2.0-linux-x64.tar.xz"),
        InstallScope::User,
        input,
    )?;

    println!("installed {}", report.installed.id);
    println!("command: {}", report.installed.command_path.display());

    Ok(())
}
```

Installation performs the same safety checks as the CLI. Archives with absolute
paths, `..` path traversal, unsafe symlinks, or hard-link entries are rejected.
Platform mismatches detected from the filename are rejected unless `force` is
set.

## Progress reporting

Use `install_archive_with_progress` when your UI wants progress events:

```rust
use std::path::Path;
use tar_install::{install_archive_with_progress, InstallInput, InstallProgress, InstallScope};

fn main() -> anyhow::Result<()> {
    let input = InstallInput::default();

    let report = install_archive_with_progress(
        Path::new("SomeApp-1.2.0-linux-x64.tar.xz"),
        InstallScope::User,
        input,
        Some(&|event| match event {
            InstallProgress::Planning => eprintln!("planning"),
            InstallProgress::Extracting { current, total, path } => {
                eprintln!("extracting {current}/{total}: {}", path.display());
            }
            InstallProgress::Copying { current, total, path } => {
                eprintln!("copying {current}/{total}: {}", path.display());
            }
            InstallProgress::Integrating { step } => eprintln!("{step}"),
            InstallProgress::Finished => eprintln!("finished"),
        }),
    )?;

    println!("installed {}", report.installed.id);
    Ok(())
}
```

## List, diagnose, and remove

```rust
use tar_install::install::{doctor_app, list_apps};
use tar_install::{remove_app, InstallScope};

fn main() -> anyhow::Result<()> {
    for app in list_apps(InstallScope::User)? {
        println!("{} -> {}", app.id, app.command_path.display());
    }

    for line in doctor_app(InstallScope::User, "some-app")? {
        println!("{line}");
    }

    let removed = remove_app(InstallScope::User, "some-app")?;
    println!("removed {} paths", removed.removed_paths.len());

    Ok(())
}
```

## Recipes

If automatic detection is not enough, pass an `AppRecipe` through
`InstallInput::recipe`:

```rust
use std::path::Path;
use tar_install::recipe::load_recipe;
use tar_install::{install_archive, InstallInput, InstallScope};

fn main() -> anyhow::Result<()> {
    let recipe = load_recipe(Path::new("some-app.tarapp.yml"))?;
    let input = InstallInput {
        recipe: Some(recipe),
        ..InstallInput::default()
    };

    install_archive(
        Path::new("SomeApp-1.2.0-linux-x64.tar.xz"),
        InstallScope::User,
        input,
    )?;

    Ok(())
}
```

The archive may also include an embedded `tarapp.yml`, `.tarapp.yml`,
`manifest.yml`, or `manifest.yaml`. Embedded recipes are used automatically when
the caller does not provide an explicit recipe.

## Shared library and Python bindings

The same Rust crate can also build `libtar_install.so`:

```bash
cargo build -p tar-install
cargo build -p tar-install --release
```

The Python package loads this shared library with `ctypes` and calls the Rust
API through the FFI functions in `tar_install::ffi`. Rust applications should
prefer the native Rust API shown above.
