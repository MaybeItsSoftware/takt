//! One blocking HTTP request, answered with its status and body whatever the
//! status was. Runs on the sync threads, never on the window's.

use std::time::Duration;

pub struct Reply {
    pub status: u16,
    pub body: String,
}

impl Reply {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

pub fn request(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
    timeout: Duration,
) -> Result<Reply, String> {
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let mut request = agent.request(method, url);
    for (name, value) in headers {
        request = request.set(name, value);
    }
    let answer = match body {
        Some(body) => request
            .set("Content-Type", "application/json")
            .send_string(body),
        None => request.call(),
    };
    let response = match answer {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(error)) => {
            return Err(format!("Couldn't reach the server: {error}"));
        }
    };
    let status = response.status();
    let body = response.into_string().unwrap_or_default();
    Ok(Reply { status, body })
}

/// `text` for a URL's query.
pub fn escape(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A query parameter's value from a URL, unescaped.
pub fn parameter(url: &str, name: &str) -> Option<String> {
    let query = url.split_once(['?', '#'])?.1;
    query
        .split(['&', '#'])
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| unescape(value))
}

fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut ix = 0;
    while ix < bytes.len() {
        match bytes[ix] {
            b'%' if ix + 2 < bytes.len() => {
                match std::str::from_utf8(&bytes[ix + 1..ix + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                {
                    Some(byte) => {
                        out.push(byte);
                        ix += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        ix += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A form POST (`application/x-www-form-urlencoded`), as OAuth wants.
pub fn form(url: &str, pairs: &[(&str, &str)], timeout: Duration) -> Result<Reply, String> {
    let body = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", escape(key), escape(value)))
        .collect::<Vec<_>>()
        .join("&");
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let answer = agent
        .post(url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body);
    let response = match answer {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(error)) => {
            return Err(format!("Couldn't reach the server: {error}"));
        }
    };
    let status = response.status();
    let body = response.into_string().unwrap_or_default();
    Ok(Reply { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_escape_and_read_back() {
        assert_eq!(escape("takt://auth-callback"), "takt%3A%2F%2Fauth-callback");
        assert_eq!(
            parameter("takt://auth-callback?code=ab%2Fc&x=1", "code").as_deref(),
            Some("ab/c")
        );
        assert_eq!(
            parameter(
                "takt://auth-callback#error_description=No+way",
                "error_description"
            )
            .as_deref(),
            Some("No way")
        );
        assert_eq!(parameter("takt://auth-callback", "code"), None);
    }
}
