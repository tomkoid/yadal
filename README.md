# Yadal

Yadal (Yet Another Downloader for TIDAL) is a command-line tool for downloading music from TIDAL. It supports downloading individual tracks, albums, and playlists with configurable audio quality settings.

## Purpose

This project serves as a practical showcase of [Tidlers](https://codeberg.org/tomkoid/tidlers), a Rust library for interacting with the TIDAL API. Yadal demonstrates how to build a complete application using Tidlers for authentication, API interaction, and media streaming.

## Features

- Download tracks, albums, and playlists from TIDAL
- Support for multiple audio quality levels: low, high, lossless, and hi-res
- Parallel downloads
- Session persistence across runs
- Works on Linux, macOS, and Windows (likely even more if you would want to)
- Progress indicators for downloads
- Automatic file organization

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/yadal`.

## Usage

### Basic Usage

Download a track:
```bash
yadal https://tidal.com/track/437468401
```

Download an album:
```bash
yadal https://tidal.com/album/55130630
```

Download a playlist:
```bash
yadal https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d
```

### Using Raw IDs

You can also provide just the ID without the full URL:
```bash
yadal 437468401
yadal 55130630
yadal aa692128-2954-4fe1-b5a1-4ede1add485d
```

The tool will automatically detect the media type based on the ID format.

### Options

Specify audio quality:
```bash
yadal --quality hi-res https://tidal.com/track/230917825
```

Available quality options: `low`, `high`, `lossless`, `hires` (default: `hires`)

Set output directory:
```bash
yadal --output ./music https://tidal.com/album/55130630
```

Configure parallel downloads:
```bash
yadal --parallel 10 https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d
```

Force re-authentication:
```bash
yadal --reauth https://tidal.com/track/437468401
```

Use custom session file location:
```bash
yadal --session-file /path/to/session.json https://tidal.com/track/341764697
```

Use legacy OAuth2 device flow:
```bash
yadal --oauth2 https://tidal.com/track/341764697
```

Force album/playlist recheck (old behavior):
```bash
yadal --force-recheck https://tidal.com/album/55130630
```

## Authentication

On first run, Yadal will initiate a PKCE flow by default:

1. A login URL will be displayed in your terminal
2. Visit the URL and authorize the application in your browser
3. After redirect to an error page, copy and paste the full redirect URL back into the terminal
4. The session will be saved automatically

If you pass `--oauth2`, Yadal will use the legacy OAuth2 device flow instead.

Session files are stored in platform-specific locations:
- Linux: `~/.local/share/yadal/session.json`
- macOS: `~/Library/Application Support/yadal/session.json`
- Windows: `%APPDATA%\yadal\session.json`

Sessions are automatically refreshed when needed, so you only need to authenticate once.

## Why Another TIDAL Downloader?

While several TIDAL downloaders exist, Yadal has:

- Efficient parallel downloads
- Support for high-resolution audio formats
- Cross-platform support (Linux, macOS, Windows, probably more)
- Accepts both full URLs and raw media IDs
- Accepts multiple media types (tracks, albums, playlists) in a single command

## Requirements

- Rust 1.70 or later
- Active TIDAL subscription
