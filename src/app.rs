use crate::spotify::{PlaybackState, PlaylistEntry, Track};

#[derive(Debug, Clone, PartialEq)]
pub enum ActivePanel {
    NowPlaying,
    Search,
    Playlists,
    Settings,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Typing,
    EditingValue, // editing a setting value
}

// ── Settings menu state ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsSection {
    Theme,
    Layout,
    NowPlaying,
    Keybindings,
}

impl SettingsSection {
    pub fn all() -> Vec<SettingsSection> {
        vec![
            SettingsSection::Theme,
            SettingsSection::Layout,
            SettingsSection::NowPlaying,
            SettingsSection::Keybindings,
        ]
    }
    pub fn label(&self) -> &'static str {
        match self {
            SettingsSection::Theme       => "🎨  Theme",
            SettingsSection::Layout      => "📐  Layout",
            SettingsSection::NowPlaying  => "🎵  Now Playing",
            SettingsSection::Keybindings => "⌨️   Keybindings",
        }
    }
}

pub struct SettingsState {
    pub section_selected: usize,
    pub item_selected: usize,
    pub edit_buffer: String,    // text being typed when editing a value
    pub editing: bool,          // true while the user is typing a new value
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            section_selected: 0,
            item_selected: 0,
            edit_buffer: String::new(),
            editing: false,
        }
    }

    pub fn current_section(&self) -> SettingsSection {
        SettingsSection::all().remove(self.section_selected)
    }
}

// ── App ───────────────────────────────────────────────────────────────────────

pub struct App {
    pub active_panel: ActivePanel,
    pub input_mode: InputMode,

    // Now playing
    pub playback: Option<PlaybackState>,

    // Search
    pub search_query: String,
    pub search_results: Vec<Track>,
    pub search_selected: usize,

    // Playlists
    pub playlists: Vec<PlaylistEntry>,
    pub playlist_selected: usize,

    // Shuffle / repeat
    pub shuffle: bool,
    pub repeat: RepeatMode,

    // Settings
    pub settings: SettingsState,

    // Status bar
    pub status_message: Option<String>,

    pub should_quit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepeatMode {
    Off,
    Context,
    Track,
}

impl RepeatMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RepeatMode::Off     => "off",
            RepeatMode::Context => "context",
            RepeatMode::Track   => "track",
        }
    }
    pub fn next(&self) -> RepeatMode {
        match self {
            RepeatMode::Off     => RepeatMode::Context,
            RepeatMode::Context => RepeatMode::Track,
            RepeatMode::Track   => RepeatMode::Off,
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            RepeatMode::Off     => "🔁",
            RepeatMode::Context => "🔁",
            RepeatMode::Track   => "🔂",
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            active_panel: ActivePanel::NowPlaying,
            input_mode: InputMode::Normal,
            playback: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            playlists: Vec::new(),
            playlist_selected: 0,
            shuffle: false,
            repeat: RepeatMode::Off,
            settings: SettingsState::new(),
            status_message: None,
            should_quit: false,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    pub fn selected_search_track(&self) -> Option<&Track> {
        self.search_results.get(self.search_selected)
    }

    pub fn selected_playlist(&self) -> Option<&PlaylistEntry> {
        self.playlists.get(self.playlist_selected)
    }

    pub fn sync_playback_state(&mut self, pb: &PlaybackState) {
        self.shuffle = pb.shuffle_state;
        self.repeat = match pb.repeat_state.as_str() {
            "context" => RepeatMode::Context,
            "track"   => RepeatMode::Track,
            _         => RepeatMode::Off,
        };
    }

    pub fn scroll_down(&mut self) {
        match self.active_panel {
            ActivePanel::Search => {
                if !self.search_results.is_empty() {
                    self.search_selected = (self.search_selected + 1) % self.search_results.len();
                }
            }
            ActivePanel::Playlists => {
                if !self.playlists.is_empty() {
                    self.playlist_selected = (self.playlist_selected + 1) % self.playlists.len();
                }
            }
            ActivePanel::Settings => {
                let max = settings_item_count(&self.settings.current_section());
                if max > 0 {
                    self.settings.item_selected = (self.settings.item_selected + 1) % max;
                }
            }
            _ => {}
        }
    }

    pub fn scroll_up(&mut self) {
        match self.active_panel {
            ActivePanel::Search => {
                if !self.search_results.is_empty() {
                    if self.search_selected == 0 { self.search_selected = self.search_results.len() - 1; }
                    else { self.search_selected -= 1; }
                }
            }
            ActivePanel::Playlists => {
                if !self.playlists.is_empty() {
                    if self.playlist_selected == 0 { self.playlist_selected = self.playlists.len() - 1; }
                    else { self.playlist_selected -= 1; }
                }
            }
            ActivePanel::Settings => {
                let max = settings_item_count(&self.settings.current_section());
                if max > 0 {
                    if self.settings.item_selected == 0 { self.settings.item_selected = max - 1; }
                    else { self.settings.item_selected -= 1; }
                }
            }
            _ => {}
        }
    }
}

/// How many items are in each settings section
pub fn settings_item_count(section: &SettingsSection) -> usize {
    match section {
        SettingsSection::Theme       => 5, // accent, background, muted, highlight_bg, border_style
        SettingsSection::Layout      => 3, // sidebar_width_pct, now_playing_height, show_hints
        SettingsSection::NowPlaying  => 7, // 7 toggles
        SettingsSection::Keybindings => 11,
    }
}