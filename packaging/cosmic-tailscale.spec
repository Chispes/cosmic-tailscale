Name:           cosmic-tailscale
Version:        0.1.0
Release:        1%{?dist}
Summary:        Tailscale applet for the COSMIC panel
License:        MIT
URL:            https://github.com/Chispes/cosmic-tailscale
Source0:        %{name}-%{version}.tar.gz
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  just
BuildRequires:  pkgconfig(xkbcommon)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  pkgconfig(fontconfig)
Requires:       tailscale

%description
Manage a host Tailscale daemon from the COSMIC panel, including Taildrop.

%prep
%autosetup

%build
export CARGO_HOME=%{_builddir}/.cargo-home
just build-release

%install
just rootdir=%{buildroot} prefix=%{_prefix} install

%files
%license LICENSE
%{_bindir}/cosmic-tailscale
%{_datadir}/applications/io.github.chispes.CosmicTailscale.desktop
%{_datadir}/metainfo/io.github.chispes.CosmicTailscale.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.chispes.CosmicTailscale.svg

%changelog
* Thu Sep 03 2026 Chispes <chispes@users.noreply.github.com> - 0.1.0-1
- Initial COSMIC panel applet
