# Self-hosting sync

Takt's Mac and phone apps keep one workspace in step through a small sync
server (`sync-server/`, protocol in [sync](sync.md)). By default they use
Takt's hosted server and Takt's Supabase project for accounts. This guide sets
up your own pair instead, so your synced rows sit in a database you run.

There are two parts:

1. **A Supabase project** for accounts. The apps sign in with Supabase Auth
   (email and password, Apple, Google). The server never handles a password:
   it only checks the access token each request carries against the
   project's published keys.
2. **The sync server** and a Postgres database for it. That database can be a
   container next to the server (the compose file here), or the Supabase
   project's own Postgres.

## Is Supabase required?

Yes. Supabase Auth is what the apps sign in with, and nothing else in Takt
issues the tokens the server checks.

The server can check HS256 tokens signed with a shared secret
(`SUPABASE_JWT_SECRET`), but that is not a Supabase-free mode. The token
still has to come from a Supabase Auth server. It must carry
`iss = <SUPABASE_URL>/auth/v1`, `aud = authenticated`, `role = authenticated`
and a uuid `sub`, and the apps only know how to get one from Supabase Auth.
A token minted by hand would get past the server, but no app can sign in to
get one.

Supabase's free tier is enough for one person's devices. A self-hosted
Supabase should also work if its Auth server issues tokens as above. This has
not been tested.

## 1. The Supabase project

1. Create a project at [supabase.com](https://supabase.com/dashboard) (the
   free tier is fine), or use one you already have. Takt's server keeps its
   tables in a `sync` schema of its own, which the Data API doesn't expose, so
   it won't collide with anything else in an existing project.
2. **Project Settings → API Keys.** Note two values:
   - the **publishable key** (`sb_publishable_…`, or the legacy `anon` key).
     The apps need it. It is made to ship in apps.
   - the **secret key** (`sb_secret_…`, or the legacy `service_role` key).
     This one is optional, and goes to the server only, as
     `SUPABASE_SECRET_KEY`. It lets "Delete account" remove the Supabase
     user. Never enter it in an app; the apps refuse an `sb_secret_` key.
3. **Project Settings → API → Project URL**, `https://<ref>.supabase.co`, is
   `SUPABASE_URL` for the server and "Supabase URL" in the apps.
4. **Authentication → URL Configuration → Redirect URLs:** add
   `takt://auth-callback`. Confirmation emails, password-reset links and
   Apple/Google in a browser come back to the app through it.
5. **Authentication → Sign In / Providers:**
   - **Email** is on by default. By default Supabase asks new accounts to
     confirm their address. The built-in mailer only sends a few emails an
     hour, so set up custom SMTP, or turn confirmation off, if that gets in
     the way.
   - **Apple** and **Google** are optional and need your own developer
     credentials in the provider's settings. On the Mac both run in a browser
     sheet, so they work once configured. On Android, Apple runs in a Custom
     Tab and works the same way. Google on Android uses a client id built
     into the app for Takt's project, so the app hides it while it points at
     your own project; sign in with email there instead.
6. **JWT keys.** New projects sign tokens with an asymmetric key and publish
   it at `/auth/v1/.well-known/jwks.json`, which the server fetches itself.
   Nothing to copy. A project that still signs with the **legacy JWT
   secret** (HS256) needs that secret as `SUPABASE_JWT_SECRET` on the server
   (**Project Settings → JWT Keys → Legacy JWT secret**).

## 2. The server

### Configuration

| Variable | |
| --- | --- |
| `DATABASE_URL` | Required. A Postgres URL. The compose file sets it to its own `db` service. To use the Supabase project's database instead, use its **session** pooler (port 5432) or a direct connection. Don't use the transaction pooler (6543): the server needs `LISTEN`, which needs a session. |
| `SUPABASE_URL` | Required. `https://<ref>.supabase.co`. |
| `SUPABASE_SECRET_KEY` | Optional. Without it, "Delete account" answers 503. |
| `SUPABASE_JWT_SECRET` | Optional. Only for a project on the legacy HS256 secret. |
| `PORT` | Default 8080. |
| `RUST_LOG` | Default `info`. Logs are JSON lines. |
| `REQUIRE_SUBSCRIPTION` | Off unless set to `true`. Leave it off when you host your own: every feature is free. Takt's hosted server turns it on, and then pushing, pulling and the shared-list calls answer `402` to an account without Takt Sync. |
| `COMP_ACCOUNTS` | Optional, with `REQUIRE_SUBSCRIPTION`. Comma-separated account ids that sync free. |
| `REVENUECAT_WEBHOOK_SECRET` | Optional. The Authorization header RevenueCat sends to `POST /v1/webhooks/revenuecat`. Without it that route answers 404. |

A server you host yourself never charges: `REQUIRE_SUBSCRIPTION` is off
unless you set it, and the apps sync with it with every feature, shared
lists included. The £1-a-year Takt Sync subscription is only for Takt's own
hosted server.

### Migrations

There is nothing to run by hand. The migrations in `sync-server/migrations/`
are compiled into the binary (`sqlx::migrate!`). On boot the server creates
the `sync` schema if it is missing and applies any migrations not yet
applied, before it listens. The database user therefore needs to be able to
create a schema in its database. The compose file's `takt` user owns its
database, and Supabase's `postgres` user can as well.

### Docker Compose

From `sync-server/`:

```bash
cp .env.example .env
# Fill in .env: POSTGRES_PASSWORD (openssl rand -hex 24), SUPABASE_URL,
# and optionally SUPABASE_SECRET_KEY / SUPABASE_JWT_SECRET.
docker compose up -d --build
curl http://localhost:8080/health      # {"ok":true}
docker compose logs -f server          # "migrations applied", then "listening"
```

This starts Postgres 16 (data in the `pgdata` volume) and the server, which
the compose file builds from the `Dockerfile`. The server answers plain HTTP
on `127.0.0.1:8080` only. Put TLS in front of it before your phones connect.

To use the Supabase project's database instead of the bundled one, set
`DATABASE_URL` in the `server` service to the session pooler URL. Then delete
the `db` service and the `depends_on` that names it.

Without Docker, it is a plain Rust binary:

```bash
cd sync-server
DATABASE_URL=postgres://… SUPABASE_URL=https://<ref>.supabase.co cargo run --release
```

### TLS and a reverse proxy

The apps expect HTTPS. Android refuses plain HTTP outright. On the Mac, App
Transport Security lets plain HTTP through only to local addresses such as
`http://localhost:8080`, which is good for trying things out and nothing
else.

The compose file has Caddy behind a `tls` profile. It gets and renews a
Let's Encrypt certificate by itself:

1. Point a DNS record (say `sync.example.com`) at the machine, and open ports
   80 and 443.
2. Set `TAKT_DOMAIN=sync.example.com` in `.env`.
3. `docker compose --profile tls up -d --build`
4. `curl https://sync.example.com/health`

The whole `Caddyfile` is:

```caddyfile
{$TAKT_DOMAIN} {
	encode zstd gzip
	reverse_proxy server:8080
}
```

With a proxy of your own (nginx, Traefik, a Cloudflare Tunnel), forward
everything to port 8080. Allow responses of at least 30 seconds: devices hold
`GET /v1/changes` open for up to 25 s while the app is in front. For nginx,
that means `proxy_read_timeout 60s;` and `proxy_buffering off;`.

The server keeps no state on disk, so a restart loses nothing. More than one
replica is also safe: pushes are serialised by a Postgres advisory lock, and
long-polls are woken through `LISTEN/NOTIFY`.

## 3. The apps

The Mac and Android apps take the same three values. On each, open
**Settings → Sync**, sign out if you are signed in, and expand **Use a
different server**:

| Field | Value |
| --- | --- |
| Sync server | `https://sync.example.com` |
| Supabase URL | `https://<ref>.supabase.co` |
| Supabase publishable key | `sb_publishable_…` (or the anon key) |

Leave a field blank to use Takt's. Leave both Supabase fields blank to keep
Takt's accounts with a server of your own; that server's `SUPABASE_URL` must
then be Takt's project. Pasted addresses are tidied: `https://` is added when
missing, and a trailing slash, `/auth/v1` or `/rest/v1` is dropped.

**Check** asks the server's `/health` and the project's `/auth/v1/settings`
(with the key) whether they answer. If either fails, it says which and why.
Signing in or creating an account runs the same check first. Once both
answer, the app saves the values and signs in against them.

Changing the values signs the device out first: off the old server's device
list, and out of the old Supabase project. Whatever was waiting to sync to the
old server is dropped, but the workspace on the device is kept. When it signs
in to the new server, the device uploads its whole workspace, the same as any
first sign-in. The values are kept through signing out and relaunching.

To go back to Takt's, collapse **Use a different server** (or clear all three
fields) and sign in again.

Every device that should share a workspace needs the same three values and
the same account.

The iPhone app can be pointed at another sync server, but not yet at another
Supabase project. Its accounts are Takt's.

### Building the Android app against your own pair

You can also bake your own pair into a build as its defaults, instead of
entering them at run time:

```bash
cd mobile/android
./gradlew :app:assembleRelease \
  -PprioritySyncServer=https://sync.example.com \
  -PprioritySupabaseUrl=https://<ref>.supabase.co \
  -PprioritySupabaseKey=sb_publishable_… \
  -PpriorityGoogleWebClientId=<your Google web client id, optional>
```

## Troubleshooting

- **The check says `/health` answered 404 or HTML.** The address reaches
  something other than the sync server. Check the proxy's upstream.
- **"Supabase refused that key".** That is not the project's publishable or
  anon key, or it is from another project.
- **Signing in works, then "Signed out. Sign in again".** The server is
  refusing the project's tokens. Its `SUPABASE_URL` must be the same project
  as the apps' Supabase URL, exactly, with no trailing path. A legacy-secret
  project also needs `SUPABASE_JWT_SECRET`.
- **Confirmation or reset links open a browser page instead of the app.**
  `takt://auth-callback` is missing from the project's Redirect URLs.
- **"Delete account" answers that it isn't set up.** Set
  `SUPABASE_SECRET_KEY` on the server.
