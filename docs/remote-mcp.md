# The hosted MCP server

`takt mcp` (see [`mcp-server.md`](mcp-server.md)) is a local process, which
an assistant on the web or on a phone can't start. `takt-mcp` is the same
tools over HTTPS, signed in to the user's own Takt account, for anyone with
Takt Sync. Someone adds its address to their assistant as a custom
connector, signs in with their Takt account, allows the assistant, and the
assistant works on their synced workspace as one more of their devices.

| | |
|---|---|
| Code | `cli/src/remote/` (built only with `--features remote`), `cli/src/bin/takt-mcp.rs` |
| Container | `cli/remote/Dockerfile` |
| Deploy | `scripts/deploy_mcp_server.sh` → Cloud Run `takt-mcp`, project `takt-511416`, europe-west1 |
| Consent page | `site/oauth/consent/` → `https://takt.maybeitssoftware.co.uk/oauth/consent/` |
| Endpoint | `POST /mcp` (MCP Streamable HTTP, JSON responses only) |

The `takt` the app bundles is built without the feature, so it carries no web
server.

## How a call is answered

1. **Who.** `POST /mcp` wants `Authorization: Bearer <token>`, a token from
   the Supabase project's OAuth 2.1 server. It is checked as the sync server
   checks the apps' tokens (`auth.rs`): signed by a key the project
   publishes at `/auth/v1/.well-known/jwks.json`, issuer
   `<project>/auth/v1`, audience and role `authenticated`, unexpired. The
   account is its `sub`. Without one the answer is `401` with
   `WWW-Authenticate: Bearer resource_metadata="<base>/.well-known/oauth-protected-resource"`,
   which is how an MCP client finds where to sign in.
2. **Paid.** With `REQUIRE_SUBSCRIPTION` (the hosted server sets it), the
   account must have Takt Sync: `GET /v1/account` on the sync server, with
   the caller's token, must say `subscription.entitled` (`docs/sync.md`,
   "Subscription"). This holds even while sync itself is free. It is asked
   before the account's first call and again every ten minutes. Without it
   the tool answers an error saying how to subscribe, and nothing else
   happens: no device is registered, nothing is read.
3. **The workspace.** The sync server stores rows and knows nothing about
   tasks, so the tools run here, against a SQLite workspace kept for the
   account (`accounts.rs`). The first call joins the account as a phone does
   on first sign-in. It registers a device, "AI assistants (remote MCP)",
   whose id is derived from the account id so it is always the same device.
   Then it makes a database with the core's migrations, pairs it, pulls
   everything and runs the core's `bootstrap`. Every call then runs between a
   sync cycle (push, then pull) before it and a push after it, all with the
   caller's own token. So a tool sees what a phone did a moment ago, the
   phone hears of the change at once through the sync server's Realtime
   broadcast, and the sync server decides what may be read exactly as it
   would for the user's phone.
4. **The tools.** The tool table is `tools.rs`, and the JSON-RPC is the
   stdio server's (`mcp::respond`), shared through `mcp::ToolHost`. The
   hosted server offers the tools that need only the synced workspace
   (`host.rs`): every `workspace_*` tool, `agent_instructions_get`/`_set`,
   `focus_status` and `focus_history`. It doesn't offer the Checkvist tools
   or the dailies and day-log tools, which read the Mac's own files.

A failed push after a write doesn't undo the write. The answer says the
change hasn't reached the other devices yet, and the next call's cycle sends
it. A workspace is dropped after 30 quiet minutes (`IDLE_SECONDS`), but never
while its outbox holds something unsent. A new instance starts from a fresh
pull, which took about half a second for a 1,400-row workspace. A warm call
takes about 70 ms.

No device condition holds here (`set_this_device_kind(None)`). The server is
no device in the user's hand, so a "Laptop" task isn't available to it,
just as it isn't on a phone. Days are counted in the server's zone (`TZ`,
Europe/London on the hosted server), since the account's own zone isn't
known to it.

### One instance

Each account's workspace lives in one instance. Two instances would be the
same device of the account twice, each with its own outbox and clock, so the
service runs with `--max-instances 1`. Cloud Run sends an instance up to 80
requests at once, and calls for one account take turns on its workspace.

## Signing in: Supabase as the authorization server

The server is only an OAuth *resource server*. Everything else is the
Supabase project's OAuth 2.1 server (beta):

- **Discovery.** `GET /.well-known/oauth-protected-resource` (and
  `…/oauth-protected-resource/mcp`) names `<project>/auth/v1` as the
  authorization server. Clients then read Supabase's metadata at
  `<project>/.well-known/oauth-authorization-server/auth/v1`.
- **Registration.** Dynamic client registration is left off: Supabase warns
  that it lets anyone register a client, phishing apps included. Each
  assistant is instead registered by hand under **Authentication → OAuth
  Apps** as a public client (PKCE, no secret). Claude's is `Claude`, client
  ID `26b3e190-a85d-400f-adf6-1035031799cc`, with redirect URIs
  `https://claude.ai/api/mcp/auth_callback` and
  `https://claude.com/api/mcp/auth_callback`. Without a registration
  endpoint in Supabase's metadata, the client ID is entered with the
  connector.
- **Consent.** Supabase sends the browser to the project's *authorization
  path*, relative to its Site URL: `/oauth/consent/` on
  `https://takt.maybeitssoftware.co.uk`, served by GitHub Pages from
  `site/oauth/consent/index.html`. The page signs the user in if needed
  (Google, Apple, or email and password, as the apps do). A Google or Apple
  round trip comes back to the page's bare address, so that one URL must be
  in the project's redirect list. The page then shows which assistant is
  asking, and calls supabase-js's `auth.oauth.approveAuthorization` or
  `denyAuthorization`, which send the browser back to the assistant.
- **Tokens** last an hour and are refreshed by the client. Supabase's OAuth
  tokens have the same audience and role as the apps' sessions, plus
  `client_id`, so the sync server accepts them as they are.
- **Revoking.** Removing the connector in the assistant ends its use.

## Configuration

| Variable | |
|---|---|
| `SUPABASE_URL` | Required. `https://<ref>.supabase.co`. |
| `SYNC_SERVER_URL` | Required. The sync server, e.g. `https://api.takt.maybeitssoftware.co.uk`. |
| `PUBLIC_URL` | This server's address as clients see it: `https://mcp.takt.maybeitssoftware.co.uk`, which the deploy script sets. Without it, it is taken from each request's `Host`, as `https`. |
| `REQUIRE_SUBSCRIPTION` | Off unless `true`. Off means anyone signed in is served, which is what a self-hosted copy wants. |
| `SUPABASE_JWT_SECRET` | Only for a project still signing with the legacy HS256 secret. |
| `DATA_DIR` | Where the workspaces are kept; `/data` in the container. |
| `IDLE_SECONDS` | How long a quiet account's workspace is kept; default 1800. |
| `TZ` | The zone days are counted in. |
| `PORT` | Default 8080. |

`scripts/deploy_mcp_server.sh` sets all of these on every deploy. None of
them is secret.

## Setting it up (once)

1. Deploy the sync server, so `GET /v1/account` says `entitled`:
   `./scripts/deploy_sync_server.sh`.
2. Deploy this one: `./scripts/deploy_mcp_server.sh`.
3. In the Supabase dashboard:
   - **Authentication → OAuth Server**: enable the OAuth 2.1 server and set
     the authorization path to `/oauth/consent/`. Leave dynamic client
     registration off.
   - **Authentication → OAuth Apps**: a public app per assistant, as above.
   - **Authentication → URL Configuration**: make sure the Site URL is
     `https://takt.maybeitssoftware.co.uk`, and add
     `https://takt.maybeitssoftware.co.uk/oauth/consent/` to the redirect
     URLs.
4. `mcp.takt.maybeitssoftware.co.uk` is mapped to the service
   (`gcloud beta run domain-mappings create --service takt-mcp --domain …`,
   with a CNAME to `ghs.googlehosted.com` at Spaceship), and the deploy
   script sets it as `PUBLIC_URL` on every deploy.
5. In Claude: Settings → Connectors → Add custom connector, with
   `https://mcp.takt.maybeitssoftware.co.uk/mcp` and, under Advanced
   settings, the OAuth client ID above (no secret).

## Checking it

`cargo test --manifest-path cli/Cargo.toml --features remote` covers the
token checks, the metadata, the 401, which tools are offered and the
entitlement parsing. End to end, it was checked against a local Postgres,
the real sync server and `takt-mcp`, with HS256 test tokens
(`SUPABASE_JWT_SECRET` on both). That check covered:

- the 401s and the metadata;
- `initialize` and `tools/list`;
- the refusal for an account without Takt Sync while sync is free;
- writes from the server, read back by a fresh instance;
- a real 1,400-row workspace pushed by a "Mac" and then read and written
  through the server, with no stray rows or tombstones.

## Tool annotations

Every tool carries MCP `annotations`, which clients and the Claude
connector directory read: a `title`, `readOnlyHint` (true of the core's
read list, `core/src/agent_tools.rs`), `destructiveHint` on writes (false
only for those that just add, `ADDITIVE_TOOLS` in `cli/src/mcp.rs`, since
any other write changes or removes what is there) and `openWorldHint`
(true only of the Checkvist tools, which the hosted server doesn't offer).
A CLI test fails on a tool without them.
