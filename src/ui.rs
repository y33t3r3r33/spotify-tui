use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{ActivePanel, App, InputMode, RepeatMode},
    config::{Config, Keys, Theme},
    spotify::format_duration,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn rgb(c: [u8; 3]) -> Color {
    Color::Rgb(c[0], c[1], c[2])
}

fn border_type(style: &str) -> BorderType {
    match style {
        "rounded" => BorderType::Rounded,
        "double"  => BorderType::Double,
        "thick"   => BorderType::Thick,
        _         => BorderType::Plain,
    }
}

fn active_style(theme: &Theme, is_active: bool) -> Style {
    if is_active {
        Style::default().fg(rgb(theme.accent))
    } else {
        Style::default().fg(rgb(theme.muted))
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, app: &App, cfg: &Config) {
    let size = frame.area();
    let np_height = cfg.layout.now_playing_height.max(4);
    let hints_height = if cfg.layout.show_hints { 1 } else { 0 };

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(np_height),
            Constraint::Length(hints_height),
        ])
        .split(size);

    draw_main(frame, app, cfg, root[0]);
    draw_now_playing(frame, app, cfg, root[1]);
    if cfg.layout.show_hints {
        draw_hints(frame, app, cfg, root[2]);
    }
}

// ─── Main area ───────────────────────────────────────────────────────────────

fn draw_main(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let sidebar_pct = cfg.layout.sidebar_width_pct.clamp(10, 80);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(sidebar_pct),
            Constraint::Percentage(100 - sidebar_pct),
        ])
        .split(area);

    draw_playlists(frame, app, cfg, cols[0]);
    draw_search(frame, app, cfg, cols[1]);
}

// ─── Playlists sidebar ───────────────────────────────────────────────────────

fn draw_playlists(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let t = &cfg.theme;
    let is_active = app.active_panel == ActivePanel::Playlists;
    let bt = border_type(&t.border_style);

    let items: Vec<ListItem> = app
        .playlists
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(&p.name, Style::default().fg(Color::White)),
                Span::styled(
                    format!("  ({})", p.total_tracks),
                    Style::default().fg(rgb(t.muted)),
                ),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.playlist_selected));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(bt)
                .border_style(active_style(t, is_active))
                .title(" 📋 Playlists ")
                .title_alignment(Alignment::Left)
                .style(Style::default().bg(rgb(t.background))),
        )
        .highlight_style(
            Style::default()
                .bg(rgb(t.highlight_bg))
                .fg(rgb(t.accent))
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut state);
}

// ─── Search panel ────────────────────────────────────────────────────────────

fn draw_search(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let t = &cfg.theme;
    let is_active = app.active_panel == ActivePanel::Search;
    let bt = border_type(&t.border_style);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Search input
    let input_style = if app.input_mode == InputMode::Typing {
        Style::default().fg(rgb(t.accent))
    } else {
        Style::default().fg(rgb(t.muted))
    };
    let cursor = if app.input_mode == InputMode::Typing { "█" } else { "" };
    let search_key = &cfg.keys.search;
    let input = Paragraph::new(format!(" 🔍 {}{}", app.search_query, cursor))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(bt)
                .border_style(input_style)
                .title(format!(" Search (press {} to type) ", search_key))
                .style(Style::default().bg(rgb(t.background))),
        );
    frame.render_widget(input, inner[0]);

    // Results list
    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|tr| {
            let artist = tr.artists.first().map(|a| a.name.as_str()).unwrap_or("?");
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:30}", truncate(&tr.name, 30)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:25}", truncate(artist, 25)),
                    Style::default().fg(rgb(t.muted)),
                ),
                Span::styled(
                    format_duration(tr.duration_ms),
                    Style::default().fg(rgb(t.muted)),
                ),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(if app.search_results.is_empty() { None } else { Some(app.search_selected) });

    let placeholder = if app.search_results.is_empty() {
        format!("  Press {} to search for tracks...", search_key)
    } else {
        String::new()
    };

    let list = List::new(if items.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(placeholder, Style::default().fg(rgb(t.muted)))))]
    } else {
        items
    })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(bt)
                .border_style(active_style(t, is_active))
                .title(" 🎵 Results (Enter to play) ")
                .style(Style::default().bg(rgb(t.background))),
        )
        .highlight_style(
            Style::default()
                .bg(rgb(t.highlight_bg))
                .fg(rgb(t.accent))
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, inner[1], &mut state);
}

// ─── Now playing bar ─────────────────────────────────────────────────────────

fn draw_now_playing(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let t = &cfg.theme;
    let np = &cfg.now_playing;
    let bt = border_type(&t.border_style);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(bt)
        .border_style(Style::default().fg(rgb(t.accent)))
        .title(" ▶ Now Playing ")
        .style(Style::default().bg(rgb(t.background)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(pb) = &app.playback {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);

        // Left: track info
        if let Some(track) = &pb.item {
            let artist = track.artists.first().map(|a| a.name.as_str()).unwrap_or("?");
            let status_icon = if pb.is_playing { "▶" } else { "⏸" };

            let mut lines = vec![];

            if np.show_track_name {
                lines.push(Line::from(vec![
                    Span::raw(format!("{} ", status_icon)),
                    Span::styled(
                        &track.name,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            if np.show_artist {
                lines.push(Line::from(Span::styled(artist, Style::default().fg(rgb(t.muted)))));
            }
            if np.show_album {
                lines.push(Line::from(Span::styled(&track.album.name, Style::default().fg(rgb(t.muted)))));
            }
            if np.show_device {
                if let Some(device) = &pb.device {
                    lines.push(Line::from(Span::styled(
                        format!("📱 {}", device.name),
                        Style::default().fg(rgb(t.muted)),
                    )));
                }
            }

            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), cols[0]);

            // Right: progress + shuffle/repeat
            let mut right_constraints = vec![];
            if np.show_progress_bar { right_constraints.push(Constraint::Length(1)); }
            if np.show_time        { right_constraints.push(Constraint::Length(1)); }
            if np.show_shuffle_repeat { right_constraints.push(Constraint::Length(1)); }
            if right_constraints.is_empty() { return; }

            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints(right_constraints)
                .split(cols[1]);

            let mut idx = 0;

            if np.show_progress_bar {
                let progress = pb.progress_ms.unwrap_or(0);
                let duration = track.duration_ms.max(1);
                let ratio = (progress as f64 / duration as f64).clamp(0.0, 1.0);
                let gauge = Gauge::default()
                    .gauge_style(Style::default().fg(rgb(t.accent)).bg(Color::Rgb(50, 50, 50)))
                    .ratio(ratio);
                frame.render_widget(gauge, right[idx]);
                idx += 1;
            }

            if np.show_time {
                let progress = pb.progress_ms.unwrap_or(0);
                let duration = track.duration_ms.max(1);
                let time_text = Paragraph::new(Line::from(vec![
                    Span::styled(format_duration(progress), Style::default().fg(rgb(t.muted))),
                    Span::styled(" / ", Style::default().fg(rgb(t.muted))),
                    Span::styled(format_duration(duration), Style::default().fg(rgb(t.muted))),
                ]))
                    .alignment(Alignment::Center);
                frame.render_widget(time_text, right[idx]);
                idx += 1;
            }

            if np.show_shuffle_repeat {
                let shuffle_style = if app.shuffle {
                    Style::default().fg(rgb(t.accent)).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(rgb(t.muted))
                };
                let repeat_style = if app.repeat != RepeatMode::Off {
                    Style::default().fg(rgb(t.accent)).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(rgb(t.muted))
                };
                let indicators = Paragraph::new(Line::from(vec![
                    Span::styled(
                        if app.shuffle { " 🔀 ON " } else { " 🔀 off" },
                        shuffle_style,
                    ),
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        match &app.repeat {
                            RepeatMode::Off     => format!("{} off", app.repeat.icon()),
                            RepeatMode::Context => format!("{} playlist", app.repeat.icon()),
                            RepeatMode::Track   => format!("{} track", app.repeat.icon()),
                        },
                        repeat_style,
                    ),
                ]))
                    .alignment(Alignment::Center);
                frame.render_widget(indicators, right[idx]);
            }
        }
    } else {
        let msg = Paragraph::new(Span::styled(
            "  No active playback. Open Spotify on a device first.",
            Style::default().fg(rgb(t.muted)),
        ));
        frame.render_widget(msg, inner);
    }
}

// ─── Hints bar ───────────────────────────────────────────────────────────────

fn draw_hints(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let t = &cfg.theme;
    let k = &cfg.keys;

    let hints = if app.input_mode == InputMode::Typing {
        format!(" [Enter] Search  [Esc] Cancel")
    } else {
        format!(
            " [{}] Search  [{}] Play/Pause  [{}] Next  [{}] Prev  [{}] Shuffle  [{}] Repeat  [Tab] Panel  [{}] Quit",
            k.search,
            k.play_pause.trim(),
            k.next, k.previous,
            k.shuffle, k.repeat,
            k.quit,
        )
    };

    let status = app.status_message.as_deref().unwrap_or("");

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    frame.render_widget(
        Paragraph::new(Span::styled(hints, Style::default().fg(rgb(t.muted)))),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(status, Style::default().fg(rgb(t.accent))))
            .alignment(Alignment::Right),
        cols[1],
    );
}

// ─── Utility ─────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
    } else {
        format!("{:<width$}", s, width = max)
    }
}