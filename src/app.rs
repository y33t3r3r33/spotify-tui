use crate::spotify::{PlaybackState, PlaylistEntry, Track};

#[derive(Debug, Clone, PartialEq)]
pub enum ActivePanel {
    NowPlaying,
    Search,
    Playlists,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Typing,
}

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

    // Shuffle / repeat (mirrors what we last sent to Spotify)
    pub shuffle: bool,
    pub repeat: RepeatMode,

    // Status bar
    pub status_message: Option<String>,

    pub should_quit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepeatMode {
    Off,
    Context, // repeat the whole playlist/album
    Track,   // repeat one track
}

impl RepeatMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::Context => "context",
            RepeatMode::Track => "track",
        }
    }

    pub fn next(&self) -> RepeatMode {
        match self {
            RepeatMode::Off => RepeatMode::Context,
            RepeatMode::Context => RepeatMode::Track,
            RepeatMode::Track => RepeatMode::Off,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            RepeatMode::Off => "🔁",
            RepeatMode::Context => "🔁",
            RepeatMode::Track => "🔂",
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

    /// Sync shuffle/repeat state from latest playback poll
    pub fn sync_playback_state(&mut self, pb: &PlaybackState) {
        self.shuffle = pb.shuffle_state;
        self.repeat = match pb.repeat_state.as_str() {
            "context" => RepeatMode::Context,
            "track" => RepeatMode::Track,
            _ => RepeatMode::Off,
        };
    }

    pub fn scroll_down(&mut self) {
        match self.active_panel {
            ActivePanel::Search => {
                if !self.search_results.is_empty() {
                    self.search_selected =
                        (self.search_selected + 1) % self.search_results.len();
                }
            }
            ActivePanel::Playlists => {
                if !self.playlists.is_empty() {
                    self.playlist_selected =
                        (self.playlist_selected + 1) % self.playlists.len();
                }
            }
            _ => {}
        }
    }

    pub fn scroll_up(&mut self) {
        match self.active_panel {
            ActivePanel::Search => {
                if !self.search_results.is_empty() {
                    if self.search_selected == 0 {
                        self.search_selected = self.search_results.len() - 1;
                    } else {
                        self.search_selected -= 1;
                    }
                }
            }
            ActivePanel::Playlists => {
                if !self.playlists.is_empty() {
                    if self.playlist_selected == 0 {
                        self.playlist_selected = self.playlists.len() - 1;
                    } else {
                        self.playlist_selected -= 1;
                    }
                }
            }
            _ => {}
        }
    }
}