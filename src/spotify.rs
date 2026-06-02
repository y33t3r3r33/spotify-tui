use anyhow::Result;
use reqwest::{header, Client};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Artist {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Album {
    pub name: String,
    pub images: Vec<SpotifyImage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SpotifyImage {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artists: Vec<Artist>,
    pub album: Album,
    pub duration_ms: u64,
    pub uri: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub progress_ms: Option<u64>,
    pub item: Option<Track>,
    pub device: Option<Device>,
    pub shuffle_state: bool,
    pub repeat_state: String, // "off" | "context" | "track"
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    tracks: TrackPage,
}

#[derive(Debug, Deserialize)]
struct TrackPage {
    items: Vec<Track>,
}

/// A playlist entry shown in the sidebar. Can be a real Spotify playlist,
/// Liked Songs, or the AI DJ.
#[derive(Debug, Clone)]
pub struct PlaylistEntry {
    pub name: String,
    pub uri: String,
    pub total_tracks: u32,
    pub kind: PlaylistKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaylistKind {
    Normal,
    LikedSongs,
    Dj,
}

// Raw Spotify API types for deserialization
#[derive(Debug, Deserialize)]
struct SimplifiedPlaylist {
    name: String,
    uri: String,
    tracks: PlaylistTrackRef,
}

#[derive(Debug, Deserialize)]
struct PlaylistTrackRef {
    total: u32,
}

#[derive(Debug, Deserialize)]
struct PlaylistsResponse {
    items: Vec<SimplifiedPlaylist>,
}

#[derive(Debug, Deserialize)]
struct SavedTracksResponse {
    total: u32,
}

#[derive(Debug, Deserialize)]
struct UserProfile {
    id: String,
}

pub struct SpotifyClient {
    client: Client,
    access_token: String,
}

impl SpotifyClient {
    pub fn new(access_token: String) -> Self {
        Self {
            client: Client::new(),
            access_token,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    pub async fn get_active_device_id(&self) -> Option<String> {
        let resp = self
            .client
            .get("https://api.spotify.com/v1/me/player/devices")
            .header(header::AUTHORIZATION, self.auth_header())
            .send()
            .await
            .ok()?;

        let data: DevicesResponse = resp.json().await.ok()?;
        data.devices
            .iter()
            .find(|d| d.is_active)
            .or_else(|| data.devices.first())
            .and_then(|d| d.id.clone())
    }

    pub async fn current_playback(&self) -> Result<Option<PlaybackState>> {
        let resp = self
            .client
            .get("https://api.spotify.com/v1/me/player")
            .header(header::AUTHORIZATION, self.auth_header())
            .send()
            .await?;

        if resp.status() == 204 {
            return Ok(None);
        }

        Ok(Some(resp.error_for_status()?.json().await?))
    }

    pub async fn play(&self, device_id: Option<&str>) -> Result<()> {
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/play")
            .header(header::AUTHORIZATION, self.auth_header())
            .header(header::CONTENT_LENGTH, "0");
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn pause(&self, device_id: Option<&str>) -> Result<()> {
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/pause")
            .header(header::AUTHORIZATION, self.auth_header())
            .header(header::CONTENT_LENGTH, "0");
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn next_track(&self, device_id: Option<&str>) -> Result<()> {
        let mut req = self
            .client
            .post("https://api.spotify.com/v1/me/player/next")
            .header(header::AUTHORIZATION, self.auth_header())
            .header(header::CONTENT_LENGTH, "0");
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn previous_track(&self, device_id: Option<&str>) -> Result<()> {
        let mut req = self
            .client
            .post("https://api.spotify.com/v1/me/player/previous")
            .header(header::AUTHORIZATION, self.auth_header())
            .header(header::CONTENT_LENGTH, "0");
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn search_tracks(&self, query: &str) -> Result<Vec<Track>> {
        let resp: SearchResponse = self
            .client
            .get("https://api.spotify.com/v1/search")
            .header(header::AUTHORIZATION, self.auth_header())
            .query(&[("q", query), ("type", "track"), ("limit", "20")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.tracks.items)
    }

    pub async fn play_track(&self, track_uri: &str, device_id: Option<&str>) -> Result<()> {
        let body = serde_json::json!({ "uris": [track_uri] });
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/play")
            .header(header::AUTHORIZATION, self.auth_header())
            .json(&body);
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn play_context(&self, context_uri: &str, device_id: Option<&str>) -> Result<()> {
        let body = serde_json::json!({ "context_uri": context_uri });
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/play")
            .header(header::AUTHORIZATION, self.auth_header())
            .json(&body);
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    async fn get_user_id(&self) -> Result<String> {
        let profile: UserProfile = self
            .client
            .get("https://api.spotify.com/v1/me")
            .header(header::AUTHORIZATION, self.auth_header())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(profile.id)
    }

    pub async fn play_liked_songs(&self, device_id: Option<&str>) -> Result<()> {
        // Liked Songs context URI requires the real user ID
        let user_id = self.get_user_id().await?;
        let collection_uri = format!("spotify:user:{}:collection", user_id);

        let body = serde_json::json!({ "context_uri": collection_uri });
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/play")
            .header(header::AUTHORIZATION, self.auth_header())
            .json(&body);
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        let resp = req.send().await?;
        if resp.status().is_success() || resp.status() == 204 {
            return Ok(());
        }
        // Fallback: fetch first 50 saved tracks and play directly
        self.play_liked_songs_fallback(device_id).await
    }

    async fn play_liked_songs_fallback(&self, device_id: Option<&str>) -> Result<()> {
        #[derive(Deserialize)]
        struct SavedTrackItem { track: Track }
        #[derive(Deserialize)]
        struct SavedTracksPage { items: Vec<SavedTrackItem> }

        let page: SavedTracksPage = self
            .client
            .get("https://api.spotify.com/v1/me/tracks")
            .header(header::AUTHORIZATION, self.auth_header())
            .query(&[("limit", "50")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let uris: Vec<&str> = page.items.iter().map(|i| i.track.uri.as_str()).collect();
        if uris.is_empty() {
            return Err(anyhow::anyhow!("No liked songs found"));
        }

        let body = serde_json::json!({ "uris": uris });
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/play")
            .header(header::AUTHORIZATION, self.auth_header())
            .json(&body);
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn set_shuffle(&self, state: bool, device_id: Option<&str>) -> Result<()> {
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/shuffle")
            .header(header::AUTHORIZATION, self.auth_header())
            .header(header::CONTENT_LENGTH, "0")
            .query(&[("state", state.to_string())]);
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    /// state: "off" | "context" | "track"
    pub async fn set_repeat(&self, state: &str, device_id: Option<&str>) -> Result<()> {
        let mut req = self
            .client
            .put("https://api.spotify.com/v1/me/player/repeat")
            .header(header::AUTHORIZATION, self.auth_header())
            .header(header::CONTENT_LENGTH, "0")
            .query(&[("state", state)]);
        if let Some(id) = device_id {
            req = req.query(&[("device_id", id)]);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    /// Fetch all playlists, prepend Liked Songs and DJ as special entries
    pub async fn get_all_playlist_entries(&self) -> Result<Vec<PlaylistEntry>> {
        // Get liked songs count
        let liked_total = self.get_liked_songs_count().await.unwrap_or(0);

        // Fetch user playlists
        let resp: PlaylistsResponse = self
            .client
            .get("https://api.spotify.com/v1/me/playlists")
            .header(header::AUTHORIZATION, self.auth_header())
            .query(&[("limit", "50")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut entries = Vec::new();

        // Liked Songs always first
        entries.push(PlaylistEntry {
            name: "❤️  Liked Songs".to_string(),
            uri: "liked_songs".to_string(), // handled specially
            total_tracks: liked_total,
            kind: PlaylistKind::LikedSongs,
        });

        // DJ — look for it in playlists (it shows up as "DJ" from spotify user)
        // Also add as a hardcoded fallback entry
        let has_dj = resp.items.iter().any(|p| {
            p.uri.contains("spotify:user:spotify") || p.name.to_lowercase() == "dj"
        });

        if !has_dj {
            entries.push(PlaylistEntry {
                name: "🎧  AI DJ".to_string(),
                uri: "dj".to_string(), // handled specially
                total_tracks: 0,
                kind: PlaylistKind::Dj,
            });
        }

        // Normal playlists
        for p in resp.items {
            let is_dj = p.uri.contains("spotify:user:spotify") || p.name.to_lowercase() == "dj";
            entries.push(PlaylistEntry {
                name: if is_dj {
                    format!("🎧  {}", p.name)
                } else {
                    p.name
                },
                uri: p.uri,
                total_tracks: p.tracks.total,
                kind: if is_dj { PlaylistKind::Dj } else { PlaylistKind::Normal },
            });
        }

        Ok(entries)
    }

    async fn get_liked_songs_count(&self) -> Result<u32> {
        let resp: SavedTracksResponse = self
            .client
            .get("https://api.spotify.com/v1/me/tracks")
            .header(header::AUTHORIZATION, self.auth_header())
            .query(&[("limit", "1")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.total)
    }
}

pub fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{}:{:02}", total_secs / 60, total_secs % 60)
}