use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Top-level config ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
    pub keys: Keys,
    pub layout: Layout,
    pub now_playing: NowPlayingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            keys: Keys::default(),
            layout: Layout::default(),
            now_playing: NowPlayingConfig::default(),
        }
    }
}

// ─── Theme ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    /// Accent color as [r, g, b]
    pub accent: [u8; 3],
    /// Background color as [r, g, b]
    pub background: [u8; 3],
    /// Muted/secondary text color as [r, g, b]
    pub muted: [u8; 3],
    /// Selected item highlight background as [r, g, b]
    pub highlight_bg: [u8; 3],
    /// Border style: "plain" | "rounded" | "double" | "thick"
    pub border_style: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: [30, 215, 96],       // Spotify green
            background: [18, 18, 18],    // Near black
            muted: [120, 120, 120],      // Grey
            highlight_bg: [40, 40, 40],  // Slightly lighter bg
            border_style: "plain".to_string(),
        }
    }
}

// ─── Keybindings ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Keys {
    pub quit: String,
    pub search: String,
    pub play_pause: String,
    pub next: String,
    pub previous: String,
    pub shuffle: String,
    pub repeat: String,
    pub panel_next: String,
    pub scroll_down: String,
    pub scroll_up: String,
    pub select: String,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            quit: "q".to_string(),
            search: "/".to_string(),
            play_pause: " ".to_string(),
            next: "n".to_string(),
            previous: "p".to_string(),
            shuffle: "s".to_string(),
            repeat: "r".to_string(),
            panel_next: "Tab".to_string(),
            scroll_down: "j".to_string(),
            scroll_up: "k".to_string(),
            select: "Enter".to_string(),
        }
    }
}

// ─── Layout ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// Width % of the playlist sidebar (rest goes to search/content)
    pub sidebar_width_pct: u16,
    /// Height of the now-playing bar in terminal rows
    pub now_playing_height: u16,
    /// Show the keybind hints bar at the bottom
    pub show_hints: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            sidebar_width_pct: 30,
            now_playing_height: 5,
            show_hints: true,
        }
    }
}

// ─── Now Playing display options ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NowPlayingConfig {
    pub show_track_name: bool,
    pub show_artist: bool,
    pub show_album: bool,
    pub show_progress_bar: bool,
    pub show_time: bool,
    pub show_shuffle_repeat: bool,
    pub show_device: bool,
}

impl Default for NowPlayingConfig {
    fn default() -> Self {
        Self {
            show_track_name: true,
            show_artist: true,
            show_album: true,
            show_progress_bar: true,
            show_time: true,
            show_shuffle_repeat: true,
            show_device: false,
        }
    }
}

// ─── Load / save ─────────────────────────────────────────────────────────────

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("spotify-tui")
        .join("config.toml")
}

pub fn load() -> Config {
    let path = config_path();
    if !path.exists() {
        // Write defaults so the user has a file to edit
        let _ = save_defaults(&path);
        return Config::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("Warning: config parse error ({}), using defaults", e);
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

fn save_defaults(path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = DEFAULT_CONFIG;
    std::fs::write(path, content)?;
    Ok(())
}

// ─── Default config file content (written on first run) ──────────────────────

const DEFAULT_CONFIG: &str = r#"# spotify-tui configuration
# Location: ~/.config/spotify-tui/config.toml

[theme]
# Accent color used for borders, highlights, progress bar [r, g, b]
accent = [255, 100, 100]       # Spotify green — try [29, 185, 84] for classic green
                              # or [255, 100, 100] for red, [100, 149, 237] for blue

# Terminal background color [r, g, b]
background = [0, 0, 0]

# Muted/secondary text color [r, g, b]
muted = [120, 120, 120]

# Selected item highlight background [r, g, b]
highlight_bg = [40, 40, 40]

# Border style: "plain" | "rounded" | "double" | "thick"
border_style = "plain"

[keys]
# Single characters or special keys: "Tab", "Enter", "Esc", " " (space)
quit        = "q"
search      = "/"
play_pause  = " "
next        = "n"
previous    = "p"
shuffle     = "s"
repeat      = "r"
panel_next  = "Tab"
scroll_down = "j"
scroll_up   = "k"
select      = "Enter"

[layout]
# Width of the playlist sidebar as a percentage (0-100)
sidebar_width_pct = 30

# Height of the now-playing bar in terminal rows (minimum 4)
now_playing_height = 5

# Show the keybind hints bar at the bottom
show_hints = true

[now_playing]
# Toggle each element of the now-playing bar
show_track_name    = true
show_artist        = true
show_album         = true
show_progress_bar  = true
show_time          = true
show_shuffle_repeat = true
show_device        = false   # shows which Spotify device is active
"#;