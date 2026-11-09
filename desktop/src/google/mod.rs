//! The Google account the Calendar and Tasks integrations share
//! (`GoogleAccount.swift`, `GoogleOAuthLoopbackReceiver.swift`).
//!
//! Sign-in is OAuth with PKCE against the person's own OAuth client ID,
//! through the browser and back to a listener on `127.0.0.1` at a port the
//! system picks. The token, its scopes and the client it belongs to are kept
//! in the keychain; a token for another client ID is dropped, not sent.

pub mod calendar;
pub mod drive;
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
/// The Drive files the person picks in Google's Picker, and no others: a
/// non-sensitive scope, so no yearly security assessment (docs/links.md).
pub const DRIVE_FILE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
/// What Drive search asked for before the Picker: every file's name and
/// type. Restricted, so never asked for again; a grant that has it is
/// revoked at the next sign-in so it drops off.
pub const LEGACY_DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.metadata.readonly";

const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const REVOKE: &str = "https://oauth2.googleapis.com/revoke";
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
    pub fn has(&self, scopes: &[&str]) -> bool {
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

/// The Desktop client's secret. Google asks for it on every token call
/// from a Desktop app client, PKCE or not, though it is not secret in any
/// real sense: it ships inside every installed copy.
static CLIENT_SECRET: Mutex<String> = Mutex::new(String::new());

/// Takt's own Desktop client, compiled in by the install script, which
/// lets anyone connect without a Google Cloud project of their own.
pub const BUILT_IN_CLIENT_ID: Option<&str> = option_env!("TAKT_GOOGLE_CLIENT_ID");
pub const BUILT_IN_CLIENT_SECRET: Option<&str> = option_env!("TAKT_GOOGLE_CLIENT_SECRET");

pub fn set_client_secret(secret: &str) {
    *CLIENT_SECRET.lock().unwrap_or_else(|p| p.into_inner()) = secret.trim().to_string();
}

/// `fields` with the client secret added when one is set.
fn with_secret<'a>(fields: &[(&'a str, &'a str)], secret: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut all = fields.to_vec();
    if !secret.is_empty() {
        all.push(("client_secret", secret));
    }
    all
}

fn client_secret() -> String {
    CLIENT_SECRET
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

fn now_s() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The keychain items: the sign-in Calendar and Tasks share, and Drive
/// linking's own. Drive's Picker hands out a token for `drive.file` alone,
/// which Google won't mix with other scopes, so it is kept apart.
const ACCOUNT: &str = "google";
const DRIVE_ACCOUNT: &str = "google-drive";

pub fn token() -> Option<Token> {
    keychain::load(ACCOUNT)
}

/// Drive linking's token, from the last time files were picked.
pub fn drive_token() -> Option<Token> {
    keychain::load(DRIVE_ACCOUNT)
}

pub fn connected() -> bool {
    token().is_some()
}

pub fn disconnect() {
    keychain::clear(ACCOUNT);
    keychain::clear(DRIVE_ACCOUNT);
}

/// The sign-in still holds the restricted Drive scope from before the
/// Picker: connecting again sheds it.
pub fn holds_legacy_drive() -> bool {
    token().is_some_and(|t| t.has(&[LEGACY_DRIVE_SCOPE]))
}

/// Tells Google to drop this client's whole grant, then forgets the token.
fn revoke(token: &Token) {
    let _ = http::form(
        REVOKE,
        &[("token", &token.refresh_token)],
        Duration::from_secs(15),
    );
    disconnect();
}

/// The Swift app's client ID, for a first prefill.
pub fn swift_client_id() -> Option<String> {
    crate::state::adopt::swift_default("googleOAuthClientID")
}

/// A token good for now with `scopes`, refreshed first when spent.
pub fn access_token(
    client_id: &str,
    scopes: &[&str],
    service: &str,
) -> Result<String, GoogleError> {
    access_token_in(ACCOUNT, client_id, scopes, service)
}

/// Drive linking's token good for now, refreshed first when spent.
pub fn drive_access_token(client_id: &str) -> Result<String, GoogleError> {
    access_token_in(
        DRIVE_ACCOUNT,
        client_id,
        &[DRIVE_FILE_SCOPE],
        "Google Drive",
    )
}

fn access_token_in(
    account: &str,
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
    let Some(token) = keychain::load::<Token>(account) else {
        return Err(GoogleError::SignIn(format!(
            "Connect {service} in Settings."
        )));
    };
    if token.client_id != client_id.trim() {
        keychain::clear(account);
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
    let secret = client_secret();
    let reply = http::form(
        TOKEN,
        &with_secret(
            &[
                ("client_id", client_id.trim()),
                ("refresh_token", &token.refresh_token),
                ("grant_type", "refresh_token"),
            ],
            &secret,
        ),
        Duration::from_secs(30),
    )
    .map_err(GoogleError::Other)?;
    if matches!(reply.status, 400 | 401 | 403) {
        keychain::clear(account);
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
    let _ = keychain::save(account, &refreshed);
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
    let mut wanted: BTreeSet<String> = scopes
        .iter()
        .filter(|s| **s != LEGACY_DRIVE_SCOPE && **s != DRIVE_FILE_SCOPE)
        .map(|s| s.to_string())
        .collect();
    // A grant with the restricted Drive scope can't be narrowed: Google
    // adds every scope granted before to a new consent. So it is revoked,
    // and this sign-in asks afresh for what is wanted now.
    if let Some(old) = token().filter(|t| t.has(&[LEGACY_DRIVE_SCOPE])) {
        revoke(&old);
    }
    let previous = token().filter(|t| t.client_id == client_id);
    if let Some(token) = &previous {
        wanted.extend(
            token
                .scopes
                .split(' ')
                .filter(|s| !s.is_empty() && *s != LEGACY_DRIVE_SCOPE)
                .map(str::to_string),
        );
    }
    let scope = wanted.into_iter().collect::<Vec<_>>().join(" ");
    let (mut token, _) = authorize(
        client_id,
        &scope,
        "&include_granted_scopes=true",
        "Signed in. You can close this tab and go back to Takt.",
    )?;
    if token.refresh_token.is_empty() {
        token.refresh_token = previous
            .map(|t| t.refresh_token)
            .ok_or("Google gave no refresh token.")?;
    }
    keychain::save(ACCOUNT, &token)
}

/// Google's Picker for desktop apps: a consent for `drive.file` that shows
/// the person their Drive to choose from (`trigger_onepick`), and comes
/// back with the ids picked. Keeps the token for Drive linking, and
/// answers the ids, none when nothing was picked. Blocks, so it runs on a
/// thread.
pub fn pick_drive_files(client_id: &str) -> Result<Vec<String>, String> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        return Err("Set the Google OAuth client ID first.".into());
    }
    let (mut token, callback) = authorize(
        client_id,
        DRIVE_FILE_SCOPE,
        "&trigger_onepick=true&allow_multiple=true&allow_folder_selection=true",
        "Linked in Takt. You can close this tab.",
    )?;
    if token.refresh_token.is_empty()
        && let Some(previous) = drive_token().filter(|t| t.client_id == client_id)
    {
        token.refresh_token = previous.refresh_token;
    }
    keychain::save(DRIVE_ACCOUNT, &token)?;
    Ok(picked_ids(&callback))
}

/// The comma-separated `picked_file_ids` the Picker's redirect carries.
fn picked_ids(callback: &str) -> Vec<String> {
    http::parameter(callback, "picked_file_ids")
        .map(|ids| {
            ids.split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The browser's round trip: Google's consent for `scope` with `extra`
/// parameters, back to a listener on the loopback, and the code exchanged.
/// Answers the token (its refresh token empty when Google sent none) and
/// the address the browser came back to.
fn authorize(
    client_id: &str,
    scope: &str,
    extra: &str,
    done: &str,
) -> Result<(Token, String), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    let (verifier, challenge) = crate::sync::auth::pkce();
    let state = uuid::Uuid::new_v4().simple().to_string();
    let url = format!(
        "{AUTHORIZE}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={challenge}&code_challenge_method=S256&state={state}&access_type=offline&prompt=consent{extra}",
        http::escape(client_id),
        http::escape(&redirect),
        http::escape(scope),
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
        let page = format!(
            "<html><body style=\"font-family:-apple-system;padding:40px\">{done}</body></html>"
        );
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
    let secret = client_secret();
    let reply = http::form(
        TOKEN,
        &with_secret(
            &[
                ("client_id", client_id),
                ("code", &code),
                ("code_verifier", &verifier),
                ("redirect_uri", &redirect),
                ("grant_type", "authorization_code"),
            ],
            &secret,
        ),
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
    let token = Token {
        access_token: value["access_token"].as_str().unwrap_or_default().into(),
        refresh_token: value["refresh_token"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        expires_at: now_s() + value["expires_in"].as_i64().unwrap_or(3600),
        scopes: value["scope"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| scope.to_string()),
        client_id: client_id.into(),
    };
    Ok((token, full))
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

    #[test]
    fn the_pickers_redirect_names_the_files() {
        let back = "http://127.0.0.1/google-oauth-callback?code=c&state=s&picked_file_ids=1a%2C2b,3c&scope=x";
        assert_eq!(picked_ids(back), vec!["1a", "2b", "3c"]);
        assert!(picked_ids("http://127.0.0.1/google-oauth-callback?code=c").is_empty());
    }
}
