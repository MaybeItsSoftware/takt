//! Where a device syncs, as typed under "Use a different server": the sync
//! server, and the Supabase project whose accounts it trusts
//! (`SyncEndpoints.resolve` on the Mac, iPhone and Android). The three travel
//! together because the server checks every token against one project.
//!
//! Addresses are read here rather than with each platform's URL type, so a
//! typed address means the same on every device. What the platforms disagreed
//! on, the Mac's answer won, with two exceptions where the Mac kept an
//! address nothing could open: a scheme that only starts with `http`
//! (`httpx://`) and a user and password in the address are refused, as
//! Android refused them.

/// A sync server, a Supabase project and that project's publishable key.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncEndpointsRecord {
    pub server_url: String,
    pub supabase_url: String,
    pub supabase_key: String,
}

/// What typed endpoints came to: the endpoints, or why they can't be used.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncEndpointsResolution {
    pub endpoints: Option<SyncEndpointsRecord>,
    /// A sentence to show.
    pub problem: Option<String>,
}

/// An address read into its parts. Scheme and host are lowercased.
struct Address {
    scheme: String,
    host: String,
    port: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

impl Address {
    fn text(&self, with_query: bool) -> String {
        let mut out = format!("{}://{}", self.scheme, self.host);
        if let Some(port) = &self.port {
            out.push(':');
            out.push_str(port);
        }
        out.push_str(&self.path);
        if with_query {
            if let Some(query) = &self.query {
                out.push('?');
                out.push_str(query);
            }
            if let Some(fragment) = &self.fragment {
                out.push('#');
                out.push_str(fragment);
            }
        }
        out
    }
}

fn parse(typed: &str) -> Option<Address> {
    let trimmed = typed.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let (scheme, rest) = with_scheme.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let (rest, fragment) = match rest.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment.to_string())),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, Some(query.to_string())),
        None => (rest, None),
    };
    let (authority, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], rest[slash..].to_string()),
        None => (rest, String::new()),
    };
    // A user and password in the address is never what was meant.
    if authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (inside, after) = bracketed.split_once(']')?;
        let port = match after {
            "" => None,
            _ => Some(after.strip_prefix(':')?),
        };
        (format!("[{inside}]"), port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (authority.to_string(), None),
        }
    };
    let port = match port {
        None | Some("") => None,
        Some(port) if port.chars().all(|c| c.is_ascii_digit()) && port.parse::<u16>().is_ok() => {
            Some(port.to_string())
        }
        Some(_) => return None,
    };
    let bad = [
        '<', '>', '"', '{', '}', '|', '\\', '^', '`', '%', '[', ']', ':',
    ];
    let usable = if host.starts_with('[') {
        host.len() > 2
    } else {
        !host.is_empty() && !host.contains(bad)
    };
    if !usable {
        return None;
    }
    Some(Address {
        scheme,
        host: host.to_lowercase(),
        port,
        path,
        query,
        fragment,
    })
}

/// A server address as typed: trimmed, and given `https://` when it has no
/// scheme. Nothing when it still isn't an http(s) address with a host.
/// `SyncServer.url(from:)`.
#[uniffi::export]
pub fn sync_http_url(typed: String) -> Option<String> {
    parse(&typed).map(|address| address.text(true))
}

/// `typed` as an http(s) address without a query, fragment or trailing slash,
/// and without any of `dropping_suffixes` pasted on the end (a Supabase URL
/// copied from an API example often ends `/rest/v1`). Nothing when it isn't
/// an address with a host.
#[uniffi::export]
pub fn sync_normalised_url(typed: String, dropping_suffixes: Vec<String>) -> Option<String> {
    let mut address = parse(&typed)?;
    let mut path = address.path.clone();
    let mut trimming = true;
    while trimming {
        trimming = false;
        while path.ends_with('/') {
            path.pop();
        }
        for suffix in &dropping_suffixes {
            if path.to_lowercase().ends_with(&suffix.to_lowercase()) {
                path.truncate(path.len() - suffix.len());
                trimming = true;
            }
        }
    }
    address.path = path;
    Some(address.text(false))
}

/// Where the hosted server used to be: on Railway as Takt, and before that as
/// Priority. Each answers 404 to everything now.
const LEGACY_HOSTED_SERVERS: [&str; 2] = [
    "https://takt-sync.up.railway.app",
    "https://priority-sync.up.railway.app",
];

/// `server`, or `hosted` when `server` is one of the hosted server's old
/// addresses, so a device that signed in there follows it to its new one.
/// Read without a trailing slash and whatever the scheme's or host's case.
#[uniffi::export]
pub fn sync_current_server(server: String, hosted: String) -> String {
    let trimmed = server.trim();
    let normalised = sync_normalised_url(trimmed.into(), vec![]);
    let legacy = normalised.as_deref().is_some_and(|url| {
        let https = url.replacen("http://", "https://", 1);
        LEGACY_HOSTED_SERVERS.contains(&https.as_str())
    });
    if legacy { hosted } else { trimmed.into() }
}

/// The endpoints as typed under "Use a different server". A blank server is
/// `hosted`'s; a blank Supabase URL *and* key are `hosted`'s project.
/// Anything else that can't be used comes back as a problem to show.
#[uniffi::export]
pub fn sync_resolve_endpoints(
    server: String,
    supabase_url: String,
    supabase_key: String,
    hosted: SyncEndpointsRecord,
) -> SyncEndpointsResolution {
    match resolve(&server, &supabase_url, &supabase_key, hosted) {
        Ok(endpoints) => SyncEndpointsResolution {
            endpoints: Some(endpoints),
            problem: None,
        },
        Err(problem) => SyncEndpointsResolution {
            endpoints: None,
            problem: Some(problem.into()),
        },
    }
}

fn resolve(
    server: &str,
    project: &str,
    key: &str,
    hosted: SyncEndpointsRecord,
) -> Result<SyncEndpointsRecord, &'static str> {
    let (server, project, key) = (server.trim(), project.trim(), key.trim());
    let mut endpoints = hosted;
    if !server.is_empty() {
        endpoints.server_url = sync_normalised_url(server.into(), vec![])
            .ok_or("The sync server address isn't a web address.")?;
    }
    match (project.is_empty(), key.is_empty()) {
        (true, true) => {}
        (false, true) => {
            return Err("Enter the Supabase project's publishable key as well as its URL.");
        }
        (true, false) => return Err("Enter the Supabase project's URL as well as its key."),
        (false, false) => {
            endpoints.supabase_url =
                sync_normalised_url(project.into(), vec!["/auth/v1".into(), "/rest/v1".into()])
                    .ok_or("The Supabase URL isn't a web address.")?;
            if key.chars().any(char::is_whitespace) {
                return Err("The Supabase key has a space in it. Paste it again.");
            }
            // The secret key bypasses row-level security. It belongs on the
            // server (SUPABASE_SECRET_KEY), never in an app.
            if key.starts_with("sb_secret_") {
                return Err(
                    "That's the project's secret key. Use the publishable key here; the secret one is for the server.",
                );
            }
            endpoints.supabase_key = key.into();
        }
    }
    Ok(endpoints)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosted() -> SyncEndpointsRecord {
        SyncEndpointsRecord {
            server_url: "https://takt-sync.up.railway.app".into(),
            supabase_url: "https://ref.supabase.co".into(),
            supabase_key: "sb_publishable_hosted".into(),
        }
    }

    fn resolved(server: &str, project: &str, key: &str) -> SyncEndpointsResolution {
        sync_resolve_endpoints(server.into(), project.into(), key.into(), hosted())
    }

    fn endpoints(server: &str, project: &str, key: &str) -> SyncEndpointsRecord {
        resolved(server, project, key).endpoints.expect("resolves")
    }

    // SyncEndpointsTests.testBlankFieldsAreTaktsOwn, SyncEndpointsTest.blankFieldsAreTaktsOwn
    #[test]
    fn the_hosted_servers_old_addresses_follow_it() {
        let hosted = "https://api.takt.maybeitssoftware.co.uk".to_string();
        for old in [
            "https://takt-sync.up.railway.app",
            "https://takt-sync.up.railway.app/",
            "HTTPS://Priority-Sync.up.railway.app",
            "http://priority-sync.up.railway.app",
        ] {
            assert_eq!(
                sync_current_server(old.into(), hosted.clone()),
                hosted,
                "{old}"
            );
        }
        for kept in [
            "https://sync.example.com",
            "https://takt-sync.up.railway.app.example.com",
            "",
        ] {
            assert_eq!(sync_current_server(kept.into(), hosted.clone()), kept);
        }
    }

    #[test]
    fn blank_fields_are_takts_own() {
        assert_eq!(endpoints("", "", ""), hosted());
        assert_eq!(endpoints("  ", " \n", ""), hosted());
    }

    // testAServerAloneKeepsTaktsAccounts
    #[test]
    fn a_server_alone_keeps_takts_accounts() {
        let own = endpoints("sync.example.com/", "", "");
        assert_eq!(own.server_url, "https://sync.example.com");
        assert_eq!(own.supabase_url, hosted().supabase_url);
        assert_eq!(own.supabase_key, hosted().supabase_key);
    }

    // testAllThreeAreNormalised
    #[test]
    fn all_three_are_normalised() {
        let own = endpoints(
            " https://sync.example.com/takt/?x=1 ",
            "abc.supabase.co/rest/v1/",
            "  sb_publishable_own ",
        );
        assert_eq!(own.server_url, "https://sync.example.com/takt");
        assert_eq!(own.supabase_url, "https://abc.supabase.co");
        assert_eq!(own.supabase_key, "sb_publishable_own");

        let pasted = endpoints(
            "http://localhost:8080",
            "http://127.0.0.1:54321/auth/v1",
            "eyJhbGciOi.x.y",
        );
        assert_eq!(pasted.server_url, "http://localhost:8080");
        assert_eq!(pasted.supabase_url, "http://127.0.0.1:54321");

        let emulator = endpoints("http://10.0.2.2:8080", "http://10.0.2.2:54321", "k");
        assert_eq!(emulator.server_url, "http://10.0.2.2:8080");
        assert_eq!(emulator.supabase_url, "http://10.0.2.2:54321");

        // Suffixes go however many were pasted, in any case.
        assert_eq!(
            endpoints("", "https://abc.supabase.co/REST/v1/auth/v1//#x", "k").supabase_url,
            "https://abc.supabase.co"
        );
    }

    // testWhatCantBeUsedSaysWhy
    #[test]
    fn what_cant_be_used_says_why() {
        for (server, project, key, expected) in [
            ("ftp://sync.example.com", "", "", "sync server address"),
            ("", "https://abc.supabase.co", "", "publishable key"),
            ("", "", "sb_publishable_own", "URL as well"),
            ("", "ftp://abc", "sb_publishable_own", "Supabase URL isn't"),
            (
                "",
                "https://abc.supabase.co",
                "sb_secret_oops",
                "secret key",
            ),
            ("", "https://abc.supabase.co", "sb_publishable own", "space"),
        ] {
            let resolution = resolved(server, project, key);
            assert_eq!(resolution.endpoints, None);
            let problem = resolution.problem.unwrap();
            assert!(
                problem.contains(expected),
                "{problem} should mention {expected}"
            );
        }
    }

    // SyncTransportTests' SyncServer.url(from:), SyncEndpointsTest.onlyHttpAddressesWithAHost
    #[test]
    fn only_http_addresses_with_a_host() {
        let url = |typed: &str| sync_http_url(typed.into());
        assert_eq!(
            url(" sync.example.com ").as_deref(),
            Some("https://sync.example.com")
        );
        assert_eq!(
            url("http://localhost:8080").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(
            url("HTTPS://Sync.Example.com").as_deref(),
            Some("https://sync.example.com")
        );
        assert_eq!(
            url("https://ex.com/a/?q=1#f").as_deref(),
            Some("https://ex.com/a/?q=1#f")
        );
        assert_eq!(
            url("https://[::1]:80/p").as_deref(),
            Some("https://[::1]:80/p")
        );
        for refused in [
            "",
            "   ",
            "ftp://example.com",
            "httpx://example.com",
            "https://",
            "https:///path",
            "mailto:me@example.com",
            "https://user:pw@example.com",
            "https://my server.com",
            "https://example.com:port",
            "https://example.com:99999",
        ] {
            assert_eq!(url(refused), None, "{refused:?}");
        }
    }
}
