mod app;
mod auth;
mod config;
mod spotify;
mod ui;

use anyhow::Result;
use app::{ActivePanel, App, InputMode, RepeatMode};
use config::Config;
use auth::Auth;
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

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cfg = config::load();

    let client_id = std::env::var("SPOTIFY_CLIENT_ID")
        .expect("SPOTIFY_CLIENT_ID not set. Copy .env.example to .env and fill it in.");
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI")
        .unwrap_or_else(|_| "http://127.0.0.1:8888/callback".to_string());

    let auth = Auth::new(client_id, redirect_uri);
    let tokens = auth.load_or_authenticate().await?;

    let spotify = Arc::new(SpotifyClient::new(tokens.access_token.clone()));
    let app = Arc::new(Mutex::new(App::new()));

    // Initial data fetch
    {
        let mut a = app.lock().await;
        match spotify.current_playback().await {
            Ok(Some(pb)) => {
                a.sync_playback_state(&pb);
                a.playback = Some(pb);
            }
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

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: Arc<Mutex<App>>,
    spotify: Arc<SpotifyClient>,
    cfg: Config,
) -> Result<()> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || loop {
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if event_tx.send(AppEvent::Key(key)).is_err() {
                    break;
                }
            }
        } else if event_tx.send(AppEvent::Tick).is_err() {
            break;
        }
    });

    let mut poll_ticker = tokio::time::interval(POLL_INTERVAL);
    poll_ticker.tick().await;

    loop {
        {
            let a = app.lock().await;
            terminal.draw(|f| ui::draw(f, &a, &cfg))?;
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

                handle_key(key.code, app.clone(), spotify.clone(), &cfg.keys).await;

                if app.lock().await.should_quit {
                    return Ok(());
                }
            }
        }
    }
}

async fn handle_key(code: KeyCode, app: Arc<Mutex<App>>, spotify: Arc<SpotifyClient>, keys: &config::Keys) {
    let mode = app.lock().await.input_mode.clone();

    match mode {
        InputMode::Typing => match code {
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
            KeyCode::Char(c) => app.lock().await.search_query.push(c),
            KeyCode::Backspace => { app.lock().await.search_query.pop(); }
            _ => {}
        },

        InputMode::Normal => match code {
            KeyCode::Char('q') => app.lock().await.should_quit = true,
            KeyCode::Char('/') => {
                let mut a = app.lock().await;
                a.active_panel = ActivePanel::Search;
                a.input_mode = InputMode::Typing;
                a.search_query.clear();
            }
            KeyCode::Tab => {
                let mut a = app.lock().await;
                a.active_panel = match a.active_panel {
                    ActivePanel::NowPlaying => ActivePanel::Search,
                    ActivePanel::Search => ActivePanel::Playlists,
                    ActivePanel::Playlists => ActivePanel::NowPlaying,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => app.lock().await.scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => app.lock().await.scroll_up(),

            // Play / Pause
            KeyCode::Char(' ') => {
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
            }

            // Next / Prev
            KeyCode::Char('n') => {
                let device_id = spotify.get_active_device_id().await;
                let result = spotify.next_track(device_id.as_deref()).await;
                let mut a = app.lock().await;
                match result {
                    Ok(_) => a.set_status("⏭ Next track"),
                    Err(e) => a.set_status(format!("Error: {e}")),
                }
            }
            KeyCode::Char('p') => {
                let device_id = spotify.get_active_device_id().await;
                let result = spotify.previous_track(device_id.as_deref()).await;
                let mut a = app.lock().await;
                match result {
                    Ok(_) => a.set_status("⏮ Previous track"),
                    Err(e) => a.set_status(format!("Error: {e}")),
                }
            }

            // Toggle shuffle
            KeyCode::Char('s') => {
                let new_shuffle = !app.lock().await.shuffle;
                let device_id = spotify.get_active_device_id().await;
                let result = spotify.set_shuffle(new_shuffle, device_id.as_deref()).await;
                let mut a = app.lock().await;
                match result {
                    Ok(_) => {
                        a.shuffle = new_shuffle;
                        a.set_status(if new_shuffle { "🔀 Shuffle ON" } else { "🔀 Shuffle OFF" });
                    }
                    Err(e) => a.set_status(format!("Error: {e}")),
                }
            }

            // Cycle repeat: off → context → track → off
            KeyCode::Char('r') => {
                let new_repeat = app.lock().await.repeat.next();
                let device_id = spotify.get_active_device_id().await;
                let result = spotify.set_repeat(new_repeat.as_str(), device_id.as_deref()).await;
                let mut a = app.lock().await;
                match result {
                    Ok(_) => {
                        let label = match &new_repeat {
                            RepeatMode::Off => "🔁 Repeat OFF",
                            RepeatMode::Context => "🔁 Repeat: Playlist",
                            RepeatMode::Track => "🔂 Repeat: Track",
                        };
                        a.repeat = new_repeat;
                        a.set_status(label);
                    }
                    Err(e) => a.set_status(format!("Error: {e}")),
                }
            }

            // Play selected item
            KeyCode::Enter => {
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

                            // Always turn off shuffle and repeat when starting a playlist
                            let _ = spotify.set_shuffle(false, did).await;
                            let _ = spotify.set_repeat("off", did).await;
                            {
                                let mut a = app.lock().await;
                                a.shuffle = false;
                                a.repeat = RepeatMode::Off;
                            }

                            // let result = match kind {
                            //     PlaylistKind::LikedSongs => spotify.play_liked_songs(did).await,
                            //     PlaylistKind::Dj => {
                            //         // DJ uses a special radio context — try the URI if it's real,
                            //         // otherwise nudge user to start it from the Spotify app first
                            //         if uri == "dj" {
                            //             Err(anyhow::anyhow!("Start the DJ from Spotify first, then control it here"))
                            //         } else {
                            //             spotify.play_context(&uri, did).await
                            //         }
                            //     }
                            //     PlaylistKind::Normal => spotify.play_context(&uri, did).await,
                            // };

                            // let mut a = app.lock().await;
                            // match result {
                            //     Ok(_) => a.set_status(format!("▶ Playing: {name}")),
                            //     Err(e) => a.set_status(format!("{e}")),
                            // }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        },
    }
}