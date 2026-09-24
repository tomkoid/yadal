Name:           yadal
Version:        0.3.0
Release:        1%{?dist}
Summary:        Command-line TIDAL music downloader

License:        GPL-3.0-only
URL:            https://codeberg.org/tomkoid/yadal
Source0:        https://codeberg.org/tomkoid/yadal/archive/main.tar.gz

BuildRequires:  cargo
BuildRequires:  rust-packaging
BuildRequires:  rustc
Requires:       /usr/bin/ffmpeg

%description
Yadal (Yet Another Downloader for TIDAL) is a command-line tool for
downloading tracks, albums, and playlists from TIDAL. It supports multiple
audio quality levels, parallel downloads, progress indicators, and tagging
downloaded audio with TIDAL metadata.

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
* Wed Sep 23 2026 Tomkoid <tomkoid@tomkoid.cz> - 0.3.0-1
- Initial package
