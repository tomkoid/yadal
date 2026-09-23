%global repo https://codeberg.org/tomkoid/yadal
%global branch main
%global commit %(git ls-remote %{repo}.git refs/heads/%{branch} | awk '{print $1}')
%global shortcommit %(echo %{commit} | cut -c1-7)
%global snapshot_date %(date -u +%Y%m%d)

Name:           yadal-git
Version:        0.3.0^git%{snapshot_date}.g%{shortcommit}
Release:        1%{?dist}
Summary:        Command-line TIDAL music downloader

License:        GPL-3.0-only
URL:            %{repo}
Source0:        %{repo}/archive/%{commit}.tar.gz

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
%cargo_build

%install
install -Dpm0755 target/rpm/yadal %{buildroot}%{_bindir}/yadal

%files
%license LICENSE.txt
%doc README.md
%{_bindir}/yadal

%changelog
* Wed Sep 23 2026 Tomkoid <tomkoid@tomkoid.cz> - 0.3.0^git%{snapshot_date}.g%{shortcommit}-1
- Initial main branch snapshot package
