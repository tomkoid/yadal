# Yadal

Yadal (Yet Another Downloader for TIDAL) is a pretty simple command-line tool for downloading music from TIDAL. It supports downloading individual tracks, albums, and playlists with configurable audio quality settings.

[![asciicast](https://asciinema.org/a/1262584.svg)](https://asciinema.org/a/1262584)

## Purpose

This project serves as a practical showcase of [Tidlers](https://codeberg.org/tomkoid/tidlers), a Rust library for interacting with the TIDAL API. Yadal demonstrates how to build a complete application using Tidlers for authentication, API interaction, and media streaming.

## Why another TIDAL downloader?

- Download tracks, albums, and playlists from TIDAL in 24-bit, 192kHz
- Download multiple albums at once
- Support for multiple audio quality levels: low, high, lossless, and hi-res
- Download range of tracks from an album or playlist (`--range`)
- Parallel downloads
- Works on Windows, macOS, Linux, FreeBSD, and more!
- Progress indicators for downloads
- Tags downloaded audio with files with all possible tags from TIDAL
- Internal Rust library can be used anywhere

## Installation (Linux)

### Arch Linux/Arch based distributions

```bash
yay -S yadal-git
```

### Fedora

```bash
sudo dnf copr enable tomkoid/yadal
sudo dnf install yadal
```

### NixOS

You can install Yadal using the Nix package manager to your profile like this:

```bash
nix profile install github:tomkoid/yadal
```

Or you can install it system-wide like any other flake using the `flake.nix`.

### From source (Linux, macOS, Windows)

To build and install Yadal, you need to have Rust installed. You can install Rust using [rustup](https://rustup.rs/).
Rust 1.89+ is required.

```bash
cargo install --git https://codeberg.org/tomkoid/yadal --locked
```

The binary will be available in your `~/.cargo/bin`.

## Usage

> An active TIDAL subscription is required to use yadal.

### Basic Usage

Download a track:
```bash
yadal download https://tidal.com/track/437468401
```

Download an album:
```bash
yadal download https://tidal.com/album/55130630
```

Download a playlist:
```bash
yadal download https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d
```

Download a playlist and an album after each other:
```bash
yadal download https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d,https://tidal.com/album/55130630
```

### Using Raw IDs

You can also provide just the ID without the full URL:
```bash
yadal download 437468401
yadal download 55130630
yadal download aa692128-2954-4fe1-b5a1-4ede1add485d
```
As well as in one command:
```bash
yadal download 437468401 55130630 aa692128-2954-4fe1-b5a1-4ede1add485d
```

The tool will automatically detect the media type based on the ID format.

### Options

Specify audio quality:
```bash
yadal download -q hi-res https://tidal.com/track/230917825
```

Available quality options: `low`, `high`, `lossless`, `hires` (default: `hires`)

Set output directory:
```bash
yadal download --output ./music https://tidal.com/album/55130630
```

Configure parallel downloads:
```bash
yadal download --parallel 10 https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d
```

Force re-authentication:
```bash
yadal download --reauth https://tidal.com/track/437468401
```

Use custom session file location:
```bash
yadal download --session-file /path/to/session.json https://tidal.com/track/341764697
```

Use legacy OAuth2 device flow:
```bash
yadal download --oauth2 https://tidal.com/track/341764697
```

Download album items even if they already exist:
```bash
yadal download --force https://tidal.com/album/55130630
```

## Configuration

See [`CONFIG.md`](docs/CONFIG.md) for a guide on configuring yadal more extensively.

## Authentication tutorial

On first run, Yadal will initiate a PKCE flow by default:

1. A login URL will be displayed in your terminal
2. Visit the URL and authorize the application in your browser
3. After redirect to an error page, copy and paste the full redirect URL back into the terminal
4. The session will be saved automatically

If you pass `--oauth2`, Yadal will use the legacy OAuth2 device flow instead. This flow is less stable and should be used only if you do not want to download in full quality.

Session files are stored in platform-specific locations:
- Linux: `~/.local/share/yadal/session.json`
- macOS: `~/Library/Application Support/yadal/session.json`
- Windows: `%APPDATA%\yadal\session.json`

Sessions are automatically refreshed when needed, so you only need to authenticate once.
