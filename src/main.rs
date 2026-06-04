mod app;
mod auth;
mod config;
mod spotify;
mod ui;

use anyhow::Result;
use app::{ActivePanel, App, InputMode, RepeatMode, SettingsSection};
use auth::Auth;
use config::Config;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use spotify::{PlaylistKind, SpotifyClient};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex};

const POLL_INTERVAL: Duration = Duration::from_secs(3);

enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Tick,
}

fn parse_key(s: &str) -> KeyCode {
    match s {
        "Tab"   => KeyCode::Tab,
        "Enter" => KeyCode::Enter,
        "Esc"   => KeyCode::Esc,
        "Up"    => KeyCode::Up,
        "Down"  => KeyCode::Down,
        "Left"  => KeyCode::Left,
        "Right" => KeyCode::Right,
        s if s.len() == 1 => KeyCode::Char(s.chars().next().unwrap()),
        _ => KeyCode::Null,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cfg = config::load();

    let client_id = std::env::var("SPOTIFY_CLIENT_ID")
        .expect("SPOTIFY_CLIENT_ID not set.");
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI")
        .unwrap_or_else(|_| "http://127.0.0.1:8888/callback".to_string());

    let auth = Auth::new(client_id, redirect_uri);
    let tokens = auth.load_or_authenticate().await?;

    let spotify = Arc::new(SpotifyClient::new(tokens.access_token.clone()));
    let app = Arc::new(Mutex::new(App::new()));

    {
        let mut a = app.lock().await;
        match spotify.current_playback().await {
            Ok(Some(pb)) => { a.sync_playback_state(&pb); a.playback = Some(pb); }
            Ok(None) => {}
            Err(e) => a.set_status(format!("Playback error: {e}")),
        }
        match spotify.get_all_playlist_entries().await {
            Ok(pl) => a.playlists = pl,
            Err(e) => a.set_status(format!("Playlists error: {e}")),
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, app.clone(), spotify.clone(), cfg).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result { eprintln!("Error: {e}"); }
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: Arc<Mutex<App>>,
    spotify: Arc<SpotifyClient>,
    cfg: Config,
) -> Result<()> {
    let cfg = Arc::new(Mutex::new(cfg));

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || loop {
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if event_tx.send(AppEvent::Key(key)).is_err() { break; }
            }
        } else if event_tx.send(AppEvent::Tick).is_err() {
            break;
        }
    });

    let mut poll_ticker = tokio::time::interval(POLL_INTERVAL);
    poll_ticker.tick().await;

    loop {
        // Build a snapshot for rendering
        {
            let a = app.lock().await;
            let c = cfg.lock().await.clone();
            let snap = build_snapshot(&a);
            terminal.draw(move |f| ui::draw(f, &snap, &c))?;
        }

        tokio::select! {
            _ = poll_ticker.tick() => {
                let sp = spotify.clone();
                let app_clone = app.clone();
                tokio::spawn(async move {
                    if let Ok(Some(pb)) = sp.current_playback().await {
                        let mut a = app_clone.lock().await;
                        a.sync_playback_state(&pb);
                        a.playback = Some(pb);
                    }
                });
            }
            Some(ev) = event_rx.recv() => {
                let AppEvent::Key(key) = ev else { continue };
                if key.kind != KeyEventKind::Press { continue; }
                handle_key(key.code, app.clone(), spotify.clone(), cfg.clone()).await;
                if app.lock().await.should_quit { return Ok(()); }
            }
        }
    }
}

fn build_snapshot(a: &App) -> App {
    let mut snap = App::new();
    snap.shuffle = a.shuffle;
    snap.repeat = a.repeat.clone();
    snap.playback = a.playback.clone();
    snap.search_query = a.search_query.clone();
    snap.search_results = a.search_results.clone();
    snap.search_selected = a.search_selected;
    snap.playlists = a.playlists.clone();
    snap.playlist_selected = a.playlist_selected;
    snap.active_panel = a.active_panel.clone();
    snap.input_mode = a.input_mode.clone();
    snap.status_message = a.status_message.clone();
    snap.should_quit = a.should_quit;
    snap.settings.section_selected = a.settings.section_selected;
    snap.settings.item_selected = a.settings.item_selected;
    snap.settings.edit_buffer = a.settings.edit_buffer.clone();
    snap.settings.editing = a.settings.editing;
    snap
}

async fn handle_key(
    code: KeyCode,
    app: Arc<Mutex<App>>,
    spotify: Arc<SpotifyClient>,
    cfg: Arc<Mutex<Config>>,
) {
    let mode = app.lock().await.input_mode.clone();
    let panel = app.lock().await.active_panel.clone();

    // ── Settings editing mode ─────────────────────────────────────────────────
    if mode == InputMode::EditingValue {
        match code {
            KeyCode::Esc => {
                let mut a = app.lock().await;
                a.settings.editing = false;
                a.settings.edit_buffer.clear();
                a.input_mode = InputMode::Normal;
            }
            KeyCode::Enter => {
                let (section, item, value) = {
                    let a = app.lock().await;
                    (
                        a.settings.current_section().clone(),
                        a.settings.item_selected,
                        a.settings.edit_buffer.clone(),
                    )
                };
                let ok = apply_setting(&section, item, &value, cfg.clone()).await;
                let mut a = app.lock().await;
                a.settings.editing = false;
                a.settings.edit_buffer.clear();
                a.input_mode = InputMode::Normal;
                if ok {
                    a.set_status("✅ Setting saved");
                } else {
                    a.set_status("❌ Invalid value — for colors use: r,g,b (e.g. 30,215,96)");
                }
            }
            KeyCode::Backspace => { app.lock().await.settings.edit_buffer.pop(); }
            KeyCode::Char(c)   => { app.lock().await.settings.edit_buffer.push(c); }
            _ => {}
        }
        return;
    }

    // ── Search typing mode ────────────────────────────────────────────────────
    if mode == InputMode::Typing {
        match code {
            KeyCode::Esc => {
                let mut a = app.lock().await;
                a.input_mode = InputMode::Normal;
                a.set_status("Search cancelled");
            }
            KeyCode::Enter => {
                let query = {
                    let mut a = app.lock().await;
                    a.input_mode = InputMode::Normal;
                    a.search_selected = 0;
                    a.search_query.clone()
                };
                match spotify.search_tracks(&query).await {
                    Ok(tracks) => {
                        let mut a = app.lock().await;
                        a.set_status(format!("Found {} tracks", tracks.len()));
                        a.search_results = tracks;
                    }
                    Err(e) => app.lock().await.set_status(format!("Search failed: {e}")),
                }
            }
            KeyCode::Char(c)   => { app.lock().await.search_query.push(c); }
            KeyCode::Backspace => { app.lock().await.search_query.pop(); }
            _ => {}
        }
        return;
    }

    // ── Settings panel normal mode ────────────────────────────────────────────
    if panel == ActivePanel::Settings {
        match code {
            KeyCode::Esc | KeyCode::Char('c') => {
                app.lock().await.active_panel = ActivePanel::NowPlaying;
            }
            KeyCode::Left => {
                let mut a = app.lock().await;
                let sections = SettingsSection::all();
                if a.settings.section_selected > 0 {
                    a.settings.section_selected -= 1;
                } else {
                    a.settings.section_selected = sections.len() - 1;
                }
                a.settings.item_selected = 0;
            }
            KeyCode::Right => {
                let mut a = app.lock().await;
                let sections = SettingsSection::all();
                a.settings.section_selected = (a.settings.section_selected + 1) % sections.len();
                a.settings.item_selected = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => app.lock().await.scroll_down(),
            KeyCode::Up   | KeyCode::Char('k') => app.lock().await.scroll_up(),
            KeyCode::Enter => {
                // Toggle booleans immediately; open edit buffer for text/numbers
                let (section, item) = {
                    let a = app.lock().await;
                    (a.settings.current_section().clone(), a.settings.item_selected)
                };
                if is_toggle(&section, item) {
                    toggle_setting(&section, item, cfg.clone()).await;
                    app.lock().await.set_status("✅ Saved");
                } else {
                    // Pre-fill buffer with current value — drop cfg guard before locking app
                    let current = {
                        let c = cfg.lock().await;
                        get_setting_value(&section, item, &c)
                    };
                    let mut a = app.lock().await;
                    a.settings.edit_buffer = current;
                    a.settings.editing = true;
                    a.input_mode = InputMode::EditingValue;
                }
            }
            _ => {}
        }
        return;
    }

    // ── Normal mode (main app) ────────────────────────────────────────────────
    let keys = cfg.lock().await.keys.clone();
    let k_quit       = parse_key(&keys.quit);
    let k_search     = parse_key(&keys.search);
    let k_play_pause = parse_key(&keys.play_pause);
    let k_next       = parse_key(&keys.next);
    let k_prev       = parse_key(&keys.previous);
    let k_shuffle    = parse_key(&keys.shuffle);
    let k_repeat     = parse_key(&keys.repeat);
    let k_down       = parse_key(&keys.scroll_down);
    let k_up         = parse_key(&keys.scroll_up);

    if code == k_quit {
        app.lock().await.should_quit = true;

    } else if code == KeyCode::Char('c') {
        app.lock().await.active_panel = ActivePanel::Settings;

    } else if code == k_search {
        let mut a = app.lock().await;
        a.active_panel = ActivePanel::Search;
        a.input_mode = InputMode::Typing;
        a.search_query.clear();

    } else if code == KeyCode::Tab {
        let mut a = app.lock().await;
        a.active_panel = match a.active_panel {
            ActivePanel::NowPlaying => ActivePanel::Search,
            ActivePanel::Search     => ActivePanel::Playlists,
            ActivePanel::Playlists  => ActivePanel::NowPlaying,
            ActivePanel::Settings   => ActivePanel::NowPlaying,
        };

    } else if code == k_down || code == KeyCode::Down {
        app.lock().await.scroll_down();

    } else if code == k_up || code == KeyCode::Up {
        app.lock().await.scroll_up();

    } else if code == k_play_pause {
        let is_playing = app.lock().await.playback.as_ref().map(|p| p.is_playing).unwrap_or(false);
        let device_id = spotify.get_active_device_id().await;
        let did = device_id.as_deref();
        let result = if is_playing { spotify.pause(did).await } else { spotify.play(did).await };
        let mut a = app.lock().await;
        match result {
            Ok(_) => {
                if let Some(pb) = &mut a.playback { pb.is_playing = !is_playing; }
                a.set_status(if is_playing { "⏸ Paused" } else { "▶ Playing" });
            }
            Err(e) => a.set_status(format!("Error: {e}")),
        }

    } else if code == k_next {
        let device_id = spotify.get_active_device_id().await;
        let result = spotify.next_track(device_id.as_deref()).await;
        let mut a = app.lock().await;
        match result {
            Ok(_) => a.set_status("⏭ Next track"),
            Err(e) => a.set_status(format!("Error: {e}")),
        }

    } else if code == k_prev {
        let device_id = spotify.get_active_device_id().await;
        let result = spotify.previous_track(device_id.as_deref()).await;
        let mut a = app.lock().await;
        match result {
            Ok(_) => a.set_status("⏮ Previous track"),
            Err(e) => a.set_status(format!("Error: {e}")),
        }

    } else if code == k_shuffle {
        let new_shuffle = !app.lock().await.shuffle;
        let device_id = spotify.get_active_device_id().await;
        let result = spotify.set_shuffle(new_shuffle, device_id.as_deref()).await;
        let mut a = app.lock().await;
        match result {
            Ok(_) => { a.shuffle = new_shuffle; a.set_status(if new_shuffle { "🔀 Shuffle ON" } else { "🔀 Shuffle OFF" }); }
            Err(e) => a.set_status(format!("Error: {e}")),
        }

    } else if code == k_repeat {
        let new_repeat = app.lock().await.repeat.next();
        let device_id = spotify.get_active_device_id().await;
        let result = spotify.set_repeat(new_repeat.as_str(), device_id.as_deref()).await;
        let mut a = app.lock().await;
        match result {
            Ok(_) => {
                let label = match &new_repeat {
                    RepeatMode::Off     => "🔁 Repeat OFF",
                    RepeatMode::Context => "🔁 Repeat: Playlist",
                    RepeatMode::Track   => "🔂 Repeat: Track",
                };
                a.repeat = new_repeat;
                a.set_status(label);
            }
            Err(e) => a.set_status(format!("Error: {e}")),
        }

    } else if code == KeyCode::Enter {
        let panel = app.lock().await.active_panel.clone();
        match panel {
            ActivePanel::Search => {
                let track_info = app.lock().await
                    .selected_search_track()
                    .map(|t| (t.uri.clone(), t.name.clone()));
                if let Some((uri, name)) = track_info {
                    let device_id = spotify.get_active_device_id().await;
                    let result = spotify.play_track(&uri, device_id.as_deref()).await;
                    let mut a = app.lock().await;
                    match result {
                        Ok(_) => a.set_status(format!("▶ Playing: {name}")),
                        Err(e) => a.set_status(format!("Error: {e}")),
                    }
                }
            }
            ActivePanel::Playlists => {
                let playlist_info = app.lock().await
                    .selected_playlist()
                    .map(|p| (p.uri.clone(), p.name.clone(), p.kind.clone()));
                if let Some((uri, name, kind)) = playlist_info {
                    let device_id = spotify.get_active_device_id().await;
                    let did = device_id.as_deref();
                    let _ = spotify.set_shuffle(false, did).await;
                    let _ = spotify.set_repeat("off", did).await;
                    { let mut a = app.lock().await; a.shuffle = false; a.repeat = RepeatMode::Off; }
                    let result = match kind {
                        PlaylistKind::LikedSongs => spotify.play_liked_songs(did).await,
                        PlaylistKind::Dj => {
                            if uri == "dj" {
                                Err(anyhow::anyhow!("Start the DJ from Spotify first, then control it here"))
                            } else {
                                spotify.play_context(&uri, did).await
                            }
                        }
                        PlaylistKind::Normal => spotify.play_context(&uri, did).await,
                    };
                    let mut a = app.lock().await;
                    match result {
                        Ok(_) => a.set_status(format!("▶ Playing: {name}")),
                        Err(e) => a.set_status(format!("{e}")),
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Settings helpers ──────────────────────────────────────────────────────────

fn is_toggle(section: &SettingsSection, item: usize) -> bool {
    match section {
        SettingsSection::NowPlaying => true,
        SettingsSection::Layout     => item == 2, // show_hints
        _ => false,
    }
}

async fn toggle_setting(section: &SettingsSection, item: usize, cfg: Arc<Mutex<Config>>) {
    let mut c = cfg.lock().await;
    match section {
        SettingsSection::NowPlaying => {
            let np = &mut c.now_playing;
            match item {
                0 => np.show_track_name    = !np.show_track_name,
                1 => np.show_artist        = !np.show_artist,
                2 => np.show_album         = !np.show_album,
                3 => np.show_progress_bar  = !np.show_progress_bar,
                4 => np.show_time          = !np.show_time,
                5 => np.show_shuffle_repeat = !np.show_shuffle_repeat,
                6 => np.show_device        = !np.show_device,
                _ => {}
            }
        }
        SettingsSection::Layout => {
            if item == 2 { c.layout.show_hints = !c.layout.show_hints; }
        }
        _ => {}
    }
    config::save(&c).ok();
}

async fn apply_setting(section: &SettingsSection, item: usize, value: &str, cfg: Arc<Mutex<Config>>) -> bool {
    let mut c = cfg.lock().await;
    match section {
        SettingsSection::Theme => {
            match item {
                0 => { if let Some(v) = parse_rgb(value) { c.theme.accent = v; } else { return false; } }
                1 => { if let Some(v) = parse_rgb(value) { c.theme.background = v; } else { return false; } }
                2 => { if let Some(v) = parse_rgb(value) { c.theme.muted = v; } else { return false; } }
                3 => { if let Some(v) = parse_rgb(value) { c.theme.highlight_bg = v; } else { return false; } }
                4 => { c.theme.border_style = value.to_string(); }
                _ => {}
            }
        }
        SettingsSection::Layout => {
            match item {
                0 => { if let Ok(v) = value.parse::<u16>() { c.layout.sidebar_width_pct = v.clamp(10, 90); } }
                1 => { if let Ok(v) = value.parse::<u16>() { c.layout.now_playing_height = v.max(4); } }
                _ => {}
            }
        }
        SettingsSection::Keybindings => {
            let k = &mut c.keys;
            match item {
                0  => k.quit        = value.to_string(),
                1  => k.search      = value.to_string(),
                2  => k.play_pause  = value.to_string(),
                3  => k.next        = value.to_string(),
                4  => k.previous    = value.to_string(),
                5  => k.shuffle     = value.to_string(),
                6  => k.repeat      = value.to_string(),
                7  => k.panel_next  = value.to_string(),
                8  => k.scroll_down = value.to_string(),
                9  => k.scroll_up   = value.to_string(),
                10 => k.select      = value.to_string(),
                _  => {}
            }
        }
        _ => { return false; }
    }
    config::save(&c).ok();
    true
}

fn get_setting_value(section: &SettingsSection, item: usize, cfg: &Config) -> String {
    match section {
        SettingsSection::Theme => {
            match item {
                0 => format!("{},{},{}", cfg.theme.accent[0], cfg.theme.accent[1], cfg.theme.accent[2]),
                1 => format!("{},{},{}", cfg.theme.background[0], cfg.theme.background[1], cfg.theme.background[2]),
                2 => format!("{},{},{}", cfg.theme.muted[0], cfg.theme.muted[1], cfg.theme.muted[2]),
                3 => format!("{},{},{}", cfg.theme.highlight_bg[0], cfg.theme.highlight_bg[1], cfg.theme.highlight_bg[2]),
                4 => cfg.theme.border_style.clone(),
                _ => String::new(),
            }
        }
        SettingsSection::Layout => {
            match item {
                0 => cfg.layout.sidebar_width_pct.to_string(),
                1 => cfg.layout.now_playing_height.to_string(),
                _ => String::new(),
            }
        }
        SettingsSection::Keybindings => {
            let k = &cfg.keys;
            match item {
                0  => k.quit.clone(),
                1  => k.search.clone(),
                2  => k.play_pause.clone(),
                3  => k.next.clone(),
                4  => k.previous.clone(),
                5  => k.shuffle.clone(),
                6  => k.repeat.clone(),
                7  => k.panel_next.clone(),
                8  => k.scroll_down.clone(),
                9  => k.scroll_up.clone(),
                10 => k.select.clone(),
                _  => String::new(),
            }
        }
        _ => String::new(),
    }
}

fn parse_rgb(s: &str) -> Option<[u8; 3]> {
    // Accept "r,g,b" or "r, g, b" or "r g b"
    let parts: Vec<&str> = s.split(|c| c == ',' || c == ' ')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 { return None; }
    Some([
        parts[0].trim().parse().ok()?,
        parts[1].trim().parse().ok()?,
        parts[2].trim().parse().ok()?,
    ])
}