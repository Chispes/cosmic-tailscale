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
Requires:       cosmic-tailscale-helper = %{version}-%{release}

%description
Manage a host Tailscale daemon from the COSMIC panel, including Taildrop.

%package helper
Summary:        Host authorization service for COSMIC Tailscale
Requires:       tailscale
Requires:       polkit
Requires:       lxpolkit

%description helper
Host-side PolicyKit service allowing administrator-approved Tailscale operator changes.

%prep
%autosetup

%build
export CARGO_HOME=%{_builddir}/.cargo-home
just build-release

%install
just rootdir=%{buildroot} prefix=%{_prefix} install
install -Dm0755 target/release/cosmic-tailscale-helper %{buildroot}%{_libexecdir}/cosmic-tailscale-helper
install -Dm0644 resources/io.github.chispes.CosmicTailscale.Helper.service %{buildroot}%{_datadir}/dbus-1/system-services/io.github.chispes.CosmicTailscale.Helper.service
install -Dm0644 resources/io.github.chispes.CosmicTailscale.Helper.conf %{buildroot}%{_datadir}/dbus-1/system.d/io.github.chispes.CosmicTailscale.Helper.conf
install -Dm0644 resources/cosmic-tailscale-helper.service %{buildroot}%{_unitdir}/cosmic-tailscale-helper.service
install -Dm0644 resources/io.github.chispes.CosmicTailscale.enable-operator.policy %{buildroot}%{_datadir}/polkit-1/actions/io.github.chispes.CosmicTailscale.enable-operator.policy
install -Dm0644 resources/cosmic-tailscale-polkit-agent.desktop %{buildroot}%{_sysconfdir}/xdg/autostart/cosmic-tailscale-polkit-agent.desktop

%files
%license LICENSE
%{_bindir}/cosmic-tailscale
%{_datadir}/applications/io.github.chispes.CosmicTailscale.desktop
%{_datadir}/metainfo/io.github.chispes.CosmicTailscale.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.chispes.CosmicTailscale.svg

%files helper
%license LICENSE
%{_libexecdir}/cosmic-tailscale-helper
%{_datadir}/dbus-1/system-services/io.github.chispes.CosmicTailscale.Helper.service
%{_datadir}/dbus-1/system.d/io.github.chispes.CosmicTailscale.Helper.conf
%{_unitdir}/cosmic-tailscale-helper.service
%{_datadir}/polkit-1/actions/io.github.chispes.CosmicTailscale.enable-operator.policy
%{_sysconfdir}/xdg/autostart/cosmic-tailscale-polkit-agent.desktop

%changelog
* Mon Sep 28 2026 Chispes <chispes@users.noreply.github.com> - 0.1.0-1
- Add administrator-approved Tailscale operator helper and compact peer rows.
