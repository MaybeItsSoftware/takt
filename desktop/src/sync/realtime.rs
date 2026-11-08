//! Waiting for other devices' news on the account's Supabase Realtime
//! channel (`docs/sync.md`, "News on Supabase Realtime").
//!
//! The sync server runs on Cloud Run, which bills every second a request is
//! open, so holding a long-poll open while nothing happens costs money. This
//! holds a websocket to Supabase instead: the server broadcasts `changed` on
//! `sync:<account>` after every push that writes, and this only says so; the
//! cycle does the pulling, exactly as it did for the long-poll.
//!
//! Supabase Realtime speaks the Phoenix channel protocol: JSON frames of
//! `{topic, event, payload, ref, join_ref}`, a `phx_join` answered by a
//! `phx_reply`, and a heartbeat on the `phoenix` topic every so often or the
//! socket is closed. The pure half (the frames) is tested below; [`listen`]
//! is the blocking loop around it.

use serde_json::{Value, json};
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// Supabase closes a socket that has been quiet for a minute or so.
const HEARTBEAT: Duration = Duration::from_secs(25);
/// How long a read blocks before the loop looks at the time again.
const TICK: Duration = Duration::from_secs(1);
/// A join not answered by then is a failure.
const JOIN_TIMEOUT: Duration = Duration::from_secs(15);
/// How often the token is looked at, so a refreshed one reaches the channel
/// before the old one runs out.
const TOKEN_CHECK: Duration = Duration::from_secs(30);

/// The websocket address of a Supabase project.
pub fn socket_url(supabase_url: &str, key: &str) -> String {
    let base = supabase_url
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    format!(
        "{base}/realtime/v1/websocket?apikey={}&vsn=1.0.0",
        super::http::escape(key)
    )
}

/// The channel an account's devices join.
pub fn channel(account: &str) -> String {
    format!("realtime:sync:{account}")
}

fn join(channel: &str, token: &str, reference: &str) -> String {
    json!({
        "topic": channel,
        "event": "phx_join",
        "payload": {
            "config": {
                "broadcast": { "self": false, "ack": false },
                "presence": { "key": "" },
                "postgres_changes": [],
                "private": true,
            },
            "access_token": token,
        },
        "ref": reference,
        "join_ref": reference,
    })
    .to_string()
}

fn heartbeat(reference: &str) -> String {
    json!({ "topic": "phoenix", "event": "heartbeat", "payload": {}, "ref": reference }).to_string()
}

fn access_token(channel: &str, token: &str, reference: &str, join_ref: &str) -> String {
    json!({
        "topic": channel,
        "event": "access_token",
        "payload": { "access_token": token },
        "ref": reference,
        "join_ref": join_ref,
    })
    .to_string()
}

/// What a frame from Supabase means to sync.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming {
    /// The join was accepted: from now on nothing is missed.
    Joined,
    /// The join, or the channel, was refused or closed, with why.
    Ended(String),
    /// Another device (or this one) pushed: time to pull.
    News,
    /// Heartbeat replies, presence, anything else.
    Other,
}

pub fn classify(text: &str, channel: &str, join_ref: &str) -> Incoming {
    let Ok(frame) = serde_json::from_str::<Value>(text) else {
        return Incoming::Other;
    };
    if frame["topic"].as_str() != Some(channel) {
        return Incoming::Other;
    }
    let payload = &frame["payload"];
    match frame["event"].as_str().unwrap_or_default() {
        "phx_reply" if frame["ref"].as_str() == Some(join_ref) => {
            if payload["status"].as_str() == Some("ok") {
                Incoming::Joined
            } else {
                Incoming::Ended(reason(payload))
            }
        }
        "broadcast" => Incoming::News,
        "phx_close" => Incoming::Ended("the channel closed".into()),
        "phx_error" => Incoming::Ended("the channel failed".into()),
        "system" if payload["status"].as_str() == Some("error") => Incoming::Ended(reason(payload)),
        _ => Incoming::Other,
    }
}

fn reason(payload: &Value) -> String {
    payload["response"]["reason"]
        .as_str()
        .or_else(|| payload["message"].as_str())
        .unwrap_or("refused")
        .to_string()
}

/// Why [`listen`] stopped.
pub enum Stop {
    /// `live` said to.
    Asked,
    /// The token was refused for good: the session is over.
    Unauthorized,
    /// Anything else; try again later. Whether it had joined first.
    Failed { joined: bool, why: String },
}

/// Holds the channel open until `live` says stop or it breaks, calling
/// `news` once on joining and on every broadcast. `token` gives a token
/// good for now, refreshing first if it must.
pub fn listen(
    url: &str,
    channel: &str,
    mut token: impl FnMut() -> Result<String, Stop>,
    mut news: impl FnMut(),
    live: impl Fn() -> bool,
) -> Stop {
    let failed = |joined: bool, why: String| Stop::Failed { joined, why };
    let mut socket = match tungstenite::connect(url) {
        Ok((socket, _)) => socket,
        Err(error) => return failed(false, error.to_string()),
    };
    if let Err(error) = set_read_timeout(&socket, TICK) {
        return failed(false, error.to_string());
    }

    let mut next_ref = 1u64;
    let mut reference = || {
        next_ref += 1;
        next_ref.to_string()
    };
    let join_ref = reference();
    let mut sent_token = match token() {
        Ok(token) => token,
        Err(stop) => return stop,
    };
    if let Err(error) = socket.send(Message::text(join(channel, &sent_token, &join_ref))) {
        return failed(false, error.to_string());
    }

    let started = Instant::now();
    let mut joined = false;
    let mut last_heartbeat = Instant::now();
    let mut last_token_check = Instant::now();
    loop {
        if !live() {
            let _ = socket.close(None);
            return Stop::Asked;
        }
        if !joined && started.elapsed() > JOIN_TIMEOUT {
            return failed(false, "no answer to joining the channel".into());
        }
        if last_heartbeat.elapsed() >= HEARTBEAT {
            last_heartbeat = Instant::now();
            if let Err(error) = socket.send(Message::text(heartbeat(&reference()))) {
                return failed(joined, error.to_string());
            }
        }
        if joined && last_token_check.elapsed() >= TOKEN_CHECK {
            last_token_check = Instant::now();
            let current = match token() {
                Ok(token) => token,
                Err(stop) => return stop,
            };
            if current != sent_token {
                let frame = access_token(channel, &current, &reference(), &join_ref);
                if let Err(error) = socket.send(Message::text(frame)) {
                    return failed(joined, error.to_string());
                }
                sent_token = current;
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => match classify(&text, channel, &join_ref) {
                Incoming::Joined => {
                    joined = true;
                    // Whatever was pushed while nobody listened.
                    news();
                }
                Incoming::News => news(),
                Incoming::Ended(why) => return failed(joined, why),
                Incoming::Other => {}
            },
            Ok(Message::Close(_)) => return failed(joined, "Supabase closed the socket".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => return failed(joined, error.to_string()),
        }
    }
}

fn set_read_timeout(
    socket: &WebSocket<MaybeTlsStream<TcpStream>>,
    timeout: Duration,
) -> std::io::Result<()> {
    match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(timeout)),
        MaybeTlsStream::Rustls(stream) => stream.get_ref().set_read_timeout(Some(timeout)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANNEL: &str = "realtime:sync:6a1f";

    #[test]
    fn the_socket_and_the_channel_are_named_as_supabase_wants() {
        assert_eq!(
            socket_url("https://abc.supabase.co/", "sb_publishable_x"),
            "wss://abc.supabase.co/realtime/v1/websocket?apikey=sb_publishable_x&vsn=1.0.0"
        );
        assert_eq!(channel("6a1f"), CHANNEL);
    }

    #[test]
    fn joining_asks_for_the_private_channel_with_the_token() {
        let frame: Value = serde_json::from_str(&join(CHANNEL, "jwt", "2")).unwrap();
        assert_eq!(frame["event"], "phx_join");
        assert_eq!(frame["topic"], CHANNEL);
        assert_eq!(frame["payload"]["config"]["private"], true);
        assert_eq!(frame["payload"]["access_token"], "jwt");
        assert_eq!(frame["join_ref"], "2");
    }

    #[test]
    fn replies_broadcasts_and_closes_are_told_apart() {
        let reply = |status: &str, reference: &str| {
            json!({"topic": CHANNEL, "event": "phx_reply", "ref": reference,
                   "payload": {"status": status, "response": {"reason": "Unauthorized"}}})
            .to_string()
        };
        assert_eq!(classify(&reply("ok", "2"), CHANNEL, "2"), Incoming::Joined);
        assert_eq!(
            classify(&reply("error", "2"), CHANNEL, "2"),
            Incoming::Ended("Unauthorized".into())
        );
        // A heartbeat's reply is not the join's.
        assert_eq!(classify(&reply("ok", "7"), CHANNEL, "2"), Incoming::Other);

        let broadcast = json!({"topic": CHANNEL, "event": "broadcast", "ref": null,
            "payload": {"type": "broadcast", "event": "changed", "payload": {"cursor": 9}}});
        assert_eq!(
            classify(&broadcast.to_string(), CHANNEL, "2"),
            Incoming::News
        );
        let elsewhere =
            json!({"topic": "realtime:sync:other", "event": "broadcast", "payload": {}});
        assert_eq!(
            classify(&elsewhere.to_string(), CHANNEL, "2"),
            Incoming::Other
        );

        let expired = json!({"topic": CHANNEL, "event": "system",
            "payload": {"status": "error", "message": "Token has expired"}});
        assert_eq!(
            classify(&expired.to_string(), CHANNEL, "2"),
            Incoming::Ended("Token has expired".into())
        );
        let closed = json!({"topic": CHANNEL, "event": "phx_close", "payload": {}});
        assert!(matches!(
            classify(&closed.to_string(), CHANNEL, "2"),
            Incoming::Ended(_)
        ));
        assert_eq!(classify("not json", CHANNEL, "2"), Incoming::Other);
    }
}

/// Against the hosted Supabase project: `cargo test -- --ignored realtime`.
/// With no session it can only be refused, which is what proves the socket,
/// TLS and the join's framing all work.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore = "needs the network"]
    fn the_hosted_project_answers_a_join() {
        let url = socket_url(super::super::SUPABASE_URL, super::super::SUPABASE_KEY);
        let stop = listen(
            &url,
            &channel("00000000-0000-0000-0000-000000000000"),
            || Ok("not-a-session".into()),
            || panic!("nothing should be heard without a session"),
            || true,
        );
        match stop {
            Stop::Failed { joined: false, why } => {
                eprintln!("refused: {why}");
                assert!(
                    !why.contains("connect") && !why.contains("no answer"),
                    "{why}"
                );
            }
            _ => panic!("expected the join to be refused"),
        }
    }
}
