//! Choosing Drive files to link, through Google's own Picker (docs/links.md).
//!
//! Takt asks for `drive.file`, which reaches only the files a person picks
//! in the Picker (or that Takt made), so nothing here can list a Drive.
//! GPUI has no web view, and Google no longer shows the JavaScript Picker
//! inside one anyway, so the Mac uses the Picker for desktop apps: an OAuth
//! consent with `trigger_onepick=true` in the browser, which shows the
//! person their Drive and comes back to the sign-in's loopback listener with
//! `picked_file_ids` beside the code (`super::pick_drive_files`). Each id is
//! then read with `files.get` for its name, type and address.

use super::{GoogleError, failure, send};
use crate::sync::http;

/// A file the person picked, ready to link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub url: String,
    pub name: String,
    pub mime_type: Option<String>,
}

const FOLDER: &str = "application/vnd.google-apps.folder";

/// A Drive id as Google makes them; anything else is not asked about.
fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The address a picked file is linked by: Drive's own viewing address when
/// it is a Drive or Docs one, else the one made from the id.
pub fn link_url(id: &str, mime_type: Option<&str>, url: Option<&str>) -> Option<String> {
    if let Some(url) = url.map(str::trim).filter(|u| {
        u.starts_with("https://docs.google.com/") || u.starts_with("https://drive.google.com/")
    }) {
        return Some(url.to_string());
    }
    let id = id.trim();
    if !is_id(id) {
        return None;
    }
    Some(if mime_type == Some(FOLDER) {
        format!("https://drive.google.com/drive/folders/{id}")
    } else {
        format!("https://drive.google.com/file/d/{id}/view")
    })
}

/// Drive API v3 `files.get` for what a link needs.
fn file_url(id: &str) -> String {
    format!(
        "https://www.googleapis.com/drive/v3/files/{id}?fields={}&supportsAllDrives=true",
        http::escape("id,name,mimeType,webViewLink"),
    )
}

/// One `files.get` answer as a link.
pub fn read_file(id: &str, body: &str) -> Option<Picked> {
    let file: serde_json::Value = serde_json::from_str(body).ok()?;
    let mime_type = file["mimeType"].as_str().map(str::to_string);
    let url = link_url(id, mime_type.as_deref(), file["webViewLink"].as_str())?;
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
}

/// The picked ids as links, each read from Drive with `token`. A file Drive
/// won't describe is still linked by its id, unnamed.
pub fn describe(token: &str, ids: &[String]) -> Result<Vec<Picked>, GoogleError> {
    let mut out = Vec::new();
    for id in ids.iter().map(|id| id.trim()).filter(|id| is_id(id)) {
        let reply = send("GET", &file_url(id), token, None)?;
        if reply.status == 401 {
            return Err(failure("Google Drive", &reply));
        }
        let picked = reply
            .ok()
            .then(|| read_file(id, &reply.body))
            .flatten()
            .or_else(|| {
                link_url(id, None, None).map(|url| Picked {
                    url,
                    name: "Google Drive file".into(),
                    mime_type: None,
                })
            });
        out.extend(picked);
    }
    Ok(out)
}

/// The file id in a Drive or Docs address: `/d/<id>`, `/folders/<id>` or
/// `?id=<id>`. None for anything else.
pub fn file_id(url: &str) -> Option<String> {
    let url = url.trim();
    if !(url.starts_with("https://drive.google.com/")
        || url.starts_with("https://docs.google.com/"))
    {
        return None;
    }
    if let Some(id) = http::parameter(url, "id").filter(|id| is_id(id)) {
        return Some(id);
    }
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let mut segments = path.split('/');
    while let Some(segment) = segments.next() {
        if matches!(segment, "d" | "folders")
            && let Some(id) = segments.next().filter(|id| is_id(id))
        {
            return Some(id.to_string());
        }
    }
    None
}

/// A picture of the file, as Drive draws it for its own lists, about
/// `size` pixels on its long side: PNG or JPEG bytes. None when Drive has
/// none for it (a folder, a file it can't render).
pub fn thumbnail(token: &str, id: &str, size: u32) -> Result<Option<Vec<u8>>, GoogleError> {
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{id}?fields={}&supportsAllDrives=true",
        http::escape("thumbnailLink,hasThumbnail"),
    );
    let reply = super::send("GET", &url, token, None)?;
    if !reply.ok() {
        return Err(super::failure("Google Drive", &reply));
    }
    let file: serde_json::Value = serde_json::from_str(&reply.body).unwrap_or_default();
    let Some(link) = file["thumbnailLink"].as_str() else {
        return Ok(None);
    };
    // Drive's link ends in its size, `=s220`; ask for the one wanted.
    let link = match link.rfind("=s") {
        Some(at) => format!("{}=s{size}", &link[..at]),
        None => link.to_string(),
    };
    let bearer = format!("Bearer {token}");
    let (status, bytes, _) = http::get_bytes(
        &link,
        &[("Authorization", &bearer)],
        std::time::Duration::from_secs(20),
    )
    .map_err(GoogleError::Other)?;
    Ok(((200..300).contains(&status) && !bytes.is_empty()).then_some(bytes))
}

/// What a Google Docs file is saved as, being no file of its own: the
/// export's type and the extension it takes. None for an ordinary file.
fn export_as(mime_type: &str) -> Option<(&'static str, &'static str)> {
    Some(
        match mime_type.strip_prefix("application/vnd.google-apps.")? {
            "document" => (
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "docx",
            ),
            "spreadsheet" => (
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "xlsx",
            ),
            "presentation" => (
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                "pptx",
            ),
            "drawing" => ("image/png", "png"),
            _ => ("application/pdf", "pdf"),
        },
    )
}

/// The file's name and contents, ready to write: Google Docs exported as
/// Word, Excel or PowerPoint files, everything else as Drive keeps it.
pub fn download(token: &str, id: &str) -> Result<(String, Vec<u8>), GoogleError> {
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{id}?fields={}&supportsAllDrives=true",
        http::escape("name,mimeType"),
    );
    let reply = super::send("GET", &url, token, None)?;
    if !reply.ok() {
        return Err(super::failure("Google Drive", &reply));
    }
    let file: serde_json::Value = serde_json::from_str(&reply.body).unwrap_or_default();
    let mut name = file["name"].as_str().unwrap_or("Untitled").to_string();
    let mime = file["mimeType"].as_str().unwrap_or_default();
    if mime == FOLDER {
        return Err(GoogleError::Other(
            "A folder can't be saved as one file.".into(),
        ));
    }
    let content = match export_as(mime) {
        Some((as_type, extension)) => {
            if !name.to_lowercase().ends_with(&format!(".{extension}")) {
                name = format!("{name}.{extension}");
            }
            format!(
                "https://www.googleapis.com/drive/v3/files/{id}/export?mimeType={}",
                http::escape(as_type)
            )
        }
        None => format!(
            "https://www.googleapis.com/drive/v3/files/{id}?alt=media&supportsAllDrives=true"
        ),
    };
    let bearer = format!("Bearer {token}");
    let (status, bytes, _) = http::get_bytes(
        &content,
        &[("Authorization", &bearer)],
        std::time::Duration::from_secs(120),
    )
    .map_err(GoogleError::Other)?;
    if !(200..300).contains(&status) {
        return Err(GoogleError::Other(format!(
            "Google Drive wouldn't send the file ({status})."
        )));
    }
    // A name is a file name here, never a path.
    Ok((name.replace(['/', ':'], "-"), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_links_by_drives_own_address() {
        let sheet = read_file(
            "1",
            r#"{"id":"1","name":"Budget","mimeType":"application/vnd.google-apps.spreadsheet","webViewLink":"https://docs.google.com/spreadsheets/d/1/edit?usp=drivesdk"}"#,
        )
        .unwrap();
        assert_eq!(
            sheet.url,
            "https://docs.google.com/spreadsheets/d/1/edit?usp=drivesdk"
        );
        assert_eq!(sheet.name, "Budget");
        let folder = read_file(
            "2",
            r#"{"id":"2","name":"Trips","mimeType":"application/vnd.google-apps.folder"}"#,
        )
        .unwrap();
        assert_eq!(folder.url, "https://drive.google.com/drive/folders/2");
        let pdf = read_file(
            "3",
            r#"{"name":"","mimeType":"application/pdf","webViewLink":"javascript:alert(1)"}"#,
        )
        .unwrap();
        assert_eq!(pdf.url, "https://drive.google.com/file/d/3/view");
        assert_eq!(pdf.name, "Untitled");
        assert_eq!(pdf.mime_type.as_deref(), Some("application/pdf"));
        assert!(read_file("../x", r#"{"name":"Bad"}"#).is_none());
        assert!(read_file("4", "nope").is_none());
    }

    #[test]
    fn only_drive_ids_reach_the_address() {
        assert!(is_id("1AbC-d_9"));
        assert!(!is_id("a/b"));
        assert!(!is_id(""));
        assert!(file_url("abc").starts_with("https://www.googleapis.com/drive/v3/files/abc?"));
    }

    #[test]
    fn a_file_id_is_read_from_drive_and_docs_addresses() {
        use super::file_id;
        assert_eq!(
            file_id("https://drive.google.com/file/d/1AbC_d-9/view?usp=sharing").as_deref(),
            Some("1AbC_d-9")
        );
        assert_eq!(
            file_id("https://docs.google.com/document/d/XyZ123/edit").as_deref(),
            Some("XyZ123")
        );
        assert_eq!(
            file_id("https://drive.google.com/open?id=Qq7").as_deref(),
            Some("Qq7")
        );
        assert_eq!(
            file_id("https://drive.google.com/drive/folders/F0ld3r").as_deref(),
            Some("F0ld3r")
        );
        assert_eq!(file_id("https://example.com/d/nope"), None);
    }
}
