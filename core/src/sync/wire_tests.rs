//! The sync cycle over its bodies, against a server that merges with the real
//! server's rule (`takt_sync_rules::merge`). Ported from the Mac's
//! `SyncEngineTests` and Android's `SyncEngineTest`, which drive the same
//! calls through their engines.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde_json::{Value, json};
use takt_sync_rules::merge::{self, Change, Op, Outcome, StoredRow};
use takt_sync_rules::wire::{ChangedRow, ChangesResponse, PushRequest, WireOp};
use uuid::Uuid;

use super::wire::*;
use super::*;
use crate::schema::migrate;

const T: &str = "2024-01-01 00:00:00.000";

/// The server's feed in memory: rows merged per column, each change given the
/// next sequence number.
#[derive(Default)]
struct Server {
    rows: BTreeMap<(String, String), (StoredRow, i64)>,
    seq: i64,
    pushes: Vec<Value>,
}

impl Server {
    fn push(&mut self, body: &str, device: Uuid) {
        self.pushes.push(serde_json::from_str(body).unwrap());
        let request: PushRequest = serde_json::from_str(body).unwrap();
        for change in request.changes {
            let key = (change.table.clone(), change.id.clone());
            let merged = Change {
                table: change.table,
                id: change.id,
                op: match change.op {
                    WireOp::Upsert => Op::Upsert(change.values.unwrap_or_default()),
                    WireOp::Delete => Op::Delete,
                },
                hlc: change.hlc,
            };
            let stored = self.rows.get(&key).map(|(row, _)| row);
            if let Outcome::Changed(row) = merge::merge(stored, &merged, device) {
                self.seq += 1;
                self.rows.insert(key, (row, self.seq));
            }
        }
    }

    fn changes(&self, since: i64, limit: usize) -> String {
        let mut feed: Vec<_> = self
            .rows
            .iter()
            .filter(|(_, (_, seq))| *seq > since)
            .collect();
        feed.sort_by_key(|(_, (_, seq))| *seq);
        let has_more = feed.len() > limit;
        feed.truncate(limit);
        let cursor = feed.last().map_or(since, |(_, (_, seq))| *seq);
        let rows = feed
            .into_iter()
            .map(|((table, id), (row, _))| ChangedRow {
                table: table.clone(),
                id: id.clone(),
                deleted: row.deleted,
                values: row.data.clone(),
                hlc: merge::row_hlc(&row.col_hlc, row.deleted_hlc.as_deref()),
                space: None,
            })
            .collect();
        serde_json::to_string(&ChangesResponse {
            rows,
            cursor,
            has_more,
            spaces: None,
        })
        .unwrap()
    }
}

struct Device {
    connection: Connection,
    uuid: Uuid,
}

impl Device {
    fn new(name: &str) -> Device {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON")
            .unwrap();
        migrate(&mut connection).unwrap();
        let uuid = Uuid::new_v4();
        let tx = connection.transaction().unwrap();
        begin(&tx, &format!("{name}-{uuid}"), "https://sync.example.com").unwrap();
        tx.commit().unwrap();
        Device { connection, uuid }
    }

    fn with_workspace(name: &str) -> Device {
        let device = Device::new(name);
        device.run(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{T}', '{T}');
             INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt, systemRole)
               VALUES ('inbox', 'w', 'Inbox', 0, '{T}', '{T}', 'inbox');
             INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('a', 'inbox', 'Alpha', 0, '{T}', '{T}');"
        ));
        device
    }

    fn run(&self, sql: &str) {
        self.connection.execute_batch(sql).unwrap();
    }

    fn title(&self, id: &str) -> Option<String> {
        self.connection
            .query_row("SELECT title FROM tasks WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .unwrap()
    }

    /// One cycle as the engines run it: the first after pairing snapshots
    /// and pulls first; every other pushes first.
    fn sync(&mut self, server: &mut Server, wall_ms: i64) -> (u32, SyncPullOutcome) {
        let first = state(&self.connection).unwrap().unwrap().needs_snapshot;
        if first {
            let tx = self.connection.transaction().unwrap();
            enqueue_snapshot(&tx, wall_ms).unwrap();
            tx.commit().unwrap();
            let pulled = self.pull(server, wall_ms);
            (self.push(server, wall_ms), pulled)
        } else {
            let pushed = self.push(server, wall_ms);
            (pushed, self.pull(server, wall_ms))
        }
    }

    fn push(&mut self, server: &mut Server, wall_ms: i64) -> u32 {
        let mut pushed = 0;
        while let Some(batch) = prepare_push(&self.connection, 2, wall_ms).unwrap() {
            server.push(&batch.body, self.uuid);
            let tx = self.connection.transaction().unwrap();
            finish_push(&tx, batch.through_seq, &batch.hlc, wall_ms).unwrap();
            tx.commit().unwrap();
            pushed += batch.count;
        }
        pushed
    }

    fn pull(&mut self, server: &Server, wall_ms: i64) -> SyncPullOutcome {
        let pull = SyncPull::new();
        let mut cursor = state(&self.connection).unwrap().unwrap().cursor;
        loop {
            let page = pull.add_page(server.changes(cursor, 2)).unwrap();
            cursor = page.cursor;
            if !page.has_more {
                break;
            }
        }
        let tx = self.connection.transaction().unwrap();
        let outcome = apply_pull(&tx, &pull, wall_ms, wall_ms).unwrap();
        tx.commit().unwrap();
        outcome
    }
}

#[test]
fn a_device_joining_adopts_the_account_through_paged_bodies() {
    let mut server = Server::default();
    let mut mac = Device::with_workspace("mac");
    let (pushed, _) = mac.sync(&mut server, 1_000);
    assert_eq!(pushed, 3, "the workspace, its Inbox and its task");

    let mut phone = Device::new("phone");
    let (pushed, pulled) = phone.sync(&mut server, 2_000);
    assert_eq!(pushed, 0);
    assert_eq!(pulled.pulled, 3, "three rows over two pages");
    assert!(pulled.changed);
    assert_eq!(phone.title("a").as_deref(), Some("Alpha"));
    assert_eq!(state(&phone.connection).unwrap().unwrap().cursor, 3);
}

#[test]
fn edits_to_different_columns_both_survive_and_a_delete_travels() {
    let mut server = Server::default();
    let mut mac = Device::with_workspace("mac");
    mac.sync(&mut server, 1_000);
    let mut phone = Device::new("phone");
    phone.sync(&mut server, 1_000);

    mac.run("UPDATE tasks SET title = 'From the Mac' WHERE id = 'a'");
    phone.run("UPDATE tasks SET notes = 'From the phone' WHERE id = 'a'");
    mac.sync(&mut server, 2_000);
    phone.sync(&mut server, 3_000);
    mac.sync(&mut server, 4_000);
    for device in [&mac, &phone] {
        let (title, notes): (String, Option<String>) = device
            .connection
            .query_row("SELECT title, notes FROM tasks WHERE id = 'a'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(title, "From the Mac");
        assert_eq!(notes.as_deref(), Some("From the phone"));
    }

    phone.run("DELETE FROM tasks WHERE id = 'a'");
    phone.sync(&mut server, 5_000);
    mac.sync(&mut server, 6_000);
    assert_eq!(mac.title("a"), None);
}

#[test]
fn a_push_body_is_the_wire_format_stamped_in_edit_order() {
    let mut server = Server::default();
    let mut mac = Device::with_workspace("mac");
    mac.sync(&mut server, 1_000);
    let device_id = state(&mac.connection).unwrap().unwrap().device_id;

    mac.run(
        "UPDATE tasks SET title = 'Second' WHERE id = 'a';
         UPDATE sync_outbox SET changedAtMs = 9000 WHERE rowId = 'a';
         INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
           VALUES ('b', 'inbox', 'Beta', 1, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000');
         UPDATE sync_outbox SET changedAtMs = 5000 WHERE rowId = 'b';",
    );
    let batch = prepare_push(&mac.connection, 500, 2_000).unwrap().unwrap();
    let body: Value = serde_json::from_str(&batch.body).unwrap();
    let changes = body["changes"].as_array().unwrap();
    assert_eq!(batch.count, 2);
    // 'b' was edited first, so it is stamped first, whatever the outbox order.
    assert_eq!(changes[0]["id"], "b");
    assert_eq!(changes[0]["op"], "upsert");
    assert_eq!(changes[1]["id"], "a");
    assert_eq!(
        changes[1]["values"],
        json!({ "id": "a", "title": "Second" }),
        "only the changed columns and the key"
    );
    let first = changes[0]["hlc"].as_str().unwrap();
    let second = changes[1]["hlc"].as_str().unwrap();
    assert!(first < second);
    assert!(first.starts_with("0000000005000-"));
    assert!(first.ends_with(&device_id));
    assert_eq!(batch.hlc, second);
    // Nothing is acknowledged until the server has it.
    assert_eq!(
        prepare_push(&mac.connection, 500, 2_000).unwrap(),
        Some(batch.clone())
    );

    mac.run("DELETE FROM tasks WHERE id = 'b'");
    let batch = prepare_push(&mac.connection, 500, 2_000).unwrap().unwrap();
    let body: Value = serde_json::from_str(&batch.body).unwrap();
    let delete = body["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|change| change["id"] == "b")
        .unwrap();
    assert_eq!(delete["op"], "delete");
    assert!(
        delete.get("values").is_none(),
        "a delete leaves its values out"
    );

    let tx = mac.connection.transaction().unwrap();
    finish_push(&tx, batch.through_seq, &batch.hlc, 3_000).unwrap();
    tx.commit().unwrap();
    assert_eq!(prepare_push(&mac.connection, 500, 3_000).unwrap(), None);
    assert_eq!(
        state(&mac.connection).unwrap().unwrap().hlc.as_deref(),
        Some(batch.hlc.as_str())
    );
}

#[test]
fn a_pull_moves_the_clock_past_every_stamp_it_brings() {
    let mut server = Server::default();
    let mut mac = Device::with_workspace("mac");
    mac.sync(&mut server, 50_000);
    let mut phone = Device::new("phone");
    phone.sync(&mut server, 1_000);
    let clock = state(&phone.connection).unwrap().unwrap().hlc.unwrap();
    assert!(clock.as_str() > "0000000050000", "{clock}");

    // The phone's next edit sorts after the Mac's, though its wall clock is
    // far behind.
    phone.run("UPDATE tasks SET title = 'Later' WHERE id = 'a'");
    let batch = prepare_push(&phone.connection, 500, 1_000)
        .unwrap()
        .unwrap();
    assert!(batch.hlc > clock);
}

#[test]
fn a_page_that_is_not_the_wire_format_is_refused_and_unpaired_has_nothing_to_push() {
    assert!(SyncPull::new().add_page("{\"rows\": 1}".into()).is_err());
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    assert!(prepare_push(&connection, 500, 0).is_err());

    let pull = SyncPull::new();
    let page = pull
        .add_page(r#"{"rows":[],"cursor":7,"hasMore":false,"later":"ignored"}"#.into())
        .unwrap();
    assert_eq!(
        page,
        SyncPullPage {
            cursor: 7,
            has_more: false
        }
    );
    assert_eq!(pull.row_count(), 0);
}

#[test]
fn values_read_as_the_apps_read_them() {
    let read = |json: &str| sync_value(&serde_json::from_str(json).unwrap());
    assert_eq!(read("null"), SyncValue::Null);
    assert_eq!(read("1"), SyncValue::Integer { value: 1 });
    // Foundation's decoder reads any whole number as an integer.
    assert_eq!(read("1.0"), SyncValue::Integer { value: 1 });
    assert_eq!(read("1e3"), SyncValue::Integer { value: 1000 });
    assert_eq!(read("1.5"), SyncValue::Real { value: 1.5 });
    assert_eq!(
        read("9223372036854775808"),
        SyncValue::Real {
            value: 9_223_372_036_854_775_808.0
        }
    );
    assert_eq!(read("true"), SyncValue::Integer { value: 1 });
    assert_eq!(read("\"x\""), SyncValue::Text { value: "x".into() });
    assert_eq!(
        read("[1]"),
        SyncValue::Text {
            value: "[1]".into()
        }
    );

    let write = |value: SyncValue| json_value(&value).to_string();
    assert_eq!(write(SyncValue::Real { value: 1.0 }), "1");
    assert_eq!(write(SyncValue::Real { value: 0.1 }), "0.1");
    assert_eq!(
        write(SyncValue::Real {
            value: f64::INFINITY
        }),
        "null"
    );
    assert_eq!(write(SyncValue::Integer { value: -4 }), "-4");
}

// SyncTransportTests' error sentences, SyncSchedulerTest's backoff.
#[test]
fn refusals_read_as_sentences_and_failures_back_off() {
    assert_eq!(
        sync_failure_message(400, r#"{"error":"bad hlc"}"#.into()),
        "Bad hlc."
    );
    assert_eq!(
        sync_failure_message(503, "<html>".into()),
        "The sync server answered 503."
    );
    assert_eq!(
        sync_failure_message(500, r#"{"error":" "}"#.into()),
        "The sync server answered 500."
    );
    assert_eq!(sync_sentence("  done! ".into()), "Done!");
    assert_eq!(sync_sentence("élan".into()), "Élan.");
    assert_eq!(sync_sentence("".into()), "");
    assert_eq!(
        (0..=10).map(sync_backoff_seconds).collect::<Vec<_>>(),
        vec![1, 2, 4, 8, 16, 32, 64, 128, 256, 256, 256]
    );
}

#[test]
fn the_account_and_its_timestamps_read_as_the_server_writes_them() {
    let account = sync_decode_account(
        r#"{"accountId":"acc","email":null,"devices":[
            {"id":"d1","name":"Mac","platform":"macos","createdAt":"2024-05-01T10:00:00.123456789Z","lastSeenAt":null,"current":true},
            {"id":"d2","name":null,"platform":null,"createdAt":"2024-05-01T10:00:00Z","lastSeenAt":"2024-05-02T10:00:00+01:00","current":false,"extra":1}]}"#
            .into(),
    )
    .unwrap();
    assert_eq!(account.account_id, "acc");
    assert_eq!(account.email, None);
    assert_eq!(account.devices.len(), 2);
    assert!(account.devices[0].current);
    assert_eq!(
        sync_timestamp_ms(account.devices[0].created_at.clone()),
        Some(1_714_557_600_123)
    );
    assert_eq!(
        sync_timestamp_ms("2024-05-02T10:00:00+01:00".into()),
        Some(1_714_640_400_000)
    );
    assert_eq!(sync_timestamp_ms("yesterday".into()), None);
    assert_eq!(
        sync_register_device_body("d".into(), "Adam's Mac".into(), "macos".into()),
        r#"{"id":"d","name":"Adam's Mac","platform":"macos"}"#
    );
    assert!(sync_health_is_ok(r#"{"ok":true}"#.into()));
    assert!(!sync_health_is_ok(r#"{"ok":"yes"}"#.into()));
    assert!(sync_body_is_json_object(r#"{"external":{}}"#.into()));
    assert!(!sync_body_is_json_object("[]".into()));
}

impl Device {
    /// Saves what holds now, as the apps do, with the edit made at `at_ms`.
    fn say(
        &mut self,
        workspace: &str,
        context: crate::focus_context::StoredFocusContext,
        at_ms: i64,
    ) {
        let tx = self.connection.transaction().unwrap();
        crate::focus_context::save(&tx, workspace, &context, at_ms).unwrap();
        tx.execute(
            "UPDATE sync_outbox SET changedAtMs = ?1 WHERE tableName = 'focus_contexts'",
            [at_ms],
        )
        .unwrap();
        tx.commit().unwrap();
    }

    fn context(&self, workspace: &str) -> Option<crate::focus_context::StoredFocusContext> {
        crate::focus_context::load(&self.connection, workspace).unwrap()
    }
}

fn holding(ids: &[&str]) -> crate::focus_context::StoredFocusContext {
    crate::focus_context::StoredFocusContext {
        condition_ids: ids.iter().map(|id| id.to_string()).collect(),
        ..Default::default()
    }
}

/// What holds now is one answer per account: the last device to say it
/// wins the whole row, the time window and mode included, and the stamp of
/// when it was said travels with it so every device can call it stale.
#[test]
fn what_holds_now_follows_the_last_device_to_say_it() {
    let mut server = Server::default();
    let mut mac = Device::with_workspace("mac");
    mac.sync(&mut server, 1_000);
    let mut phone = Device::new("phone");
    phone.sync(&mut server, 1_000);

    let mut at_campus = holding(&["campus"]);
    at_campus.available_until_ms = Some(60_000);
    mac.say("w", at_campus, 10_000);
    let mut at_home = holding(&["home"]);
    at_home.mode = "finish".into();
    phone.say("w", at_home, 20_000);
    mac.sync(&mut server, 30_000);
    phone.sync(&mut server, 31_000);
    mac.sync(&mut server, 32_000);
    for device in [&mac, &phone] {
        let context = device.context("w").unwrap();
        assert_eq!(context.condition_ids, vec!["home"]);
        assert_eq!(context.available_until_ms, None, "the whole row, not a mix");
        assert_eq!(context.mode, "finish");
        assert_eq!(context.conditions_set_at_ms, Some(20_000));
    }

    // And back: the Mac says it next, and the phone follows.
    mac.say("w", holding(&["campus", "mat"]), 40_000);
    mac.sync(&mut server, 41_000);
    phone.sync(&mut server, 42_000);
    let context = phone.context("w").unwrap();
    assert_eq!(context.condition_ids, vec!["campus", "mat"]);
    assert_eq!(context.conditions_set_at_ms, Some(40_000));
    assert_eq!(context.mode, "progress");
}

/// A device that kept its own workspace before it paired brings its context
/// into the account's when it is newer, in the account's conditions.
#[test]
fn a_joining_device_brings_a_newer_context_into_the_accounts_workspace() {
    let condition = |id: &str, workspace: &str| {
        format!(
            "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category)
               VALUES ('{id}', '{workspace}', 'Campus', 1, 0, '{T}', '{T}', 'Location');"
        )
    };
    let mut server = Server::default();
    let mut mac = Device::with_workspace("mac");
    mac.run(&condition("mac-campus", "w"));
    mac.say("w", holding(&[]), 1_000);
    mac.run("UPDATE focus_contexts SET updatedAt = '2024-01-01 00:00:00.000'");
    mac.sync(&mut server, 2_000);

    let mut phone = Device::new("phone");
    phone.run(&format!(
        "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('p', 'Phone', '2024-02-01 00:00:00.000', '{T}');
         {}",
        condition("phone-campus", "p")
    ));
    phone.say("p", holding(&["phone-campus"]), 3_000);
    phone.run("UPDATE focus_contexts SET updatedAt = '2024-03-01 00:00:00.000'");
    phone.sync(&mut server, 4_000);
    assert_eq!(phone.context("p"), None);
    assert_eq!(
        phone.context("w").unwrap().condition_ids,
        vec!["mac-campus"],
        "the phone's Campus is the account's"
    );
    mac.sync(&mut server, 5_000);
    assert_eq!(mac.context("w").unwrap().condition_ids, vec!["mac-campus"]);
}

/// What a condition provides travels with the condition, and what is
/// switched off for now with the rest of what holds: the phone works out
/// the same effective conditions as the Mac that said them.
#[test]
fn what_a_condition_provides_and_what_is_off_follow_the_account() {
    let mut server = Server::default();
    let mut mac = Device::with_workspace("mac");
    mac.run(&format!(
        "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category)
           VALUES ('home', 'w', 'Home', 1, 0, '{T}', '{T}', 'Location'),
                  ('quiet', 'w', 'Quiet', 0, 0, '{T}', '{T}', NULL),
                  ('floor', 'w', 'Floor space', 0, 0, '{T}', '{T}', NULL);"
    ));
    mac.sync(&mut server, 1_000);
    let mut phone = Device::new("phone");
    phone.sync(&mut server, 2_000);

    let tx = mac.connection.transaction().unwrap();
    crate::conditions::set_condition_provides(
        &tx,
        "home",
        vec!["quiet".into(), "floor".into()],
        3_000,
    )
    .unwrap();
    tx.commit().unwrap();
    let mut housemates_in = holding(&["home"]);
    housemates_in.off_ids = vec!["quiet".into()];
    mac.say("w", housemates_in, 4_000);
    mac.sync(&mut server, 5_000);
    phone.sync(&mut server, 6_000);

    let conditions = crate::rows::conditions(&phone.connection, "w").unwrap();
    let home = conditions.iter().find(|c| c.id == "home").unwrap();
    assert_eq!(home.provides, vec!["floor", "quiet"]);
    let context = phone.context("w").unwrap();
    assert_eq!(context.off_ids, vec!["quiet"]);
    assert_eq!(
        crate::conditions::effective_ids(&conditions, &context.condition_ids, &context.off_ids),
        vec!["floor", "home"]
    );
}
