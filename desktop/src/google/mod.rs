//! The Google account the Calendar and Tasks integrations share
//! (`GoogleAccount.swift`, `GoogleOAuthLoopbackReceiver.swift`).
//!
//! Sign-in is OAuth with PKCE against the person's own OAuth client ID,
//! through the browser and back to a listener on `127.0.0.1` at a port the
//! system picks. The token, its scopes and the client it belongs to are kept
//! in the keychain; a token for another client ID is dropped, not sent.

pub mod calendar;
pub mod tasks;

use crate::sync::{http, keychain};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const CALENDAR_SCOPES: [&str; 2] = [
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/calendar.readonly",
];
pub const TASKS_SCOPES: [&str; 1] = ["https://www.googleapis.com/auth/tasks"];

const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const CALLBACK_PATH: &str = "/google-oauth-callback";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds since the epoch.
    pub expires_at: i64,
    /// Space-separated.
    pub scopes: String,
    pub client_id: String,
}

impl Token {
    fn has(&self, scopes: &[&str]) -> bool {
        let granted: BTreeSet<&str> = self.scopes.split([' ', ',']).collect();
        scopes.iter().all(|scope| granted.contains(scope))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoogleError {
    /// No client ID, no sign-in, a scope not granted, a refused refresh:
    /// connecting again is the way on.
    SignIn(String),
    Other(String),
}

impl GoogleError {
    pub fn message(&self) -> String {
        match self {
            GoogleError::SignIn(text) | GoogleError::Other(text) => text.clone(),
        }
    }
}

/// One refresh at a time, however many passes find the token spent.
static REFRESHING: Mutex<()> = Mutex::new(());

fn now_s() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn token() -> Option<Token> {
    keychain::load("google")
}

pub fn connected() -> bool {
    token().is_some()
}

pub fn disconnect() {
    keychain::clear("google");
}

/// The Swift app's client ID, for a first prefill.
pub fn swift_client_id() -> Option<String> {
    let out = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "uk.co.maybeitssoftware.takt", "googleOAuthClientID"])
        .output()
        .ok()?;
    let id = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!id.is_empty()).then_some(id)
}

/// A token good for now with `scopes`, refreshed first when spent.
pub fn access_token(
    client_id: &str,
    scopes: &[&str],
    service: &str,
) -> Result<String, GoogleError> {
    if client_id.trim().is_empty() {
        return Err(GoogleError::SignIn(
            "Set the Google OAuth client ID in Settings first.".into(),
        ));
    }
    let _guard = REFRESHING.lock().unwrap_or_else(|p| p.into_inner());
    let Some(token) = token() else {
        return Err(GoogleError::SignIn(format!(
            "Connect {service} in Settings."
        )));
    };
    if token.client_id != client_id.trim() {
        disconnect();
        return Err(GoogleError::SignIn(format!(
            "Connect {service} again: the client ID changed."
        )));
    }
    if !token.has(scopes) {
        return Err(GoogleError::SignIn(format!(
            "Connect {service} again to grant it access."
        )));
    }
    if token.expires_at - 60 > now_s() {
        return Ok(token.access_token);
    }
    let reply = http::form(
        TOKEN,
        &[
            ("client_id", client_id.trim()),
            ("refresh_token", &token.refresh_token),
            ("grant_type", "refresh_token"),
        ],
        Duration::from_secs(30),
    )
    .map_err(GoogleError::Other)?;
    if matches!(reply.status, 400 | 401 | 403) {
        disconnect();
        return Err(GoogleError::SignIn(format!(
            "Google refused the sign-in. Connect {service} again."
        )));
    }
    if !reply.ok() {
        return Err(GoogleError::Other(format!(
            "Google answered {}.",
            reply.status
        )));
    }
    let value: serde_json::Value =
        serde_json::from_str(&reply.body).map_err(|e| GoogleError::Other(e.to_string()))?;
    let refreshed = Token {
        access_token: value["access_token"].as_str().unwrap_or_default().into(),
        refresh_token: value["refresh_token"]
            .as_str()
            .map(str::to_string)
            .unwrap_or(token.refresh_token),
        expires_at: now_s() + value["expires_in"].as_i64().unwrap_or(3600),
        scopes: value["scope"]
            .as_str()
            .map(str::to_string)
            .unwrap_or(token.scopes),
        client_id: token.client_id,
    };
    let _ = keychain::save("google", &refreshed);
    Ok(refreshed.access_token)
}

/// Signs in through the browser, asking for `scopes` and keeping any
/// already granted. Blocks until the browser comes back or five minutes
/// pass, so it runs on a thread.
pub fn connect(client_id: &str, scopes: &[&str]) -> Result<(), String> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        return Err("Set the Google OAuth client ID first.".into());
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    let (verifier, challenge) = crate::sync::auth::pkce();
    let state = uuid::Uuid::new_v4().simple().to_string();
    let mut wanted: BTreeSet<String> = scopes.iter().map(|s| s.to_string()).collect();
    if let Some(token) = token().filter(|t| t.client_id == client_id) {
        wanted.extend(
            token
                .scopes
                .split(' ')
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        );
    }
    let scope = wanted.into_iter().collect::<Vec<_>>().join(" ");
    let url = format!(
        "{AUTHORIZE}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={challenge}&code_challenge_method=S256&state={state}&access_type=offline&prompt=consent&include_granted_scopes=true",
        http::escape(client_id),
        http::escape(&redirect),
        http::escape(&scope),
    );
    std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;

    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    let target = loop {
        if Instant::now() > deadline {
            return Err("Google sign-in timed out.".into());
        }
        let Ok((mut stream, _)) = listener.accept() else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        let _ = BufReader::new(&stream).read_line(&mut line);
        let path = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string();
        if !path.starts_with(CALLBACK_PATH) {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            continue;
        }
        let page = "<html><body style=\"font-family:-apple-system;padding:40px\">Signed in. You can close this tab and go back to Takt.</body></html>";
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        break path;
    };
    let full = format!("http://127.0.0.1{target}");
    if let Some(error) = http::parameter(&full, "error") {
        return Err(format!("Google said no: {error}."));
    }
    if http::parameter(&full, "state").as_deref() != Some(state.as_str()) {
        return Err("That sign-in wasn't the one asked for. Try again.".into());
    }
    let code = http::parameter(&full, "code").ok_or("Google sent no code back.")?;
    let reply = http::form(
        TOKEN,
        &[
            ("client_id", client_id),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect),
            ("grant_type", "authorization_code"),
        ],
        Duration::from_secs(30),
    )?;
    let value: serde_json::Value = serde_json::from_str(&reply.body).unwrap_or_default();
    if !reply.ok() {
        let said = value["error_description"]
            .as_str()
            .or(value["error"].as_str())
            .unwrap_or("it refused");
        return Err(format!("Google refused the sign-in: {said}."));
    }
    let previous = token().filter(|t| t.client_id == client_id);
    let token = Token {
        access_token: value["access_token"].as_str().unwrap_or_default().into(),
        refresh_token: value["refresh_token"]
            .as_str()
            .map(str::to_string)
            .or(previous.map(|t| t.refresh_token))
            .ok_or("Google gave no refresh token.")?,
        expires_at: now_s() + value["expires_in"].as_i64().unwrap_or(3600),
        scopes: value["scope"].as_str().map(str::to_string).unwrap_or(scope),
        client_id: client_id.into(),
    };
    keychain::save("google", &token)
}

/// A Google API request with a bearer token: status and body.
pub(crate) fn send(
    method: &str,
    url: &str,
    token: &str,
    body: Option<&str>,
) -> Result<http::Reply, GoogleError> {
    let bearer = format!("Bearer {token}");
    http::request(
        method,
        url,
        &[("Authorization", &bearer)],
        body,
        Duration::from_secs(60),
    )
    .map_err(GoogleError::Other)
}

/// Google's `{"error": {"message": …}}`, or the status.
pub(crate) fn failure(service: &str, reply: &http::Reply) -> GoogleError {
    if reply.status == 401 {
        return GoogleError::SignIn(format!(
            "Google refused the sign-in. Connect {service} again."
        ));
    }
    let message = serde_json::from_str::<serde_json::Value>(&reply.body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{service} answered {}.", reply.status));
    GoogleError::Other(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_are_checked_as_a_set() {
        let token = Token {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            scopes: format!("{} {}", CALENDAR_SCOPES[1], CALENDAR_SCOPES[0]),
            client_id: String::new(),
        };
        assert!(token.has(&CALENDAR_SCOPES));
        assert!(!token.has(&TASKS_SCOPES));
    }
}
