# takt-sync-server

The row-sync server Takt's Mac, iPhone and Android apps replicate their
workspace through. `../docs/sync.md` is the protocol; this crate is its server
half. It stores rows and merges them by per-column last-write-wins, and knows
nothing about tasks.

To run it yourself (a Supabase project for accounts, this server and
Postgres under Docker Compose, Caddy for TLS, and pointing the apps at it),
see [`docs/self-hosting.md`](../docs/self-hosting.md).

## Layout

| File | What |
| --- | --- |
| `src/merge.rs` | The merge rule, pure and unit-tested without a database |
| `src/push.rs` | `POST /v1/push`: one transaction per push, serialised by an advisory lock |
| `src/changes.rs` | `GET /v1/changes`: the paged feed and its long-poll |
| `src/notify.rs` | `LISTEN rows_changed`, fanned out to waiting long-polls |
| `src/realtime.rs` | The Supabase Realtime broadcast a push sends, and the access policy that lets an account's devices hear it |
| `src/auth.rs` | Checking the Supabase access token each request carries |
| `src/devices.rs` | Registering a device, the account's devices, signing out, deleting an account |
| `src/spaces.rs` | Shared lists: spaces, members, invites |
| `src/billing.rs` | Takt Sync: the RevenueCat webhook, and the `402` for an unsubscribed account when `REQUIRE_SUBSCRIPTION` is on |
| `migrations/` | The Postgres schema, applied at boot |

## Configuration

| Variable | |
| --- | --- |
| `DATABASE_URL` | Required. On Supabase, the **session** pooler (port 5432): `LISTEN` needs a session, which the transaction pooler (6543) doesn't keep. The server keeps its tables in a `sync` schema, which the Data API doesn't expose. |
| `PORT` | Default 8080. |
| `SUPABASE_URL` | Required. The project the apps sign in with, `https://<ref>.supabase.co`. Tokens are checked against its published keys (`/auth/v1/.well-known/jwks.json`). |
| `SUPABASE_JWT_SECRET` | Only for a project still signing with the legacy shared secret (HS256). |
| `SUPABASE_SECRET_KEY` | The project's secret API key, used only to delete the Supabase user when an account is deleted. Without it deleting answers 503. |
| `RUST_LOG` | Log filter, default `info`. Logs are JSON lines. |
| `REQUIRE_SUBSCRIPTION` | Off unless set to `true`. Leave it off when you host your own: every feature is free. Takt's hosted server turns it on, and then pushing, pulling and the shared-list calls answer `402` to an account without Takt Sync. |
| `COMP_ACCOUNTS` | Optional, with `REQUIRE_SUBSCRIPTION`. Comma-separated account ids that sync free. |
| `REVENUECAT_WEBHOOK_SECRET` | Optional. The Authorization header RevenueCat sends to `POST /v1/webhooks/revenuecat`. Without it that route answers 404. |

## Running locally

```bash
DATABASE_URL=postgres://localhost/priority_sync cargo run
```

## Tests

The unit tests need nothing. The integration tests in `tests/api.rs` need a
Postgres server: `#[sqlx::test]` creates a fresh database per test from
`DATABASE_URL`, so the user in it must be able to `CREATE DATABASE`.

A throwaway server with Homebrew's Postgres:

```bash
PGDIR=$(mktemp -d)
initdb -D "$PGDIR/data" -U postgres --auth=trust >/dev/null
pg_ctl -D "$PGDIR/data" -o "-p 54329 -k $PGDIR" -l "$PGDIR/log" start

DATABASE_URL=postgres://postgres@localhost:54329/postgres cargo test

pg_ctl -D "$PGDIR/data" stop && rm -rf "$PGDIR"
```

Or with Docker:

```bash
docker run --rm -d --name sync-pg -e POSTGRES_HOST_AUTH_METHOD=trust -p 54329:5432 postgres:16
DATABASE_URL=postgres://postgres@localhost:54329/postgres cargo test
docker stop sync-pg
```

The gates:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
docker build .
```

## Deploying

Takt's own: Google Cloud Run, service `takt-sync` in project `takt-511416`
(europe-west1), with `api.takt.maybeitssoftware.co.uk` mapped to it, built from
this directory by `../scripts/deploy_sync_server.sh`. Its database is the
Supabase project's Postgres, through the session pooler.

Anyone else's: `docker-compose.yml` here runs the server with a Postgres of
its own, and Caddy for HTTPS behind the `tls` profile (`.env.example` lists
the settings). The walk-through is
[`docs/self-hosting.md`](../docs/self-hosting.md).
