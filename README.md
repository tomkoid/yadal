# Yadal

Yadal (Yet Another Downloader for TIDAL) is a command-line tool for downloading music from TIDAL. It supports downloading individual tracks, albums, and playlists with configurable audio quality settings.

## Purpose

This project serves as a practical showcase of [Tidlers](https://codeberg.org/tomkoid/tidlers), a Rust library for interacting with the TIDAL API. Yadal demonstrates how to build a complete application using Tidlers for authentication, API interaction, and media streaming.

## Features

- Download tracks, albums, and playlists from TIDAL
- Support for multiple audio quality levels: low, high, lossless, and hi-res
- Parallel downloads with configurable concurrency
- OAuth authentication with automatic token management
- Session persistence across runs
- Platform-specific configuration storage (follows XDG standards on Linux)
- Progress indicators for downloads
- Automatic metadata tagging and file organization

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

## Authentication

On first run, Yadal will initiate an OAuth flow:

1. A URL and code will be displayed in your terminal
2. Visit the URL and enter the code (or use the direct link)
3. Authorize the application in your browser
4. The session will be saved automatically

Session files are stored in platform-specific locations:
- Linux: `~/.local/share/yadal/session.json`
- macOS: `~/Library/Application Support/yadal/session.json`
- Windows: `%APPDATA%\yadal\session.json`

Sessions are automatically refreshed when needed, so you only need to authenticate once.

## About Tidlers

This project is built using [Tidlers](https://codeberg.org/tomkoid/tidlers), a Rust library that provides a clean interface to the TIDAL API. Tidlers handles:

- OAuth authentication flow
- Session management and token refresh
- API endpoint access for tracks, albums, playlists, and more
- Streaming URL generation
- User and subscription management

If you're building your own TIDAL integration in Rust, check out [Tidlers](https://codeberg.org/tomkoid/tidlers).

## Why Another TIDAL Downloader?

While several TIDAL downloaders exist, Yadal offers:

- Efficient parallel downloads
- Clean OAuth authentication flow that persists across sessions
- Support for high-resolution audio formats
- Cross-platform support (Linux, macOS, Windows, probably more)
- Simple URL parsing that accepts both full URLs and raw media IDs

## Requirements

- Rust 1.70 or later
- Active TIDAL subscription (required for high-quality downloads)
