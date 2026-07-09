# Python Bindings

`tar-install` on PyPI provides Linux-only Python bindings for the Rust
`tar-install` core. It is intended for Python tools that want to reuse
Tarminal's tarball installer behavior without reimplementing the installer in
Python.

Package page:

```text
https://pypi.org/project/tar-install/
```

## Install

```bash
python -m pip install tar-install
```

Import name:

```py
import tarinstall
```

The package loads `libtar_install.so` with `ctypes`. It does not call the
`tarminal` CLI.

## Quick start

```py
import tarinstall

inspection = tarinstall.inspect("SomeApp-1.2.0-linux-x64.tar.xz")

if not inspection["safe"]:
    raise RuntimeError(inspection["unsafe_entries"])

report = tarinstall.install(
    "SomeApp-1.2.0-linux-x64.tar.xz",
    app_id="some-app",
    name="Some App",
    command="some-app",
    force=True,
)

print(report.id)
print(report.command_path)
```

## Public API

The module exposes convenience functions backed by a default client:

```py
tarinstall.inspect(path)
tarinstall.install(path, options=None, **kwargs)
tarinstall.list_apps(scope=None)
tarinstall.doctor(app_id, scope=None)
tarinstall.remove(app_id, scope=None)
tarinstall.build_library(release=False)
```

For apps that need their own shared-library path or source checkout, create a
client:

```py
from tarinstall import TarInstall

client = TarInstall(library="/opt/my-app/libtar_install.so")
inspection = client.inspect("SomeApp-1.2.0-linux-x64.tar.xz")
```

## Install options

Use keyword arguments for simple calls:

```py
report = tarinstall.install(
    "SomeApp-1.2.0-linux-x64.tar.xz",
    scope="user",
    app_id="some-app",
    name="Some App",
    version="1.2.0",
    exec_path="SomeApp/some-app",
    command="some-app",
    icon="SomeApp/icon.png",
    force=True,
)
```

Use `InstallOptions` when your app needs to pass installation configuration
around as a value:

```py
from tarinstall import InstallOptions, TarInstall

options = InstallOptions(
    scope="user",
    app_id="some-app",
    name="Some App",
    command="some-app",
    force=True,
)

client = TarInstall()
report = client.install("SomeApp-1.2.0-linux-x64.tar.xz", options)
```

`scope` may be `"user"` or `"system"`. System installs write to system paths and
normally require elevated permissions.

## Listing, doctor, and remove

```py
import tarinstall

for app in tarinstall.list_apps(scope="user"):
    print(app.id, app.version, app.command)

doctor = tarinstall.doctor("some-app", scope="user")
print(doctor.fields)

removed = tarinstall.remove("some-app", scope="user")
print(removed.removed_paths)
```

## Native library resolution

The wrapper searches for `libtar_install.so` in this order:

1. `TAR_INSTALL_LIB`
2. bundled `tarinstall/lib/libtar_install.so`
3. `target/release/libtar_install.so`
4. `target/debug/libtar_install.so`

If you are working from a source checkout, build the shared library first:

```bash
cargo build -p tar-install
```

Or ask Python to build it:

```py
import tarinstall

tarinstall.build_library(release=True)
```

## Errors

If the native library is missing, cannot be loaded, or returns an installer
error, the wrapper raises `TarInstallError`. The exception may include the raw
native JSON payload:

```py
from tarinstall import TarInstallError

try:
    tarinstall.inspect("broken.tar.xz")
except TarInstallError as err:
    print(err)
    print(err.payload)
```

`UnsupportedPlatformError` is raised when the bindings are used outside Linux.

## Packaging notes

Linux wheels bundle `libtar_install.so`. Development installs may build the
shared library from the surrounding source checkout. Set
`TAR_INSTALL_SKIP_BUNDLE=1` when building a wrapper wheel without bundling the
shared library.
