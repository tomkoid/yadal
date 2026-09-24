%global repo https://codeberg.org/tomkoid/yadal
%global branch main
%global version {{{ git_dir_version }}}

Name:           yadal-git
Version:        %{version}
Release:        1%{?dist}
Summary:        Command-line TIDAL music downloader

License:        GPL-3.0-only
URL:            %{repo}
Source0:        %{repo}/archive/%{branch}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust-packaging
BuildRequires:  rustc
Requires:       ffmpeg-free

%description
Yadal (Yet Another Downloader for TIDAL) is a development snapshot of the
command-line tool for downloading tracks, albums, and playlists from TIDAL.
It supports multiple audio quality levels, parallel downloads, progress
indicators, and tagging downloaded audio with TIDAL metadata.

%prep
%autosetup -n yadal

%build
rm -rf target
unset RUSTC_PROVIDER RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
export CARGO_BUILD_RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER=
export RUSTC=/usr/bin/rustc
%cargo_build

%install
install -Dpm0755 target/rpm/yadal %{buildroot}%{_bindir}/yadal

%files
%license LICENSE.txt
%doc README.md
%{_bindir}/yadal

%changelog
* Wed Sep 23 2026 Tomkoid <tomkoid@tomkoid.cz> - 0.3.0-1.git
- Initial main branch snapshot package
