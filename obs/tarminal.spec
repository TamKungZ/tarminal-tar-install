#
# spec file for package tarminal
#

Name:           tarminal
Version:        0.2.0
Release:        0
Summary:        CLI frontend for tar-install
License:        MIT
URL:            https://github.com/TamKungZ/tarminal-tar-install
Source0:        %{name}-%{version}.tar.zst
Source1:        vendor.tar.zst
BuildRequires:  cargo >= 1.89
BuildRequires:  rust >= 1.89
BuildRequires:  pkgconfig
BuildRequires:  zstd
%if 0%{?fedora}
BuildRequires:  bzip2-devel
%else
BuildRequires:  libbz2-devel
%endif
BuildRequires:  xz-devel

%description
Tarminal installs Linux application tarballs as desktop applications.
It can create commands, desktop entries, icons, install state, and clean
uninstall tracking for applications distributed as tar archives.

%package -n tar-install
Summary:        Core command stub for Tarminal

%description -n tar-install
tar-install is the core package identity for Tarminal. It ships a small
command stub that points users to the tarminal frontend.

%prep
%autosetup -n %{name}-%{version} -a 1

%build
export CARGO_NET_OFFLINE=true
cargo build --release --locked --offline -p tar-install --bin tar-install
cargo build --release --locked --offline -p tarminal --bin tarminal

%install
install -Dm0755 target/release/tar-install %{buildroot}%{_bindir}/tar-install
install -Dm0755 target/release/tarminal %{buildroot}%{_bindir}/tarminal
install -Dm0644 LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE
install -Dm0644 LICENSE %{buildroot}%{_licensedir}/tar-install/LICENSE
install -Dm0644 README.md %{buildroot}%{_docdir}/%{name}/README.md

%files
%dir %{_licensedir}/%{name}
%dir %{_docdir}/%{name}
%license %{_licensedir}/%{name}/LICENSE
%doc %{_docdir}/%{name}/README.md
%{_bindir}/tarminal

%files -n tar-install
%dir %{_licensedir}/tar-install
%license %{_licensedir}/tar-install/LICENSE
%{_bindir}/tar-install

%changelog
