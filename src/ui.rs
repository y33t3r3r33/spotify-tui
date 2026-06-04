use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{ActivePanel, App, InputMode, RepeatMode},
    config::Config,
    spotify::format_duration,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn rgb(c: [u8; 3]) -> Color {
    Color::Rgb(c[0], c[1], c[2])
}

fn border_type(s: &str) -> BorderType {
    match s {
        "rounded" => BorderType::Rounded,
        "double"  => BorderType::Double,
        "thick"   => BorderType::Thick,
        _         => BorderType::Plain,
    }
}

// ── entry point ──────────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, app: &App, cfg: &Config) {
    // Settings screen takes over the full terminal
    if app.active_panel == ActivePanel::Settings {
        draw_settings(frame, app, cfg);
        return;
    }

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

// ── main content area ────────────────────────────────────────────────────────

fn draw_main(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let sidebar_pct = cfg.layout.sidebar_width_pct.clamp(10u16, 90u16);
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

// ── playlists sidebar ─────────────────────────────────────────────────────────

fn draw_playlists(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let accent  = rgb(cfg.theme.accent);
    let muted   = rgb(cfg.theme.muted);
    let bg      = rgb(cfg.theme.background);
    let hl_bg   = rgb(cfg.theme.highlight_bg);
    let bt      = border_type(&cfg.theme.border_style);
    let is_active = app.active_panel == ActivePanel::Playlists;

    let border_style = if is_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(muted)
    };

    let items: Vec<ListItem> = app
        .playlists
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(&p.name, Style::default().fg(Color::White)),
                Span::styled(
                    format!("  ({})", p.total_tracks),
                    Style::default().fg(muted),
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
                .border_style(border_style)
                .title(" 📋 Playlists ")
                .title_alignment(Alignment::Left)
                .style(Style::default().bg(bg)),
        )
        .highlight_style(
            Style::default()
                .bg(hl_bg)
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut state);
}

// ── search panel ─────────────────────────────────────────────────────────────

fn draw_search(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let accent  = rgb(cfg.theme.accent);
    let muted   = rgb(cfg.theme.muted);
    let bg      = rgb(cfg.theme.background);
    let hl_bg   = rgb(cfg.theme.highlight_bg);
    let bt      = border_type(&cfg.theme.border_style);
    let is_active = app.active_panel == ActivePanel::Search;

    let border_style = if is_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(muted)
    };

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Search input
    let input_style = if app.input_mode == InputMode::Typing {
        Style::default().fg(accent)
    } else {
        Style::default().fg(muted)
    };
    let cursor = if app.input_mode == InputMode::Typing { "█" } else { "" };
    let input = Paragraph::new(format!(" 🔍 {}{}", app.search_query, cursor))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(bt)
                .border_style(input_style)
                .title(format!(" Search (press {} to type) ", cfg.keys.search))
                .style(Style::default().bg(bg)),
        );
    frame.render_widget(input, inner[0]);

    // Results
    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|t| {
            let artist = t.artists.first().map(|a| a.name.as_str()).unwrap_or("?");
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:30}", truncate(&t.name, 30)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:25}", truncate(artist, 25)),
                    Style::default().fg(muted),
                ),
                Span::styled(format_duration(t.duration_ms), Style::default().fg(muted)),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(if app.search_results.is_empty() { None } else { Some(app.search_selected) });

    let placeholder = if app.search_results.is_empty() {
        format!("  Press {} to search for tracks...", cfg.keys.search)
    } else {
        String::new()
    };

    let list = List::new(if items.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(placeholder, Style::default().fg(muted))))]
    } else {
        items
    })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(bt)
                .border_style(border_style)
                .title(" 🎵 Results (Enter to play) ")
                .style(Style::default().bg(bg)),
        )
        .highlight_style(
            Style::default()
                .bg(hl_bg)
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, inner[1], &mut state);
}

// ── now playing bar ───────────────────────────────────────────────────────────

fn draw_now_playing(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let accent = rgb(cfg.theme.accent);
    let muted  = rgb(cfg.theme.muted);
    let bg     = rgb(cfg.theme.background);
    let bt     = border_type(&cfg.theme.border_style);
    let np     = &cfg.now_playing;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(bt)
        .border_style(Style::default().fg(accent))
        .title(" ▶ Now Playing ")
        .style(Style::default().bg(bg));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(pb) = &app.playback else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  No active playback. Open Spotify on a device first.",
                Style::default().fg(muted),
            )),
            inner,
        );
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // ── left: track info ──
    if let Some(track) = &pb.item {
        let artist = track.artists.first().map(|a| a.name.as_str()).unwrap_or("?");
        let status_icon = if pb.is_playing { "▶" } else { "⏸" };

        let mut lines: Vec<Line> = Vec::new();

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
            lines.push(Line::from(Span::styled(artist, Style::default().fg(muted))));
        }
        if np.show_album {
            lines.push(Line::from(Span::styled(&track.album.name, Style::default().fg(muted))));
        }
        if np.show_device {
            if let Some(dev) = &pb.device {
                lines.push(Line::from(Span::styled(
                    format!("📱 {}", dev.name),
                    Style::default().fg(muted),
                )));
            }
        }

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), cols[0]);

        // ── right: progress + shuffle/repeat ──
        let progress = pb.progress_ms.unwrap_or(0);
        let duration = track.duration_ms.max(1);
        let ratio = (progress as f64 / duration as f64).clamp(0.0, 1.0);

        // Build right-side constraints dynamically
        let mut right_constraints = vec![];
        if np.show_progress_bar  { right_constraints.push(Constraint::Length(1)); }
        if np.show_time          { right_constraints.push(Constraint::Length(1)); }
        if np.show_shuffle_repeat { right_constraints.push(Constraint::Length(1)); }

        if right_constraints.is_empty() { return; }

        let right_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(right_constraints)
            .split(cols[1]);

        let mut idx = 0;

        if np.show_progress_bar {
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(accent).bg(Color::Rgb(50, 50, 50)))
                .ratio(ratio);
            frame.render_widget(gauge, right_layout[idx]);
            idx += 1;
        }

        if np.show_time {
            let time_text = Paragraph::new(Line::from(vec![
                Span::styled(format_duration(progress), Style::default().fg(muted)),
                Span::styled(" / ", Style::default().fg(muted)),
                Span::styled(format_duration(duration), Style::default().fg(muted)),
            ]))
                .alignment(Alignment::Center);
            frame.render_widget(time_text, right_layout[idx]);
            idx += 1;
        }

        if np.show_shuffle_repeat {
            let shuffle_style = if app.shuffle {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(muted)
            };
            let repeat_style = if app.repeat != RepeatMode::Off {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(muted)
            };
            let indicators = Paragraph::new(Line::from(vec![
                Span::styled(
                    if app.shuffle { " 🔀 ON " } else { " 🔀 off" },
                    shuffle_style,
                ),
                Span::styled("  ", Style::default()),
                Span::styled(
                    match &app.repeat {
                        RepeatMode::Off     => "🔁 off".to_string(),
                        RepeatMode::Context => "🔁 playlist".to_string(),
                        RepeatMode::Track   => "🔂 track".to_string(),
                    },
                    repeat_style,
                ),
            ]))
                .alignment(Alignment::Center);
            frame.render_widget(indicators, right_layout[idx]);
        }
    }
}

// ── hints bar ────────────────────────────────────────────────────────────────

fn draw_hints(frame: &mut Frame, app: &App, cfg: &Config, area: Rect) {
    let muted  = rgb(cfg.theme.muted);
    let accent = rgb(cfg.theme.accent);
    let k      = &cfg.keys;

    let hints = if app.input_mode == InputMode::Typing {
        format!(" [Enter] Search  [Esc] Cancel")
    } else {
        format!(
            " [{}] Search  [{}] Play/Pause  [{}] Next  [{}] Prev  [{}] Shuffle  [{}] Repeat  [Tab] Panel  [c] Settings  [{}] Quit",
            k.search, k.play_pause.trim(), k.next, k.previous, k.shuffle, k.repeat, k.quit
        )
    };

    let status = app.status_message.as_deref().unwrap_or("");

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    frame.render_widget(
        Paragraph::new(Span::styled(hints, Style::default().fg(muted))),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(status, Style::default().fg(accent)))
            .alignment(Alignment::Right),
        cols[1],
    );
}

// ── utils ─────────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
    } else {
        format!("{:<width$}", s, width = max)
    }
}

// ── Settings screen ───────────────────────────────────────────────────────────

pub fn draw_settings(frame: &mut Frame, app: &App, cfg: &Config) {
    let accent = rgb(cfg.theme.accent);
    let muted  = rgb(cfg.theme.muted);
    let bg     = rgb(cfg.theme.background);
    let hl_bg  = rgb(cfg.theme.highlight_bg);
    let bt     = border_type(&cfg.theme.border_style);

    let size = frame.area();

    // Outer block
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(bt)
        .border_style(Style::default().fg(accent))
        .title(" ⚙️  Settings  (← → switch section · ↑↓ navigate · Enter edit/toggle · Esc close) ")
        .style(Style::default().bg(bg));
    let inner = outer.inner(size);
    frame.render_widget(outer, size);

    // Split: section tabs at top, content below
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    // Section tabs
    let sections = crate::app::SettingsSection::all();
    let tab_width = layout[0].width / sections.len() as u16;
    let tabs_area = layout[0];

    for (i, section) in sections.iter().enumerate() {
        let x = tabs_area.x + i as u16 * tab_width;
        let area = Rect::new(x, tabs_area.y, tab_width, 3);
        let is_selected = i == app.settings.section_selected;
        let style = if is_selected {
            Style::default().fg(bg).bg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted).bg(bg)
        };
        let tab = Paragraph::new(section.label())
            .style(style)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_type(bt)
                .border_style(if is_selected { Style::default().fg(accent) } else { Style::default().fg(muted) }));
        frame.render_widget(tab, area);
    }

    // Section content
    let content_area = layout[1];
    match app.settings.current_section() {
        crate::app::SettingsSection::Theme       => draw_settings_theme(frame, app, cfg, content_area, accent, muted, bg, hl_bg, bt),
        crate::app::SettingsSection::Layout      => draw_settings_layout(frame, app, cfg, content_area, accent, muted, bg, hl_bg, bt),
        crate::app::SettingsSection::NowPlaying  => draw_settings_nowplaying(frame, app, cfg, content_area, accent, muted, bg, hl_bg, bt),
        crate::app::SettingsSection::Keybindings => draw_settings_keybindings(frame, app, cfg, content_area, accent, muted, bg, hl_bg, bt),
    }
}

fn setting_row<'a>(
    label: &'a str,
    value: &'a str,
    selected: bool,
    editing: bool,
    edit_buffer: &'a str,
    accent: Color,
    muted: Color,
) -> ListItem<'a> {
    let val_display = if selected && editing {
        format!("{}_", edit_buffer)
    } else {
        value.to_string()
    };

    let val_style = if selected && editing {
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default().fg(accent)
    } else {
        Style::default().fg(Color::White)
    };

    ListItem::new(Line::from(vec![
        Span::styled(format!("  {:<30}", label), Style::default().fg(if selected { accent } else { muted })),
        Span::styled(val_display, val_style),
    ]))
}

fn bool_val(b: bool) -> &'static str { if b { "✅ on" } else { "☐  off" } }

fn draw_settings_theme(frame: &mut Frame, app: &App, cfg: &Config, area: Rect,
                       accent: Color, muted: Color, bg: Color, hl_bg: Color, bt: BorderType) {

    let t = &cfg.theme;
    let sel = app.settings.item_selected;
    let editing = app.settings.editing;
    let buf = &app.settings.edit_buffer;

    let s_accent  = fmt_rgb(t.accent);
    let s_bg      = fmt_rgb(t.background);
    let s_muted   = fmt_rgb(t.muted);
    let s_hl      = fmt_rgb(t.highlight_bg);
    let items = vec![
        setting_row("Accent color (r,g,b)",     &s_accent,       sel==0, editing && sel==0, buf, accent, muted),
        setting_row("Background color (r,g,b)", &s_bg,           sel==1, editing && sel==1, buf, accent, muted),
        setting_row("Muted color (r,g,b)",      &s_muted,        sel==2, editing && sel==2, buf, accent, muted),
        setting_row("Highlight bg (r,g,b)",     &s_hl,           sel==3, editing && sel==3, buf, accent, muted),
        setting_row("Border style",             &t.border_style, sel==4, editing && sel==4, buf, accent, muted),
    ];

    let mut state = ListState::default();
    state.select(Some(sel));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(bt)
            .border_style(Style::default().fg(muted))
            .title("  Press Enter to edit · type value · Enter to confirm  ")
            .style(Style::default().bg(bg)))
        .highlight_style(Style::default().bg(hl_bg));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_settings_layout(frame: &mut Frame, app: &App, cfg: &Config, area: Rect,
                        accent: Color, muted: Color, bg: Color, hl_bg: Color, bt: BorderType) {

    let l = &cfg.layout;
    let sel = app.settings.item_selected;
    let editing = app.settings.editing;
    let buf = &app.settings.edit_buffer;

    let s_sidebar = l.sidebar_width_pct.to_string();
    let s_np_height = l.now_playing_height.to_string();
    let items = vec![
        setting_row("Sidebar width %",    &s_sidebar,              sel==0, editing && sel==0, buf, accent, muted),
        setting_row("Now playing height", &s_np_height,            sel==1, editing && sel==1, buf, accent, muted),
        setting_row("Show hints bar",     bool_val(l.show_hints),  sel==2, false,              buf, accent, muted),
    ];

    let mut state = ListState::default();
    state.select(Some(sel));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(bt)
            .border_style(Style::default().fg(muted))
            .title("  Enter to edit number · Enter to toggle boolean  ")
            .style(Style::default().bg(bg)))
        .highlight_style(Style::default().bg(hl_bg));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_settings_nowplaying(frame: &mut Frame, app: &App, cfg: &Config, area: Rect,
                            accent: Color, muted: Color, bg: Color, hl_bg: Color, bt: BorderType) {

    let np = &cfg.now_playing;
    let sel = app.settings.item_selected;

    let items = vec![
        setting_row("Show track name",    bool_val(np.show_track_name),    sel==0, false, "", accent, muted),
        setting_row("Show artist",        bool_val(np.show_artist),        sel==1, false, "", accent, muted),
        setting_row("Show album",         bool_val(np.show_album),         sel==2, false, "", accent, muted),
        setting_row("Show progress bar",  bool_val(np.show_progress_bar),  sel==3, false, "", accent, muted),
        setting_row("Show time",          bool_val(np.show_time),          sel==4, false, "", accent, muted),
        setting_row("Show shuffle/repeat",bool_val(np.show_shuffle_repeat),sel==5, false, "", accent, muted),
        setting_row("Show device name",   bool_val(np.show_device),        sel==6, false, "", accent, muted),
    ];

    let mut state = ListState::default();
    state.select(Some(sel));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(bt)
            .border_style(Style::default().fg(muted))
            .title("  Press Enter to toggle  ")
            .style(Style::default().bg(bg)))
        .highlight_style(Style::default().bg(hl_bg));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_settings_keybindings(frame: &mut Frame, app: &App, cfg: &Config, area: Rect,
                             accent: Color, muted: Color, bg: Color, hl_bg: Color, bt: BorderType) {

    let k = &cfg.keys;
    let sel = app.settings.item_selected;
    let editing = app.settings.editing;
    let buf = &app.settings.edit_buffer;

    let rows = [
        ("Quit",         k.quit.as_str()),
        ("Search",       k.search.as_str()),
        ("Play / Pause", k.play_pause.as_str()),
        ("Next track",   k.next.as_str()),
        ("Prev track",   k.previous.as_str()),
        ("Shuffle",      k.shuffle.as_str()),
        ("Repeat",       k.repeat.as_str()),
        ("Next panel",   k.panel_next.as_str()),
        ("Scroll down",  k.scroll_down.as_str()),
        ("Scroll up",    k.scroll_up.as_str()),
        ("Select",       k.select.as_str()),
    ];

    let items: Vec<ListItem> = rows.iter().enumerate().map(|(i, (label, val))| {
        setting_row(label, val, sel==i, editing && sel==i, buf, accent, muted)
    }).collect();

    let mut state = ListState::default();
    state.select(Some(sel));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(bt)
            .border_style(Style::default().fg(muted))
            .title("  Press Enter · type new key · Enter to save  ")
            .style(Style::default().bg(bg)))
        .highlight_style(Style::default().bg(hl_bg));
    frame.render_stateful_widget(list, area, &mut state);
}

fn fmt_rgb(c: [u8; 3]) -> String {
    format!("{},{},{}", c[0], c[1], c[2])
}