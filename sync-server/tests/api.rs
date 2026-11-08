//! End-to-end tests against a real Postgres. `#[sqlx::test]` creates a fresh
//! database per test from `DATABASE_URL` and runs the migrations into it; see
//! README.md for starting a throwaway server.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use takt_sync_server::auth::Verifier;
use takt_sync_server::devices::AccountAdmin;
use takt_sync_server::{AppState, MIGRATOR, notify, router};
use tokio::sync::watch;
use tower::ServiceExt;
use uuid::Uuid;

const ISSUER: &str = "https://test.supabase.co/auth/v1";
const SECRET: &str = "test-jwt-secret";

/// Stands in for Supabase's admin API, recording whom it deleted.
#[derive(Default)]
struct FakeAdmin(Mutex<Vec<Uuid>>);

impl AccountAdmin for FakeAdmin {
    fn delete_user(
        &self,
        user: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        self.0.lock().expect("lock").push(user);
        Box::pin(async { Ok(()) })
    }
}

struct Server {
    app: Router,
    admin: Arc<FakeAdmin>,
    users: Mutex<HashMap<String, Uuid>>,
    _stop: watch::Sender<bool>,
}

async fn server(pool: PgPool) -> Server {
    server_with(pool, takt_sync_server::billing::Billing::default()).await
}

async fn server_with(pool: PgPool, billing: takt_sync_server::billing::Billing) -> Server {
    let realtime = takt_sync_server::realtime::prepare(&pool).await;
    let changes = notify::spawn_listener(&pool).await.expect("listen");
    let (stop, shutdown) = watch::channel(false);
    let admin = Arc::new(FakeAdmin::default());
    let state = AppState {
        pool,
        verifier: Arc::new(Verifier::with_secret(ISSUER, SECRET)),
        admin: Some(admin.clone()),
        changes,
        shutdown,
        realtime,
        billing: Arc::new(billing),
    };
    Server {
        app: router(state),
        admin,
        users: Mutex::default(),
        _stop: stop,
    }
}

/// An access token as Supabase would issue it, for `user`.
fn jwt(user: Uuid, email: &str, issuer: &str, expires_in: i64) -> String {
    let claims = json!({"sub": user.to_string(), "email": email, "role": "authenticated",
        "aud": "authenticated", "iss": issuer,
        "exp": chrono::Utc::now().timestamp() + expires_in});
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("token")
}

impl Server {
    /// `token` is `<jwt>|<device id>` as the helpers below make it, or a bare
    /// jwt, or anything else to send as it is.
    async fn call(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            let (jwt, device) = token.split_once('|').unwrap_or((token, ""));
            request = request.header("authorization", format!("Bearer {jwt}"));
            if !device.is_empty() {
                request = request.header("x-priority-device", device);
            }
        }
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("request");
        let response = self.app.clone().oneshot(request).await.expect("response");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    /// Signs `email` in on a new device called `name` and registers it, as the
    /// apps do after Supabase signs them in. Returns the device id and the
    /// token to call with.
    async fn sign_up(&self, email: &str, name: &str) -> (String, String) {
        let user = *self
            .users
            .lock()
            .expect("lock")
            .entry(email.to_owned())
            .or_insert_with(Uuid::new_v4);
        let device = Uuid::new_v4();
        let token = format!("{}|{device}", jwt(user, email, ISSUER, 3600));
        let (status, body) = self
            .call(
                "POST",
                "/v1/devices",
                Some(&token),
                Some(json!({"id": device, "name": name, "platform": "macos"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        (device.to_string(), token)
    }

    /// Another device on the same account as `token`.
    async fn pair_with_code(&self, token: &str, name: &str) -> String {
        let (_, body) = self.call("GET", "/v1/account", Some(token), None).await;
        let email = body["email"].as_str().expect("email").to_owned();
        self.sign_up(&email, name).await.1
    }

    async fn push(&self, token: &str, changes: Value) -> Value {
        let (status, body) = self
            .call(
                "POST",
                "/v1/push",
                Some(token),
                Some(json!({ "changes": changes })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn changes(&self, token: &str, query: &str) -> Value {
        let (status, body) = self
            .call("GET", &format!("/v1/changes?{query}"), Some(token), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }
}

fn hlc(ms: u64, device: &str) -> String {
    format!("{ms:013}-0000-{device}")
}

fn ids(page: &Value) -> Vec<String> {
    page["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn health_answers_ok(pool: PgPool) {
    let server = server(pool).await;
    let (status, body) = server.call("GET", "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"ok": true, "realtime": false}));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn accounts_never_see_each_others_rows(pool: PgPool) {
    let server = server(pool).await;
    let (_, mine) = server.sign_up("me@example.com", "Mac").await;
    let (_, yours) = server.sign_up("you@example.com", "Mac").await;

    // The same row id in both accounts: two separate rows.
    server
        .push(
            &mine,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "a"),
                    "values": {"title": "mine"}}]),
        )
        .await;
    server
        .push(
            &yours,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(500, "b"),
                    "values": {"title": "yours"}},
                   {"table": "tasks", "id": "T2", "op": "upsert", "hlc": hlc(500, "b"),
                    "values": {"title": "also yours"}}]),
        )
        .await;

    let page = server.changes(&mine, "since=0").await;
    assert_eq!(ids(&page), vec!["T1"]);
    assert_eq!(page["rows"][0]["values"]["title"], "mine");
    let page = server.changes(&yours, "since=0").await;
    assert_eq!(ids(&page), vec!["T1", "T2"]);
    assert_eq!(
        page["rows"][0]["values"]["title"], "yours",
        "an older hlc, but not a rival"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pair_push_and_pull_round_trip(pool: PgPool) {
    let server = server(pool.clone()).await;
    let (mac_id, mac) = server.sign_up("me@example.com", "Mac").await;
    let phone = server.pair_with_code(&mac, "Phone").await;

    let pushed = server
        .push(
            &mac,
            json!([
                {"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"),
                 "values": {"title": "Buy milk", "sortOrder": 0, "dueAt": null}},
                {"table": "task_lists", "id": "L1", "op": "upsert", "hlc": hlc(1000, "mac"),
                 "values": {"name": "Inbox"}},
            ]),
        )
        .await;
    assert_eq!(pushed["accepted"], 2);
    assert_eq!(pushed["cursor"], 2);

    let page = server.changes(&phone, "since=0").await;
    assert_eq!(page["cursor"], 2);
    assert_eq!(page["hasMore"], false);
    assert_eq!(
        page["rows"][0],
        json!({"table": "tasks", "id": "T1", "deleted": false,
               "values": {"title": "Buy milk", "sortOrder": 0, "dueAt": null},
               "hlc": hlc(1000, "mac")})
    );

    // The phone edits another column; the Mac, offline, renamed it earlier.
    server
        .push(
            &phone,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(3000, "phone"),
                    "values": {"notes": "oat"}}]),
        )
        .await;
    server
        .push(
            &mac,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(2000, "mac"),
                    "values": {"title": "Buy oat milk", "notes": "stale"}}]),
        )
        .await;
    let page = server.changes(&phone, "since=2").await;
    let row = &page["rows"][page["rows"].as_array().expect("rows").len() - 1];
    assert_eq!(row["values"]["title"], "Buy oat milk");
    assert_eq!(row["values"]["notes"], "oat");
    assert_eq!(row["hlc"], hlc(3000, "phone"));

    // lastDeviceId is the latest pusher, diagnostics only.
    let last: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT last_device_id FROM rows WHERE table_name = 'tasks' AND row_id = 'T1'",
    )
    .fetch_one(&pool)
    .await
    .expect("row");
    assert_eq!(last.map(|id| id.to_string()), Some(mac_id));

    // Delete, then a stale edit, then a resurrecting one.
    server
        .push(
            &phone,
            json!([{"table": "tasks", "id": "T1", "op": "delete", "hlc": hlc(4000, "phone")}]),
        )
        .await;
    let page = server.changes(&mac, "since=4").await;
    assert_eq!(page["rows"][0]["deleted"], true);
    assert_eq!(page["rows"][0]["hlc"], hlc(4000, "phone"));
    let cursor = page["cursor"].as_i64().expect("cursor");

    let stale = server
        .push(
            &mac,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(3500, "mac"),
                    "values": {"title": "too late"}}]),
        )
        .await;
    assert_eq!(stale["cursor"], cursor, "a losing change writes nothing");
    server
        .push(
            &mac,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(5000, "mac"),
                    "values": {"title": "undeleted"}}]),
        )
        .await;
    let page = server.changes(&phone, &format!("since={cursor}")).await;
    assert_eq!(page["rows"][0]["deleted"], false);
    assert_eq!(page["rows"][0]["values"]["title"], "undeleted");
    assert_eq!(page["rows"][0]["values"]["notes"], "oat");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_caller_receives_its_own_rows(pool: PgPool) {
    let server = server(pool).await;
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    server
        .push(
            &mac,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"),
                    "values": {"title": "mine"}}]),
        )
        .await;
    let page = server.changes(&mac, "since=0").await;
    assert_eq!(ids(&page), vec!["T1"]);
    assert_eq!(page["rows"][0]["values"]["title"], "mine");
    assert_eq!(page["cursor"], 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pages_resume_from_the_cursor_without_gaps(pool: PgPool) {
    let server = server(pool).await;
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    let phone = server.pair_with_code(&mac, "Phone").await;
    let changes: Vec<Value> = (0..5)
        .map(|n| {
            json!({"table": "tasks", "id": format!("T{n}"), "op": "upsert",
                   "hlc": hlc(1000 + n, "mac"), "values": {"n": n}})
        })
        .collect();
    server.push(&mac, Value::Array(changes)).await;

    let mut seen = Vec::new();
    let mut cursor = 0;
    loop {
        let page = server
            .changes(&phone, &format!("since={cursor}&limit=2"))
            .await;
        seen.extend(ids(&page));
        cursor = page["cursor"].as_i64().expect("cursor");
        if page["hasMore"] == false {
            break;
        }
    }
    assert_eq!(seen, vec!["T0", "T1", "T2", "T3", "T4"]);
    assert_eq!(cursor, 5);

    // An empty page keeps the cursor where it was.
    let page = server.changes(&phone, &format!("since={cursor}")).await;
    assert_eq!(page["rows"], json!([]));
    assert_eq!(page["cursor"], cursor);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn re_pushing_a_batch_changes_nothing(pool: PgPool) {
    let server = server(pool).await;
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    let batch = json!([
        {"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"), "values": {"title": "a"}},
        {"table": "tasks", "id": "T2", "op": "delete", "hlc": hlc(1000, "mac")},
    ]);
    let first = server.push(&mac, batch.clone()).await;
    let second = server.push(&mac, batch).await;
    assert_eq!(first["cursor"], 2);
    assert_eq!(second["cursor"], 2);
    assert_eq!(second["accepted"], 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn malformed_changes_are_refused_whole(pool: PgPool) {
    let server = server(pool).await;
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    let (status, _) = server
        .call(
            "POST",
            "/v1/push",
            Some(&mac),
            Some(json!({"changes": [
                {"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"), "values": {"a": 1}},
                {"table": "tasks", "id": "T2", "op": "upsert", "hlc": "yesterday", "values": {"a": 1}},
            ]})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = server
        .call(
            "POST",
            "/v1/push",
            Some(&mac),
            Some(json!({"changes": [
                {"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"), "values": {"a": [1]}},
            ]})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let page = server.changes(&mac, "since=0").await;
    assert_eq!(
        page["rows"],
        json!([]),
        "nothing from a refused batch landed"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_long_poll_wakes_when_another_device_pushes(pool: PgPool) {
    let server = Arc::new(server(pool).await);
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    let phone = server.pair_with_code(&mac, "Phone").await;

    let started = Instant::now();
    let waiter = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.changes(&phone, "since=0&wait=20").await })
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!waiter.is_finished(), "nothing to send yet, so it waits");
    server
        .push(
            &mac,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"),
                    "values": {"title": "ping"}}]),
        )
        .await;
    let page = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("woken well before its wait ran out")
        .expect("task");
    assert_eq!(ids(&page), vec!["T1"]);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_idle_long_poll_returns_empty_when_its_wait_runs_out(pool: PgPool) {
    let server = server(pool).await;
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    let started = Instant::now();
    let page = server.changes(&mac, "since=0&wait=1").await;
    assert!(started.elapsed() >= Duration::from_millis(900));
    assert_eq!(page["rows"], json!([]));
    assert_eq!(page["cursor"], 0);
    assert_eq!(page["hasMore"], false);
}

/// The token in a reset email's link.

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_device_registers_only_itself(pool: PgPool) {
    // The victim's device, on their account.
    let server = server(pool).await;
    let (victim_device, victim) = server.sign_up("victim@example.com", "Mac").await;
    let (_, attacker) = server.sign_up("attacker@example.com", "Mac").await;

    // Naming someone else's device id from your own device is refused...
    let (status, _) = server
        .call(
            "POST",
            "/v1/devices",
            Some(&attacker),
            Some(json!({"id": victim_device, "name": "Mine now", "platform": "macos"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // ...as is registering with no device header to compare against.
    let bare = attacker.split_once('|').expect("jwt|device").0;
    let (status, _) = server
        .call(
            "POST",
            "/v1/devices",
            Some(bare),
            Some(json!({"id": victim_device, "name": "Mine now", "platform": "macos"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // And the device stayed where it was.
    let (_, body) = server.call("GET", "/v1/account", Some(&victim), None).await;
    let devices = body["devices"].as_array().expect("devices");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["name"], "Mac");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_account_lists_its_devices_and_signing_out_drops_one(pool: PgPool) {
    let server = server(pool).await;
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    let phone = server.pair_with_code(&mac, "Phone").await;

    let (status, body) = server.call("GET", "/v1/account", Some(&mac), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["email"], "me@example.com");
    let devices = body["devices"].as_array().expect("devices");
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0]["name"], "Mac");
    assert_eq!(devices[0]["current"], true);
    assert_eq!(devices[1]["current"], false);

    let (status, _) = server
        .call("POST", "/v1/sign-out", Some(&phone), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = server.call("GET", "/v1/account", Some(&mac), None).await;
    assert_eq!(body["devices"].as_array().expect("devices").len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn deleting_an_account_takes_its_rows_devices_and_user(pool: PgPool) {
    let server = server(pool.clone()).await;
    let (_, mine) = server.sign_up("me@example.com", "Mac").await;
    let (_, yours) = server.sign_up("you@example.com", "Mac").await;
    for token in [&mine, &yours] {
        server
            .push(
                token,
                json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "a"),
                        "values": {"title": "x"}}]),
            )
            .await;
    }
    let me = server.users.lock().expect("lock")["me@example.com"];

    let (status, _) = server
        .call("POST", "/v1/account/delete", Some(&mine), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(*server.admin.0.lock().expect("lock"), vec![me]);

    let count = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .expect("count")
        }
    };
    assert_eq!(
        count("SELECT COUNT(*) FROM rows").await,
        1,
        "only the other account's row"
    );
    assert_eq!(count("SELECT COUNT(*) FROM devices").await, 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_route_but_health_needs_a_valid_token(pool: PgPool) {
    let server = server(pool).await;
    let user = Uuid::new_v4();
    let device = Uuid::new_v4();
    let expired = format!("{}|{device}", jwt(user, "me@example.com", ISSUER, -600));
    let foreign = format!(
        "{}|{device}",
        jwt(
            user,
            "me@example.com",
            "https://other.supabase.co/auth/v1",
            600
        )
    );
    for token in [
        None,
        Some("nobody"),
        Some(expired.as_str()),
        Some(foreign.as_str()),
    ] {
        for (method, uri) in [
            ("GET", "/v1/changes"),
            ("POST", "/v1/push"),
            ("GET", "/v1/account"),
            ("POST", "/v1/devices"),
        ] {
            let (status, _) = server
                .call(method, uri, token, Some(json!({"changes": []})))
                .await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {uri} with {token:?}"
            );
        }
    }

    // A valid token without the device header can read but not write.
    let bare = jwt(user, "me@example.com", ISSUER, 600);
    let (status, _) = server.call("GET", "/v1/changes", Some(&bare), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = server
        .call(
            "POST",
            "/v1/push",
            Some(&bare),
            Some(json!({"changes": []})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Gives a plain Postgres the corner of Supabase's `realtime` schema the
/// server touches: `realtime.send` records what it was asked to broadcast.
async fn fake_supabase_realtime(pool: &PgPool) {
    sqlx::raw_sql(
        r#"
        DO $$ BEGIN CREATE ROLE authenticated NOLOGIN;
        EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$;
        CREATE SCHEMA realtime;
        CREATE SCHEMA auth;
        CREATE TABLE realtime.messages (
          topic TEXT, event TEXT, payload JSONB, private BOOLEAN, extension TEXT);
        ALTER TABLE realtime.messages ENABLE ROW LEVEL SECURITY;
        CREATE FUNCTION realtime.topic() RETURNS TEXT LANGUAGE sql STABLE
          AS $f$ SELECT current_setting('realtime.topic', true) $f$;
        CREATE FUNCTION auth.uid() RETURNS UUID LANGUAGE sql STABLE
          AS $f$ SELECT NULL::uuid $f$;
        CREATE FUNCTION realtime.send(payload JSONB, event TEXT, topic TEXT, private BOOLEAN DEFAULT true)
          RETURNS void LANGUAGE sql
          AS $f$ INSERT INTO realtime.messages VALUES (topic, event, payload, private, 'broadcast') $f$;
        "#,
    )
    .execute(pool)
    .await
    .expect("fake realtime");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_realtime_a_push_broadcasts_nothing_and_health_says_long_poll(pool: PgPool) {
    let server = server(pool).await;
    let (_, body) = server.call("GET", "/health", None, None).await;
    assert_eq!(body["realtime"], false);
    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    server
        .push(
            &mac,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"), "values": {"title": "a"}}]),
        )
        .await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_push_that_writes_broadcasts_on_the_accounts_channel(pool: PgPool) {
    fake_supabase_realtime(&pool).await;
    let server = server(pool.clone()).await;

    let (_, body) = server.call("GET", "/health", None, None).await;
    assert_eq!(
        body["realtime"], true,
        "the policy was made and health says so"
    );
    let policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_policies WHERE schemaname = 'realtime' AND policyname = $1",
    )
    .bind(takt_sync_server::realtime::POLICY)
    .fetch_one(&pool)
    .await
    .expect("policies");
    assert_eq!(policies, 1);

    let (_, mac) = server.sign_up("me@example.com", "Mac").await;
    let account = server.users.lock().expect("lock")["me@example.com"];
    let batch = json!([
        {"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "mac"), "values": {"title": "a"}},
    ]);
    let pushed = server.push(&mac, batch.clone()).await;
    // Nothing new: no row written, so nobody is woken.
    server.push(&mac, batch).await;

    let sent: Vec<(String, String, Value, bool)> =
        sqlx::query_as("SELECT topic, event, payload, private FROM realtime.messages")
            .fetch_all(&pool)
            .await
            .expect("messages");
    assert_eq!(
        sent,
        vec![(
            format!("sync:{account}"),
            "changed".to_owned(),
            json!({"cursor": pushed["cursor"]}),
            true
        )]
    );

    // A second boot finds the policy rather than making another.
    let again = takt_sync_server::realtime::prepare(&pool).await;
    assert!(again);
}

// Shared lists (docs/shared-lists-plan.md).

impl Server {
    async fn send(&self, method: &str, uri: &str, token: &str, body: Option<Value>) -> Value {
        let (status, body) = self.call(method, uri, Some(token), body).await;
        assert_eq!(status, StatusCode::OK, "{method} {uri}: {body}");
        body
    }

    /// Shares list `list` from `owner` and has `member` join it by invite.
    async fn share_with(&self, owner: &str, member: &str, list: &str) -> String {
        let space = self
            .send(
                "POST",
                "/v1/spaces",
                owner,
                Some(json!({"listId": list, "name": "Groceries"})),
            )
            .await;
        let space_id = space["id"].as_str().expect("id").to_owned();
        let invite = self
            .send(
                "POST",
                &format!("/v1/spaces/{space_id}/invites"),
                owner,
                None,
            )
            .await;
        let code = invite["code"].as_str().expect("code");
        let joined = self
            .send("POST", &format!("/v1/invites/{code}/accept"), member, None)
            .await;
        assert_eq!(joined["id"], space_id.as_str());
        assert_eq!(joined["role"], "member");
        space_id
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_shared_list_reaches_its_members_and_nothing_else_does(pool: PgPool) {
    let server = server(pool).await;
    let (_, owner) = server.sign_up("owner@example.com", "Mac").await;
    let (_, member) = server.sign_up("member@example.com", "Phone").await;

    // The owner's private task, and the list's rows, pushed before sharing.
    let space = server.share_with(&owner, &member, "L1").await;
    server
        .push(
            &owner,
            json!([
                {"table": "tasks", "id": "PRIVATE", "op": "upsert", "hlc": hlc(1000, "o"),
                 "values": {"title": "mine alone"}},
                {"table": "task_lists", "id": "L1", "op": "upsert", "hlc": hlc(1000, "o"),
                 "values": {"name": "Groceries"}, "space": space},
                {"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "o"),
                 "values": {"title": "Milk", "listId": "L1"}, "space": space},
            ]),
        )
        .await;

    let page = server.changes(&member, "since=0").await;
    assert_eq!(
        ids(&page),
        vec!["L1", "T1"],
        "the private task stays private"
    );
    assert_eq!(page["rows"][1]["space"], space.as_str());
    assert_eq!(page["spaces"][0]["listId"], "L1");
    assert_eq!(page["spaces"][0]["role"], "member");
    let members = page["spaces"][0]["members"].as_array().expect("members");
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["email"], "owner@example.com");
    assert_eq!(members[0]["role"], "owner");
    assert_eq!(members[1]["you"], true);

    // The member's edit reaches the owner, tagged with the space.
    let cursor = server.changes(&owner, "since=0").await["cursor"].clone();
    server
        .push(
            &member,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(2000, "m"),
                    "values": {"title": "Oat milk"}, "space": space}]),
        )
        .await;
    let page = server.changes(&owner, &format!("since={cursor}")).await;
    assert_eq!(ids(&page), vec!["T1"]);
    assert_eq!(page["rows"][0]["values"]["title"], "Oat milk");
    assert_eq!(page["rows"][0]["values"]["listId"], "L1");
    assert_eq!(page["rows"][0]["space"], space.as_str());

    // The owner's own rows carry no space.
    let page = server.changes(&owner, "since=0").await;
    let private = page["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["id"] == "PRIVATE")
        .expect("private row");
    assert!(private.get("space").is_none());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn joining_late_still_pulls_what_was_there(pool: PgPool) {
    let server = server(pool).await;
    let (_, owner) = server.sign_up("owner@example.com", "Mac").await;
    let (_, member) = server.sign_up("member@example.com", "Phone").await;
    let space = server
        .send("POST", "/v1/spaces", &owner, Some(json!({"listId": "L1"})))
        .await["id"]
        .as_str()
        .expect("id")
        .to_owned();
    server
        .push(
            &owner,
            json!([{"table": "task_lists", "id": "L1", "op": "upsert", "hlc": hlc(1000, "o"),
                    "values": {"name": "Groceries"}, "space": space}]),
        )
        .await;
    // The member has synced their own rows well past the list's.
    for n in 0..3 {
        server
            .push(
                &member,
                json!([{"table": "tasks", "id": format!("M{n}"), "op": "upsert",
                        "hlc": hlc(1000 + n, "m"), "values": {"title": "theirs"}}]),
            )
            .await;
    }
    let cursor = server.changes(&member, "since=0").await["cursor"]
        .as_i64()
        .expect("cursor");

    let invite = server
        .send("POST", &format!("/v1/spaces/{space}/invites"), &owner, None)
        .await;
    let code = invite["code"].as_str().expect("code");
    // Typed in lower case, with a dash: still the code.
    let typed = format!("{}-{}", &code[..13].to_lowercase(), &code[13..]);
    server
        .send(
            "POST",
            &format!("/v1/invites/{typed}/accept"),
            &member,
            None,
        )
        .await;
    let page = server.changes(&member, &format!("since={cursor}")).await;
    assert_eq!(ids(&page), vec!["L1"], "the list comes after the cursor");

    // The code is spent.
    let (_, other) = server.sign_up("other@example.com", "Mac").await;
    let (status, _) = server
        .call(
            "POST",
            &format!("/v1/invites/{code}/accept"),
            Some(&other),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::GONE);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn what_a_shared_space_refuses(pool: PgPool) {
    let server = server(pool).await;
    let (_, owner) = server.sign_up("owner@example.com", "Mac").await;
    let (_, member) = server.sign_up("member@example.com", "Phone").await;
    let (_, stranger) = server.sign_up("stranger@example.com", "Mac").await;
    let space = server.share_with(&owner, &member, "L1").await;

    let refused = |token: String, change: Value| {
        let server = &server;
        async move {
            let (status, _) = server
                .call(
                    "POST",
                    "/v1/push",
                    Some(&token),
                    Some(json!({"changes": [change]})),
                )
                .await;
            status
        }
    };
    let task = json!({"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1, "x"),
                      "values": {"title": "x"}, "space": space});
    // A stranger's change is skipped, not refused: a device just removed from
    // a list mustn't be stuck retrying it.
    assert_eq!(
        refused(stranger.clone(), task.clone()).await,
        StatusCode::OK
    );
    assert!(
        ids(&server.changes(&owner, "since=0").await).is_empty(),
        "and nothing was written"
    );
    assert_eq!(
        refused(
            member.clone(),
            json!({"table": "dailies", "id": "D1", "op": "upsert", "hlc": hlc(1, "x"),
                   "values": {}, "space": space})
        )
        .await,
        StatusCode::BAD_REQUEST,
        "habits stay personal"
    );
    assert_eq!(
        refused(
            member.clone(),
            json!({"table": "task_lists", "id": "OTHER", "op": "upsert", "hlc": hlc(1, "x"),
                   "values": {}, "space": space})
        )
        .await,
        StatusCode::BAD_REQUEST,
        "only the shared list's own row"
    );
    assert_eq!(
        refused(
            member.clone(),
            json!({"table": "task_lists", "id": "L1", "op": "delete", "hlc": hlc(1, "x"),
                   "space": space})
        )
        .await,
        StatusCode::OK,
        "a member leaves rather than deletes: skipped"
    );
    // Only the owner invites, and a member can't share the list as theirs.
    let (status, _) = server
        .call(
            "POST",
            &format!("/v1/spaces/{space}/invites"),
            Some(&member),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = server
        .call(
            "POST",
            "/v1/spaces",
            Some(&member),
            Some(json!({"listId": "L1"})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(refused(owner.clone(), task).await, StatusCode::OK);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn leaving_removal_and_stopping_sharing(pool: PgPool) {
    let server = server(pool).await;
    let (_, owner) = server.sign_up("owner@example.com", "Mac").await;
    let (_, member) = server.sign_up("member@example.com", "Phone").await;
    let space = server.share_with(&owner, &member, "L1").await;
    let spaces = |page: &Value| page["spaces"].as_array().expect("spaces").len();

    // The owner can't leave their own list.
    let owner_id = server.send("GET", "/v1/account", &owner, None).await["accountId"]
        .as_str()
        .expect("id")
        .to_owned();
    let (status, _) = server
        .call(
            "DELETE",
            &format!("/v1/spaces/{space}/members/{owner_id}"),
            Some(&member),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A member leaves.
    let member_id = server.send("GET", "/v1/account", &member, None).await["accountId"]
        .as_str()
        .expect("id")
        .to_owned();
    server
        .send(
            "DELETE",
            &format!("/v1/spaces/{space}/members/{member_id}"),
            &member,
            None,
        )
        .await;
    assert_eq!(spaces(&server.changes(&member, "since=0").await), 0);
    let (status, _) = server
        .call(
            "POST",
            "/v1/push",
            Some(&member),
            Some(
                json!({"changes": [{"table": "tasks", "id": "T1", "op": "upsert",
                "hlc": hlc(1, "m"), "values": {}, "space": space}]}),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a leaver's change is skipped");
    assert!(ids(&server.changes(&owner, "since=0").await).is_empty());

    // Back in by a new invite, then the owner stops sharing.
    let invite = server
        .send("POST", &format!("/v1/spaces/{space}/invites"), &owner, None)
        .await;
    server
        .send(
            "POST",
            &format!("/v1/invites/{}/accept", invite["code"].as_str().unwrap()),
            &member,
            None,
        )
        .await;
    assert_eq!(spaces(&server.changes(&member, "since=0").await), 1);
    server
        .send("DELETE", &format!("/v1/spaces/{space}"), &owner, None)
        .await;
    assert_eq!(spaces(&server.changes(&member, "since=0").await), 0);
    assert_eq!(spaces(&server.changes(&owner, "since=0").await), 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn deleting_the_owner_hands_the_list_on(pool: PgPool) {
    let server = server(pool).await;
    let (_, owner) = server.sign_up("owner@example.com", "Mac").await;
    let (_, member) = server.sign_up("member@example.com", "Phone").await;
    let space = server.share_with(&owner, &member, "L1").await;
    server
        .push(
            &owner,
            json!([{"table": "tasks", "id": "T1", "op": "upsert", "hlc": hlc(1000, "o"),
                    "values": {"title": "Milk"}, "space": space}]),
        )
        .await;
    server
        .send("POST", "/v1/account/delete", &owner, None)
        .await;
    let page = server.changes(&member, "since=0").await;
    assert_eq!(
        ids(&page),
        vec!["T1"],
        "the shared rows outlive their writer"
    );
    assert_eq!(page["spaces"][0]["role"], "owner");
    assert_eq!(
        page["spaces"][0]["members"]
            .as_array()
            .expect("members")
            .len(),
        1
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_join_link_opens_a_page(pool: PgPool) {
    let server = server(pool).await;
    let request = Request::builder()
        .uri("/join/abc%3Cscript%3E123")
        .body(Body::empty())
        .expect("request");
    let response = server.app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let page = String::from_utf8_lossy(&bytes);
    assert!(page.contains("ABCSCRIPT123"));
    assert!(!page.contains("<script>"));
}

// Takt Sync, the hosted subscription (billing.rs).

const WEBHOOK_SECRET: &str = "rc-test-secret";

fn charging() -> takt_sync_server::billing::Billing {
    takt_sync_server::billing::Billing {
        required: true,
        comp: Default::default(),
        webhook_secret: Some(WEBHOOK_SECRET.into()),
    }
}

impl Server {
    async fn account_id(&self, token: &str) -> String {
        let (_, body) = self.call("GET", "/v1/account", Some(token), None).await;
        body["accountId"].as_str().expect("account id").to_owned()
    }

    async fn webhook(&self, secret: &str, event: Value) -> StatusCode {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/webhooks/revenuecat")
            .header("authorization", secret)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "api_version": "1.0", "event": event }).to_string(),
            ))
            .expect("request");
        self.app
            .clone()
            .oneshot(request)
            .await
            .expect("response")
            .status()
    }
}

fn event(id: &str, kind: &str, account: &str, expires_in_days: i64) -> Value {
    let expires = chrono::Utc::now().timestamp_millis() + expires_in_days * 86_400_000;
    json!({ "id": id, "type": kind, "app_user_id": account, "product_id": "takt_sync_yearly",
            "entitlement_ids": ["sync"], "store": "APP_STORE", "expiration_at_ms": expires })
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_free_server_never_asks(pool: PgPool) {
    let server = server(pool).await;
    let (_, token) = server.sign_up("me@example.com", "Mac").await;
    server.changes(&token, "since=0").await;
    let (_, account) = server.call("GET", "/v1/account", Some(&token), None).await;
    assert_eq!(account["subscription"]["required"], json!(false));
    assert_eq!(account["subscription"]["active"], json!(true));
    // And the webhook isn't there without a secret.
    assert_eq!(
        server
            .webhook("anything", event("e", "RENEWAL", "x", 1))
            .await,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_charging_server_lets_in_only_the_subscribed(pool: PgPool) {
    let server = server_with(pool, charging()).await;
    let (_, token) = server.sign_up("me@example.com", "Phone").await;
    let me = server.account_id(&token).await;

    // Signed in, but not paid: data is refused with a 402 the apps can read,
    // the account page still answers.
    let (status, body) = server
        .call("GET", "/v1/changes?since=0", Some(&token), None)
        .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
    assert_eq!(body["subscription"], json!("required"));
    let (status, _) = server
        .call(
            "POST",
            "/v1/spaces",
            Some(&token),
            Some(json!({"listId": "L"})),
        )
        .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
    let (status, account) = server.call("GET", "/v1/account", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(account["subscription"]["required"], json!(true));
    assert_eq!(account["subscription"]["active"], json!(false));

    // The webhook wants its secret.
    assert_eq!(
        server
            .webhook("wrong", event("e1", "INITIAL_PURCHASE", &me, 365))
            .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .webhook(WEBHOOK_SECRET, event("e1", "INITIAL_PURCHASE", &me, 365))
            .await,
        StatusCode::OK
    );
    server.changes(&token, "since=0").await;

    // Cancelling keeps the paid year; a late, older expiry changes nothing;
    // the real expiry ends it; the same event twice is applied once.
    assert_eq!(
        server
            .webhook(WEBHOOK_SECRET, event("e2", "CANCELLATION", &me, 365))
            .await,
        StatusCode::OK
    );
    server.changes(&token, "since=0").await;
    assert_eq!(
        server
            .webhook(WEBHOOK_SECRET, event("e3", "EXPIRATION", &me, -30))
            .await,
        StatusCode::OK
    );
    server.changes(&token, "since=0").await;
    assert_eq!(
        server
            .webhook(WEBHOOK_SECRET, event("e4", "EXPIRATION", &me, 365))
            .await,
        StatusCode::OK
    );
    let (status, _) = server
        .call("GET", "/v1/changes?since=0", Some(&token), None)
        .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
    assert_eq!(
        server
            .webhook(WEBHOOK_SECRET, event("e1", "INITIAL_PURCHASE", &me, 365))
            .await,
        StatusCode::OK
    );
    let (status, _) = server
        .call("GET", "/v1/changes?since=0", Some(&token), None)
        .await;
    assert_eq!(
        status,
        StatusCode::PAYMENT_REQUIRED,
        "a replayed event is ignored"
    );

    // Renewed: back in.
    assert_eq!(
        server
            .webhook(WEBHOOK_SECRET, event("e5", "RENEWAL", &me, 365))
            .await,
        StatusCode::OK
    );
    server.changes(&token, "since=0").await;
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_restore_moves_the_purchase_and_comps_are_left_alone(pool: PgPool) {
    let mut billing = charging();
    let server = server_with(pool.clone(), charging()).await;
    let (_, old) = server.sign_up("old@example.com", "Phone").await;
    let (_, new) = server.sign_up("new@example.com", "Phone").await;
    let (old_id, new_id) = (server.account_id(&old).await, server.account_id(&new).await);
    server
        .webhook(WEBHOOK_SECRET, event("p", "INITIAL_PURCHASE", &old_id, 365))
        .await;
    server.changes(&old, "since=0").await;
    let transfer = json!({ "id": "t", "type": "TRANSFER",
        "transferred_from": [old_id], "transferred_to": [new_id] });
    assert_eq!(
        server.webhook(WEBHOOK_SECRET, transfer).await,
        StatusCode::OK
    );
    server.changes(&new, "since=0").await;
    let (status, _) = server
        .call("GET", "/v1/changes?since=0", Some(&old), None)
        .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);

    // COMP_ACCOUNTS lets an account in with nothing in the table, and a comp
    // row outlives an expiry event.
    billing.comp.insert(Uuid::parse_str(&old_id).unwrap());
    let comped_server = server_with(pool.clone(), billing).await;
    comped_server.changes(&old, "since=0").await;
    takt_sync_server::billing::grant_comp(&pool, Uuid::parse_str(&new_id).unwrap())
        .await
        .expect("comp");
    server
        .webhook(WEBHOOK_SECRET, event("x", "EXPIRATION", &new_id, 400))
        .await;
    server.changes(&new, "since=0").await;
    let (_, account) = server.call("GET", "/v1/account", Some(&new), None).await;
    assert_eq!(account["subscription"]["comp"], json!(true));
}
