# Tarminal detection heuristics

Default filename pattern:

```text
<app>-<version>-<os>-<architecture>.tar.<compression>
```

Examples:

```text
myapp-1.2.0-linux-x64.tar.xz
myapp_1.2.0_linux_amd64.tar.gz
myapp-linux-x86_64.txz
```

The parser tries to detect:

- app name from tokens before version/os/arch
- version from `v1`, `1.2.0`, `1.2.0-beta`
- OS from tokens such as `linux`, `ubuntu`, `debian`
- arch from `x64`, `x86_64`, `amd64`, `arm64`, `aarch64`

Executable detection order:

1. exact executable name matching guessed app
2. AppImage files
3. `<app>-<arch>` or `<app>_<arch>`
4. executable beginning with app name
5. executable under a `bin/` directory
6. fallback to interactive `--config`

When filename detection fails, the tool intentionally degrades into `--config` mode rather than guessing aggressively.

Safety checks reject archives with:

- absolute paths
- `..` path traversal
- root or platform prefix paths
- symlinks that point outside the archive root
- hard-link entries during installation

Relative symlinks are allowed only when they resolve inside the extracted app
tree.

Installation also rejects archives that appear to target a different OS or CPU
architecture unless the caller passes `--force` or sets `InstallInput.force`.
