//! Shared lists over the sync cycle, against a server that keeps spaces and
//! members the way `sync-server/src/spaces.rs` does and merges with the real
//! rule. Two accounts, each with its own devices.

use std::collections::BTreeMap;

use rusqlite::Connection;
use takt_sync_rules::merge::{self, Change, Op, Outcome, StoredRow};
use takt_sync_rules::wire::{
    ChangedRow, ChangesResponse, PushRequest, WireMember, WireOp, WireSpace,
};
use uuid::Uuid;

use super::spaces::*;
use super::wire::*;
use super::*;
use crate::schema::migrate;

const T: &str = "2024-01-01 00:00:00.000";

struct Space {
    list_id: String,
    /// (account, role), oldest first.
    members: Vec<(String, String)>,
}

#[derive(Default)]
struct Server {
    /// (space, table, id) → (row, seq). A personal space's id is its account's.
    rows: BTreeMap<(String, String, String), (StoredRow, i64)>,
    spaces: BTreeMap<String, Space>,
    seq: i64,
}

impl Server {
    fn push(&mut self, account: &str, body: &str, device: Uuid) {
        let request: PushRequest = serde_json::from_str(body).unwrap();
        for change in request.changes {
            let space = match change.space {
                Some(space) => {
                    let Some(shared) = self.spaces.get(&space) else {
                        continue;
                    };
                    if !shared.members.iter().any(|(who, _)| who == account) {
                        continue;
                    }
                    assert!(
                        SHARED_TABLES.contains(&change.table.as_str()),
                        "{} pushed to a shared space",
                        change.table
                    );
                    if let Some(values) = &change.values {
                        for column in values.keys() {
                            assert!(
                                !is_personal_column(&change.table, column),
                                "{}.{column} pushed to a shared space",
                                change.table
                            );
                        }
                    }
                    space
                }
                None => account.to_string(),
            };
            let key = (space, change.table.clone(), change.id.clone());
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

    fn readable(&self, account: &str) -> Vec<String> {
        let mut spaces = vec![account.to_string()];
        spaces.extend(
            self.spaces
                .iter()
                .filter(|(_, space)| space.members.iter().any(|(who, _)| who == account))
                .map(|(id, _)| id.clone()),
        );
        spaces
    }

    fn wire_spaces(&self, account: &str) -> Vec<WireSpace> {
        self.spaces
            .iter()
            .filter_map(|(id, space)| {
                let role = space
                    .members
                    .iter()
                    .find(|(who, _)| who == account)?
                    .1
                    .clone();
                Some(WireSpace {
                    id: id.clone(),
                    list_id: space.list_id.clone(),
                    name: "Shared".into(),
                    role,
                    members: space
                        .members
                        .iter()
                        .map(|(who, role)| WireMember {
                            account_id: who.clone(),
                            email: Some(format!("{who}@example.com")),
                            role: role.clone(),
                            you: who == account,
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn changes(&self, account: &str, since: i64, limit: usize) -> String {
        let readable = self.readable(account);
        let mut feed: Vec<_> = self
            .rows
            .iter()
            .filter(|((space, _, _), (_, seq))| *seq > since && readable.contains(space))
            .collect();
        feed.sort_by_key(|(_, (_, seq))| *seq);
        let has_more = feed.len() > limit;
        feed.truncate(limit);
        let cursor = feed.last().map_or(since, |(_, (_, seq))| *seq);
        let rows = feed
            .into_iter()
            .map(|((space, table, id), (row, _))| ChangedRow {
                table: table.clone(),
                id: id.clone(),
                deleted: row.deleted,
                values: row.data.clone(),
                hlc: merge::row_hlc(&row.col_hlc, row.deleted_hlc.as_deref()),
                space: (space != account).then(|| space.clone()),
            })
            .collect();
        serde_json::to_string(&ChangesResponse {
            rows,
            cursor,
            has_more,
            spaces: Some(self.wire_spaces(account)),
        })
        .unwrap()
    }

    /// `POST /v1/spaces`: the answer body the owner's device records.
    fn share(&mut self, owner: &str, list_id: &str) -> String {
        let id = Uuid::new_v4().to_string();
        self.spaces.insert(
            id.clone(),
            Space {
                list_id: list_id.into(),
                members: vec![(owner.into(), "owner".into())],
            },
        );
        self.space_body(&id, owner)
    }

    /// Accepting an invite: the member joins, and the space's rows are
    /// numbered again so they reach the member past any cursor.
    fn join(&mut self, space: &str, member: &str) -> String {
        self.spaces
            .get_mut(space)
            .unwrap()
            .members
            .push((member.into(), "member".into()));
        let keys: Vec<_> = self
            .rows
            .keys()
            .filter(|(id, _, _)| id == space)
            .cloned()
            .collect();
        for key in keys {
            self.seq += 1;
            self.rows.get_mut(&key).unwrap().1 = self.seq;
        }
        self.space_body(space, member)
    }

    fn remove(&mut self, space: &str, member: &str) {
        self.spaces
            .get_mut(space)
            .unwrap()
            .members
            .retain(|(who, _)| who != member);
    }

    fn stop_sharing(&mut self, space: &str) {
        self.spaces.remove(space);
        self.rows.retain(|(id, _, _), _| id != space);
    }

    fn space_body(&self, space: &str, account: &str) -> String {
        let wire = self
            .wire_spaces(account)
            .into_iter()
            .find(|wire| wire.id == space)
            .unwrap();
        serde_json::to_string(&wire).unwrap()
    }
}

struct Device {
    account: String,
    connection: Connection,
    uuid: Uuid,
}

impl Device {
    /// A device of `account` with a workspace of its own, `w-<account>`, and
    /// an Inbox.
    fn new(account: &str) -> Device {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON")
            .unwrap();
        migrate(&mut connection).unwrap();
        let uuid = Uuid::new_v4();
        let tx = connection.transaction().unwrap();
        begin(
            &tx,
            &format!("{account}-{uuid}"),
            "https://sync.example.com",
        )
        .unwrap();
        tx.commit().unwrap();
        let device = Device {
            account: account.into(),
            connection,
            uuid,
        };
        device.run(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w-{account}', 'Mine', '{T}', '{T}');
             INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt, systemRole)
               VALUES ('inbox-{account}', 'w-{account}', 'Inbox', 0, '{T}', '{T}', 'inbox');"
        ));
        device
    }

    /// A second device of the same account, with nothing of its own yet.
    fn another(account: &str) -> Device {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON")
            .unwrap();
        migrate(&mut connection).unwrap();
        let uuid = Uuid::new_v4();
        let tx = connection.transaction().unwrap();
        begin(
            &tx,
            &format!("{account}-{uuid}"),
            "https://sync.example.com",
        )
        .unwrap();
        tx.commit().unwrap();
        Device {
            account: account.into(),
            connection,
            uuid,
        }
    }

    fn run(&self, sql: &str) {
        self.connection.execute_batch(sql).unwrap();
    }

    fn text(&self, sql: &str) -> Option<String> {
        self.connection
            .query_row(sql, [], |row| row.get(0))
            .optional()
            .unwrap()
            .flatten()
    }

    fn count(&self, sql: &str) -> i64 {
        self.connection
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    }

    fn sync(&mut self, server: &mut Server) {
        let wall_ms = 1_000;
        let first = state(&self.connection).unwrap().unwrap().needs_snapshot;
        if first {
            let tx = self.connection.transaction().unwrap();
            enqueue_snapshot(&tx, wall_ms).unwrap();
            tx.commit().unwrap();
            self.pull(server);
            self.push(server);
        } else {
            self.push(server);
            self.pull(server);
        }
    }

    fn push(&mut self, server: &mut Server) {
        let mut rounds = 0;
        while let Some(batch) = prepare_push(&self.connection, 3, 2_000).unwrap() {
            server.push(&self.account, &batch.body, self.uuid);
            let tx = self.connection.transaction().unwrap();
            finish_push(&tx, batch.through_seq, &batch.hlc, 2_000).unwrap();
            tx.commit().unwrap();
            rounds += 1;
            assert!(rounds < 100, "the push never ran dry");
        }
    }

    fn pull(&mut self, server: &Server) {
        let pull = SyncPull::new();
        let mut cursor = state(&self.connection).unwrap().unwrap().cursor;
        loop {
            let page = pull
                .add_page(server.changes(&self.account, cursor, 3))
                .unwrap();
            cursor = page.cursor;
            if !page.has_more {
                break;
            }
        }
        let tx = self.connection.transaction().unwrap();
        apply_pull(&tx, &pull, 3_000, 3_000).unwrap();
        tx.commit().unwrap();
    }

    fn record(&mut self, body: &str) {
        let space = spaces::decode_space(body).unwrap();
        let tx = self.connection.transaction().unwrap();
        record_space(&tx, &space).unwrap();
        tx.commit().unwrap();
    }

    fn forget(&mut self, space: &str, stopped_sharing: bool) {
        let tx = self.connection.transaction().unwrap();
        forget(&tx, space, !stopped_sharing, stopped_sharing).unwrap();
        tx.commit().unwrap();
    }

    fn title(&self, id: &str) -> Option<String> {
        self.text(&format!("SELECT title FROM tasks WHERE id = '{id}'"))
    }
}

/// The owner's Mac with a Groceries list of two tasks (one a subtask), one
/// with metadata, already synced to the owner's own space.
fn owner_with_groceries(server: &mut Server) -> Device {
    let mut mac = Device::new("owner");
    mac.run(&format!(
        "INSERT INTO list_folders (id, workspaceId, name, sortOrder, createdAt, updatedAt)
           VALUES ('home', 'w-owner', 'Home', 0, '{T}', '{T}');
         INSERT INTO task_lists (id, workspaceId, folderId, name, sortOrder, createdAt, updatedAt)
           VALUES ('groceries', 'w-owner', 'home', 'Groceries', 3, '{T}', '{T}');
         INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
           VALUES ('milk', 'groceries', 'Milk', 0, '{T}', '{T}');
         INSERT INTO tasks (id, listId, parentTaskId, title, sortOrder, createdAt, updatedAt)
           VALUES ('oat', 'groceries', 'milk', 'Oat milk', 0, '{T}', '{T}');
         INSERT INTO task_metadata (taskId, priority, kanbanColumn, updatedAt)
           VALUES ('milk', 2, 'doing', '{T}');
         INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
           VALUES ('private', 'inbox-owner', 'Private', 0, '{T}', '{T}');"
    ));
    mac.sync(server);
    mac
}

#[test]
fn a_shared_list_reaches_its_member_and_nothing_else_does() {
    let mut server = Server::default();
    let mut mac = owner_with_groceries(&mut server);

    let body = server.share("owner", "groceries");
    let space = spaces::decode_space(&body).unwrap().id;
    mac.record(&body);
    mac.sync(&mut server);

    let mut phone = Device::new("member");
    phone.sync(&mut server);
    assert_eq!(
        phone.count("SELECT COUNT(*) FROM tasks"),
        0,
        "nothing before joining"
    );
    let joined = server.join(&space, "member");
    phone.record(&joined);
    phone.sync(&mut server);

    // The list and its tasks arrive, in the member's own workspace and at
    // the root of their sidebar; the owner's private task and folder don't.
    assert_eq!(phone.title("milk").as_deref(), Some("Milk"));
    assert_eq!(phone.title("oat").as_deref(), Some("Oat milk"));
    assert_eq!(phone.title("private"), None);
    assert_eq!(
        phone
            .text("SELECT workspaceId FROM task_lists WHERE id = 'groceries'")
            .as_deref(),
        Some("w-member")
    );
    assert_eq!(
        phone.text("SELECT folderId FROM task_lists WHERE id = 'groceries'"),
        None
    );
    // Shared metadata comes; the owner's board column doesn't.
    assert_eq!(
        phone.count("SELECT priority FROM task_metadata WHERE taskId = 'milk'"),
        2
    );
    assert_eq!(
        phone.text("SELECT kanbanColumn FROM task_metadata WHERE taskId = 'milk'"),
        None
    );
    let shared = shared_list(&phone.connection, "groceries")
        .unwrap()
        .unwrap();
    assert_eq!(shared.role, "member");
    assert_eq!(shared.members.len(), 2);

    // The member's edits reach the owner; their own column choice doesn't,
    // and the owner's stays.
    phone.run(&format!(
        "UPDATE tasks SET title = 'Whole milk' WHERE id = 'milk';
         UPDATE task_metadata SET kanbanColumn = 'later' WHERE taskId = 'milk';
         INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
           VALUES ('eggs', 'groceries', 'Eggs', 1, '{T}', '{T}');"
    ));
    phone.sync(&mut server);
    mac.sync(&mut server);
    assert_eq!(mac.title("milk").as_deref(), Some("Whole milk"));
    assert_eq!(mac.title("eggs").as_deref(), Some("Eggs"));
    assert_eq!(
        mac.text("SELECT kanbanColumn FROM task_metadata WHERE taskId = 'milk'")
            .as_deref(),
        Some("doing")
    );
    assert_eq!(
        mac.text("SELECT folderId FROM task_lists WHERE id = 'groceries'")
            .as_deref(),
        Some("home"),
        "the member's sidebar is theirs"
    );

    // The owner's other devices see both sides: the shared rows, and the
    // owner's own column, through the companion row.
    mac.run("UPDATE task_metadata SET kanbanColumn = 'done' WHERE taskId = 'milk'");
    mac.sync(&mut server);
    let mut ipad = Device::another("owner");
    ipad.sync(&mut server);
    assert_eq!(ipad.title("milk").as_deref(), Some("Whole milk"));
    assert_eq!(ipad.title("eggs").as_deref(), Some("Eggs"));
    assert_eq!(ipad.title("private").as_deref(), Some("Private"));
    assert_eq!(
        ipad.text("SELECT kanbanColumn FROM task_metadata WHERE taskId = 'milk'")
            .as_deref(),
        Some("done")
    );
    assert_eq!(
        ipad.text("SELECT folderId FROM task_lists WHERE id = 'groceries'")
            .as_deref(),
        Some("home")
    );
    phone.sync(&mut server);
    assert_eq!(
        phone
            .text("SELECT kanbanColumn FROM task_metadata WHERE taskId = 'milk'")
            .as_deref(),
        Some("later")
    );
}

#[test]
fn moving_a_task_out_takes_it_from_the_members_only() {
    let mut server = Server::default();
    let mut mac = owner_with_groceries(&mut server);
    let body = server.share("owner", "groceries");
    let space = spaces::decode_space(&body).unwrap().id;
    mac.record(&body);
    mac.sync(&mut server);
    let mut ipad = Device::another("owner");
    ipad.sync(&mut server);
    let mut phone = Device::new("member");
    phone.sync(&mut server);
    phone.record(&server.join(&space, "member"));
    phone.sync(&mut server);
    assert_eq!(phone.title("milk").as_deref(), Some("Milk"));

    // Into the Inbox, subtask and all.
    mac.run(
        "UPDATE tasks SET listId = 'inbox-owner' WHERE id IN ('milk', 'oat');
         UPDATE tasks SET title = 'Milk, later' WHERE id = 'milk';",
    );
    mac.sync(&mut server);
    phone.sync(&mut server);
    ipad.sync(&mut server);
    assert_eq!(phone.title("milk"), None, "gone from the member");
    assert_eq!(phone.title("oat"), None);
    assert_eq!(ipad.title("milk").as_deref(), Some("Milk, later"));
    assert_eq!(
        ipad.text("SELECT listId FROM tasks WHERE id = 'oat'")
            .as_deref(),
        Some("inbox-owner")
    );
    assert_eq!(
        ipad.count("SELECT priority FROM task_metadata WHERE taskId = 'milk'"),
        2,
        "its metadata moved with it"
    );

    // And back in again: the member has it whole.
    ipad.run("UPDATE tasks SET listId = 'groceries' WHERE id IN ('milk', 'oat')");
    ipad.sync(&mut server);
    phone.sync(&mut server);
    mac.sync(&mut server);
    assert_eq!(phone.title("milk").as_deref(), Some("Milk, later"));
    assert_eq!(phone.title("oat").as_deref(), Some("Oat milk"));
    assert_eq!(
        phone.count("SELECT priority FROM task_metadata WHERE taskId = 'milk'"),
        2
    );
    assert_eq!(
        mac.text("SELECT listId FROM tasks WHERE id = 'milk'")
            .as_deref(),
        Some("groceries")
    );

    // A member's delete reaches everyone.
    phone.run("DELETE FROM tasks WHERE id = 'oat'");
    phone.sync(&mut server);
    mac.sync(&mut server);
    ipad.sync(&mut server);
    assert_eq!(mac.title("oat"), None);
    assert_eq!(ipad.title("oat"), None);
    let mut fresh = Device::another("owner");
    fresh.sync(&mut server);
    assert_eq!(
        fresh.title("oat"),
        None,
        "and a new device doesn't bring it back"
    );
    assert_eq!(fresh.title("milk").as_deref(), Some("Milk, later"));
}

#[test]
fn a_members_other_devices_find_the_list_by_themselves() {
    let mut server = Server::default();
    let mut mac = owner_with_groceries(&mut server);
    let body = server.share("owner", "groceries");
    let space = spaces::decode_space(&body).unwrap().id;
    mac.record(&body);
    mac.sync(&mut server);
    let mut phone = Device::new("member");
    phone.sync(&mut server);
    phone.record(&server.join(&space, "member"));
    phone.sync(&mut server);

    // The member's laptop never saw the invite: the next pull lists the
    // space, and its rows come with it.
    let mut laptop = Device::new("member");
    laptop.sync(&mut server);
    assert_eq!(laptop.title("milk").as_deref(), Some("Milk"));
    laptop.run("UPDATE tasks SET status = 'done' WHERE id = 'oat'");
    laptop.sync(&mut server);
    mac.sync(&mut server);
    phone.sync(&mut server);
    assert_eq!(
        mac.text("SELECT status FROM tasks WHERE id = 'oat'")
            .as_deref(),
        Some("done")
    );
    assert_eq!(
        phone
            .text("SELECT status FROM tasks WHERE id = 'oat'")
            .as_deref(),
        Some("done")
    );
    // The list is in each device's own workspace, whatever it is called.
    assert_eq!(
        laptop.count(
            "SELECT COUNT(*) FROM task_lists WHERE id = 'groceries' AND workspaceId IS NOT NULL"
        ),
        1
    );
}

#[test]
fn leaving_and_stopping_sharing() {
    let mut server = Server::default();
    let mut mac = owner_with_groceries(&mut server);
    let body = server.share("owner", "groceries");
    let space = spaces::decode_space(&body).unwrap().id;
    mac.record(&body);
    mac.sync(&mut server);
    let mut phone = Device::new("member");
    phone.sync(&mut server);
    phone.record(&server.join(&space, "member"));
    phone.sync(&mut server);
    phone.run(&format!(
        "INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
           VALUES ('eggs', 'groceries', 'Eggs', 1, '{T}', '{T}');"
    ));
    phone.sync(&mut server);
    mac.sync(&mut server);

    // The member can't delete it; removed, it goes from their device.
    let tx = phone.connection.transaction().unwrap();
    assert!(crate::lists::delete_list(&tx, "groceries").is_err());
    drop(tx);
    server.remove(&space, "member");
    phone.sync(&mut server);
    assert_eq!(
        phone.count("SELECT COUNT(*) FROM task_lists WHERE id = 'groceries'"),
        0
    );
    assert_eq!(phone.count("SELECT COUNT(*) FROM tasks"), 0);
    assert!(shared_lists(&phone.connection).unwrap().is_empty());
    assert_eq!(
        phone.count("SELECT COUNT(*) FROM task_lists WHERE id = 'inbox-member'"),
        1,
        "their own lists stay"
    );

    // The owner stops sharing: the list stays, the member's task with it,
    // and a new device of the owner's still finds it all.
    server.stop_sharing(&space);
    mac.forget(&space, true);
    mac.sync(&mut server);
    assert_eq!(mac.title("eggs").as_deref(), Some("Eggs"));
    assert!(shared_lists(&mac.connection).unwrap().is_empty());
    let mut ipad = Device::another("owner");
    ipad.sync(&mut server);
    assert_eq!(ipad.title("eggs").as_deref(), Some("Eggs"));
    assert_eq!(ipad.title("milk").as_deref(), Some("Milk"));
    let tx = mac.connection.transaction().unwrap();
    crate::lists::delete_list(&tx, "groceries").unwrap();
}

#[test]
fn what_cant_be_shared_or_deleted() {
    let mut server = Server::default();
    let mut mac = owner_with_groceries(&mut server);
    assert!(shareable_name(&mac.connection, "inbox-owner").is_err());
    assert_eq!(
        shareable_name(&mac.connection, "groceries").unwrap(),
        "Groceries"
    );
    let body = server.share("owner", "groceries");
    mac.record(&body);
    let tx = mac.connection.transaction().unwrap();
    assert!(
        crate::lists::delete_list(&tx, "groceries").is_err(),
        "stop sharing first"
    );
}

#[test]
fn invite_codes_read_from_whatever_was_pasted() {
    let code = "ABCD1234EFGH5678JKMN9PQR".to_string();
    for text in [
        code.clone(),
        format!("  {}  ", code.to_lowercase()),
        sync_invite_code_grouped(code.clone()),
        format!("https://api.example.com/join/{code}"),
        format!("takt://join/{code}?from=mail"),
        sync_invite_link("https://api.example.com/".into(), code.clone()),
    ] {
        assert_eq!(
            sync_invite_code(text.clone()).as_deref(),
            Some(code.as_str()),
            "{text}"
        );
    }
    assert_eq!(sync_invite_code("hello".into()), None);
    assert_eq!(sync_invite_code("https://example.com/join/".into()), None);
}
