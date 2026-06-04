# 🎵 spotify-tui

A Spotify terminal application built in Rust. Control your Spotify playback, browse playlists, search tracks, and customize the interface — all from the terminal.

## Features

- **Now Playing** — live progress bar, track info, shuffle/repeat indicators
- **Playback controls** — play, pause, skip, previous
- **Search** — search Spotify's full catalog and play tracks instantly
- **Playlists** — browse and play all your playlists
- **Liked Songs** — play your entire liked songs library
- **AI DJ** — control Spotify's DJ (start it from the Spotify app first)
- **Shuffle & Repeat** — toggle shuffle, cycle repeat modes (off / playlist / track)
- **In-app settings** — live customizable theme, layout, keybindings, and now-playing display

---

## Setup

### 1. Register a Spotify App

1. Go to [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard)
2. Click **Create App**
3. Fill in any name and description
4. Under **Redirect URIs** add: `http://127.0.0.1:8888/callback`
5. Save, then copy your **Client ID** from the app settings page

### 2. Configure credentials

```bash
cp .env.example .env
```

Edit `.env`:

```env
SPOTIFY_CLIENT_ID=your_client_id_here
SPOTIFY_REDIRECT_URI=http://127.0.0.1:8888/callback
```

### 3. Build and run

```bash
cargo run --release
```

On first run your browser will open for Spotify login. Tokens are cached in your system config directory and refreshed automatically on subsequent runs.

> **Note:** Playback controls require a **Spotify Premium** account.

## Troubleshooting

**`redirect_uri` mismatch** — Make sure your Spotify dashboard redirect URI is exactly `http://127.0.0.1:8888/callback` with no trailing slash.

**405 Not Allowed on playback** — Open Spotify on any device first (phone, desktop, web player) to register an active device.

**Liked Songs not playing** — This uses your Spotify user ID internally. Make sure you're authenticated and have liked songs in your library.

**AI DJ not starting** — The DJ can't be started via the API. Start it from the Spotify app first, then use the TUI to control it.

**Settings not applying** — Colors must be entered as `r,g,b` (e.g. `138,43,226`). If you see ❌ in the status bar the format was invalid.
