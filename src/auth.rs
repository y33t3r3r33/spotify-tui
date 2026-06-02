use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

const TOKEN_FILE: &str = "spotify-tui-tokens.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64, // Unix timestamp
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

pub struct Auth {
    client_id: String,
    redirect_uri: String,
}

impl Auth {
    pub fn new(client_id: String, redirect_uri: String) -> Self {
        Self {
            client_id,
            redirect_uri,
        }
    }

    /// Full PKCE auth flow: open browser, wait for callback, exchange code
    pub async fn authenticate(&self) -> Result<TokenData> {
        let (code_verifier, code_challenge) = Self::pkce_pair();
        let state = Self::random_state();

        let scopes = [
            "user-read-playback-state",
            "user-modify-playback-state",
            "user-read-currently-playing",
            "user-library-read",
            "playlist-read-private",
        ]
            .join(" ");

        let auth_url = format!(
            "https://accounts.spotify.com/authorize\
            ?client_id={}\
            &response_type=code\
            &redirect_uri={}\
            &state={}\
            &scope={}\
            &code_challenge_method=S256\
            &code_challenge={}",
            self.client_id,
            urlencoding::encode(&self.redirect_uri),
            state,
            urlencoding::encode(&scopes),
            code_challenge,
        );

        println!("Opening Spotify login in your browser...");
        println!("If it doesn't open, visit:\n{}\n", auth_url);
        // Spawn browser open on a blocking thread so it doesn't freeze the runtime
        let url_clone = auth_url.clone();
        tokio::task::spawn_blocking(move || open::that(url_clone)).await.ok();

        // Listen for the OAuth callback on localhost
        let code = self.wait_for_callback(&state).await?;

        // Exchange code for tokens
        let tokens = self.exchange_code(&code, &code_verifier).await?;
        self.save_tokens(&tokens)?;

        Ok(tokens)
    }

    /// Attempt to load stored tokens; refresh if expired
    pub async fn load_or_authenticate(&self) -> Result<TokenData> {
        match self.load_tokens() {
            Ok(tokens) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();

                if now < tokens.expires_at - 60 {
                    // Still valid (with 60s buffer)
                    Ok(tokens)
                } else if let Some(refresh_token) = &tokens.refresh_token {
                    // Refresh the access token
                    self.refresh_access_token(refresh_token).await
                } else {
                    self.authenticate().await
                }
            }
            Err(_) => self.authenticate().await,
        }
    }

    /// Spin up a temporary TCP listener to catch the redirect
    async fn wait_for_callback(&self, expected_state: &str) -> Result<String> {
        // Parse port from redirect URI (default 8888)
        let port: u16 = self
            .redirect_uri
            .split(':')
            .last()
            .and_then(|p| p.split('/').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(8888);

        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .map_err(|e| anyhow!("Could not bind to port {}: {}", port, e))?;

        println!("Waiting for Spotify callback on port {}...", port);

        let (mut stream, _) = listener.accept().await?;
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await?;

        // Parse GET /callback?code=...&state=... HTTP/1.1
        let query = request_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| path.split('?').nth(1))
            .ok_or_else(|| anyhow!("Invalid callback request"))?
            .to_string();

        let mut code = None;
        let mut state = None;
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some("code"), Some(v)) => code = Some(v.to_string()),
                (Some("state"), Some(v)) => state = Some(v.to_string()),
                _ => {}
            }
        }

        // Send a simple HTML response back to the browser
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
            <html><body><h2>\xe2\x9c\x85 Authenticated! You can close this tab.</h2></body></html>";
        writer.write_all(response).await.ok();

        if state.as_deref() != Some(expected_state) {
            return Err(anyhow!("State mismatch — possible CSRF"));
        }

        code.ok_or_else(|| anyhow!("No code in callback"))
    }

    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<TokenData> {
        let client = reqwest::Client::new();
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.redirect_uri),
            ("client_id", &self.client_id),
            ("code_verifier", verifier),
        ];

        let resp: TokenResponse = client
            .post("https://accounts.spotify.com/api/token")
            .form(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(self.to_token_data(resp))
    }

    async fn refresh_access_token(&self, refresh_token: &str) -> Result<TokenData> {
        let client = reqwest::Client::new();
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
        ];

        let resp: TokenResponse = client
            .post("https://accounts.spotify.com/api/token")
            .form(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let tokens = self.to_token_data(resp);
        self.save_tokens(&tokens)?;
        Ok(tokens)
    }

    fn to_token_data(&self, resp: TokenResponse) -> TokenData {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        TokenData {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expires_at: now + resp.expires_in,
        }
    }

    // --- PKCE helpers ---

    fn pkce_pair() -> (String, String) {
        let mut bytes = [0u8; 64];
        rand::thread_rng().fill_bytes(&mut bytes);
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        (verifier, challenge)
    }

    fn random_state() -> String {
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    // --- Token persistence ---

    fn token_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(TOKEN_FILE)
    }

    fn save_tokens(&self, tokens: &TokenData) -> Result<()> {
        let path = Self::token_path();
        std::fs::write(&path, serde_json::to_string(tokens)?)?;
        Ok(())
    }

    fn load_tokens(&self) -> Result<TokenData> {
        let data = std::fs::read_to_string(Self::token_path())?;
        Ok(serde_json::from_str(&data)?)
    }
}