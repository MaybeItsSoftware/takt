//! Choosing Drive files to link, through Google's own Picker (docs/links.md).
//!
//! Takt asks for `drive.file`, which reaches only the files a person picks
//! in the Picker (or that Takt made), so nothing here can list a Drive. The
//! Picker is JavaScript and GPUI has no web view, so it runs in the
//! browser: a page served on `127.0.0.1` at a port the system picks, as the
//! sign-in's redirect is. The page loads Google's Picker with the signed-in
//! token, the Picker API key and the project's number, and posts what was
//! chosen back to the same listener; the app then links each file.
//!
//! The page carries an access token, so it is served only at a path with a
//! one-off nonce, only on the loopback, and only until the choice comes
//! back or ten minutes pass.

use crate::sync::http;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// A file the person picked, ready to link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub url: String,
    pub name: String,
    pub mime_type: Option<String>,
}

const PAGE_PATH: &str = "/drive-picker";
const PICKED_PATH: &str = "/drive-picker/picked";
const FOLDER: &str = "application/vnd.google-apps.folder";

/// The Cloud project's number, which the Picker needs as its app ID for a
/// pick to grant `drive.file` access: the digits an OAuth client ID starts
/// with (`<number>-<id>.apps.googleusercontent.com`).
pub fn project_number(client_id: &str) -> Option<String> {
    let number = client_id.trim().split('-').next()?;
    (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then(|| number.to_string())
}

/// The address a picked file is linked by: the Picker's own when it is a
/// Drive or Docs one, else the viewing address made from the id.
pub fn link_url(id: &str, mime_type: Option<&str>, url: Option<&str>) -> Option<String> {
    if let Some(url) = url.map(str::trim).filter(|u| {
        u.starts_with("https://docs.google.com/") || u.starts_with("https://drive.google.com/")
    }) {
        return Some(url.to_string());
    }
    let id = id.trim();
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    Some(if mime_type == Some(FOLDER) {
        format!("https://drive.google.com/drive/folders/{id}")
    } else {
        format!("https://drive.google.com/file/d/{id}/view")
    })
}

/// What the page posts: `{"files": [{id, name, mimeType, url}]}`, or
/// `{"cancelled": true}` for none.
pub fn read_picked(body: &str) -> Result<Vec<Picked>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "The Drive picker sent something unreadable.")?;
    Ok(value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let mime_type = file["mimeType"].as_str().map(str::to_string);
            let url = link_url(
                file["id"].as_str().unwrap_or_default(),
                mime_type.as_deref(),
                file["url"].as_str(),
            )?;
            let name = file["name"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .unwrap_or("Untitled")
                .to_string();
            Some(Picked {
                url,
                name,
                mime_type,
            })
        })
        .collect())
}

/// A JavaScript string literal of `text`, safe inside a `<script>`.
fn literal(text: &str) -> String {
    serde_json::to_string(text)
        .unwrap_or_else(|_| "\"\"".into())
        .replace("</", "<\\/")
}

/// The page that runs the Picker. Styled as the house style asks, since the
/// Picker itself is Google's.
pub fn page(token: &str, api_key: &str, app_id: &str, state: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Takt · Link from Drive</title>
<style>
:root {{ --paper:#faf8f4; --ink:#444054; --muted:#6e6b7c; --border:#e6e4ea; }}
@media (prefers-color-scheme: dark) {{ :root {{ --paper:#1c1a23; --ink:#f5f4f7; --muted:#b6b3bf; --border:#34313f; }} }}
body {{ margin:0; background:var(--paper); color:var(--ink); font:16px/1.5 Arvo, Georgia, serif; }}
main {{ max-width:520px; margin:64px auto; padding:0 16px; border-top:1px solid var(--border); }}
.label {{ font:700 10px/1 ui-monospace, monospace; letter-spacing:.15em; text-transform:uppercase; color:var(--muted); margin:16px 0 8px; }}
</style></head>
<body><main><p class="label">Takt · Google Drive</p><p id="say">Opening Google Drive…</p></main>
<script>
const TOKEN = {token}, KEY = {key}, APP = {app}, STATE = {state};
function say(text) {{ document.getElementById('say').textContent = text; }}
function send(body) {{
  fetch('{picked}?state=' + encodeURIComponent(STATE), {{
    method: 'POST', headers: {{ 'Content-Type': 'application/json' }}, body: JSON.stringify(body)
  }}).then(() => {{
    say(body.cancelled ? 'Nothing linked. You can close this tab.' : 'Linked in Takt. You can close this tab.');
    window.close();
  }}, () => say('Takt stopped waiting. Pick again from the link picker.'));
}}
function show() {{
  const files = new google.picker.DocsView(google.picker.ViewId.DOCS)
    .setIncludeFolders(true).setSelectFolderEnabled(true).setMode(google.picker.DocsViewMode.LIST);
  const shared = new google.picker.DocsView(google.picker.ViewId.DOCS)
    .setEnableDrives(true).setIncludeFolders(true).setSelectFolderEnabled(true);
  new google.picker.PickerBuilder()
    .addView(files).addView(shared).addView(google.picker.ViewId.RECENTLY_PICKED)
    .enableFeature(google.picker.Feature.MULTISELECT_ENABLED)
    .enableFeature(google.picker.Feature.SUPPORT_DRIVES)
    .setOAuthToken(TOKEN).setDeveloperKey(KEY).setAppId(APP)
    .setTitle('Link to the task')
    .setCallback(data => {{
      const action = data[google.picker.Response.ACTION];
      if (action === google.picker.Action.PICKED) {{
        send({{ files: data[google.picker.Response.DOCUMENTS].map(d => ({{
          id: d[google.picker.Document.ID], name: d[google.picker.Document.NAME],
          mimeType: d[google.picker.Document.MIME_TYPE], url: d[google.picker.Document.URL]
        }})) }});
      }} else if (action === google.picker.Action.CANCEL) {{
        send({{ cancelled: true }});
      }}
    }})
    .build().setVisible(true);
}}
</script>
<script src="https://apis.google.com/js/api.js" onload="gapi.load('picker', show)" onerror="say('Google Drive could not be reached.')"></script>
</body></html>"#,
        token = literal(token),
        key = literal(api_key),
        app = literal(app_id),
        state = literal(state),
        picked = PICKED_PATH,
    )
}

struct Request {
    method: String,
    path: String,
    body: String,
}

/// One HTTP request off `stream`: its line, and a body by its length.
fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    // A choice is a few hundred bytes a file; anything past a megabyte
    // isn't the page's.
    let mut body = vec![0; length.min(1 << 20)];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn reply(mut stream: &TcpStream, status: &str, kind: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// Opens the Picker in the browser and waits for the choice: the files,
/// none when the person cancelled. Blocks, so it runs on a thread.
pub fn pick(token: &str, api_key: &str, app_id: &str) -> Result<Vec<Picked>, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = uuid::Uuid::new_v4().simple().to_string();
    let url = format!("http://127.0.0.1:{port}{PAGE_PATH}?state={state}");
    std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if Instant::now() > deadline {
            return Err("The Drive picker timed out.".into());
        }
        let Ok((stream, _)) = listener.accept() else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let Some(request) = read_request(&stream) else {
            continue;
        };
        let full = format!("http://127.0.0.1{}", request.path);
        let path = request.path.split('?').next().unwrap_or_default();
        let ours = http::parameter(&full, "state").as_deref() == Some(state.as_str());
        match (request.method.as_str(), path) {
            ("GET", PAGE_PATH) if ours => reply(
                &stream,
                "200 OK",
                "text/html; charset=utf-8",
                &page(token, api_key, app_id, &state),
            ),
            ("POST", PICKED_PATH) if ours => {
                reply(&stream, "204 No Content", "text/plain", "");
                return read_picked(&request.body);
            }
            _ => reply(&stream, "404 Not Found", "text/plain", ""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_number_is_the_client_ids_digits() {
        assert_eq!(
            project_number("123456-abc.apps.googleusercontent.com").as_deref(),
            Some("123456")
        );
        assert_eq!(project_number("abc-123.apps.googleusercontent.com"), None);
        assert_eq!(project_number(""), None);
    }

    #[test]
    fn a_pick_links_by_drives_own_address() {
        let body = r#"{"files":[
            {"id":"1","name":"Budget","mimeType":"application/vnd.google-apps.spreadsheet","url":"https://docs.google.com/spreadsheets/d/1/edit?usp=drive_web"},
            {"id":"2","name":"Trips","mimeType":"application/vnd.google-apps.folder"},
            {"id":"3","name":"","mimeType":"application/pdf","url":"javascript:alert(1)"},
            {"id":"../x","name":"Bad"}
        ]}"#;
        let picked = read_picked(body).unwrap();
        assert_eq!(picked.len(), 3);
        assert_eq!(
            picked[0].url,
            "https://docs.google.com/spreadsheets/d/1/edit?usp=drive_web"
        );
        assert_eq!(picked[1].url, "https://drive.google.com/drive/folders/2");
        assert_eq!(picked[2].url, "https://drive.google.com/file/d/3/view");
        assert_eq!(picked[2].name, "Untitled");
        assert_eq!(picked[2].mime_type.as_deref(), Some("application/pdf"));
        assert!(read_picked(r#"{"cancelled":true}"#).unwrap().is_empty());
        assert!(read_picked("nope").is_err());
    }

    #[test]
    fn the_page_cannot_be_broken_out_of() {
        let page = page("tok</script><b>", "key", "1", "s");
        assert!(!page.contains("tok</script>"));
        assert!(page.contains(r#""tok<\/script><b>""#));
    }

    #[test]
    fn the_page_answers_only_with_its_nonce() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let body = r#"{"files":[]}"#;
            write!(
                stream,
                "POST /drive-picker/picked?state=x HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let (stream, _) = listener.accept().unwrap();
        let request = read_request(&stream).unwrap();
        client.join().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/drive-picker/picked?state=x");
        assert_eq!(request.body, r#"{"files":[]}"#);
    }
}
