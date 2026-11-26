#!/usr/bin/env bash
# Deploys the hosted MCP server (cli/src/remote, docs/remote-mcp.md) to Cloud
# Run as `takt-mcp`, beside `takt-sync`.
#
# Cloud Run builds from one directory, and takt-mcp needs three of the
# repository's (cli/, core/, sync-server/rules/), so this stages copies of
# them, without their target/ directories, and builds cli/remote/Dockerfile
# there.
#
# Its settings are set here on every deploy, since none is secret: tokens
# are checked against the Supabase project's published keys, and every
# request to the sync server carries the caller's own token.
#
# One instance at most: each account's workspace lives in an instance, and
# two instances would be the same device of the account twice over.
#
# Its public address is the mapped custom domain; --set-env-vars replaces
# every variable, so it is set on every deploy rather than once.
#
#   ./scripts/deploy_mcp_server.sh
#   TAKT_MCP_PUBLIC_URL=https://other.example ./scripts/deploy_mcp_server.sh
set -euo pipefail

project="${TAKT_GCP_PROJECT:-takt-511416}"
region="${TAKT_GCP_REGION:-europe-west1}"
service="${TAKT_MCP_SERVICE:-takt-mcp}"
supabase="${TAKT_SUPABASE_URL:-https://rsckzmldfpfjdrvulwke.supabase.co}"
sync="${TAKT_SYNC_URL:-https://api.takt.maybeitssoftware.co.uk}"
# Days are counted in the server's zone; the account's own isn't known.
zone="${TAKT_MCP_TZ:-Europe/London}"
public="${TAKT_MCP_PUBLIC_URL:-https://mcp.takt.maybeitssoftware.co.uk}"

root="$(cd "$(dirname "$0")/.." && pwd)"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
for dir in cli core sync-server/rules; do
  mkdir -p "$stage/$(dirname "$dir")"
  rsync -a --exclude target --exclude .DS_Store "$root/$dir/" "$stage/$dir/"
done
cp "$root/cli/remote/Dockerfile" "$stage/Dockerfile"

env="SUPABASE_URL=$supabase,SYNC_SERVER_URL=$sync,REQUIRE_SUBSCRIPTION=true,TZ=$zone,RUST_LOG=info,PUBLIC_URL=$public"

gcloud run deploy "$service" --source "$stage" --project "$project" --region "$region" \
  --allow-unauthenticated --max-instances 1 --memory 1Gi --cpu 1 --timeout 300 \
  --set-env-vars "$env" --quiet

url=$(gcloud run services describe "$service" --project "$project" --region "$region" --format='value(status.url)')
echo "url: $url"
echo "health: $(curl -fsS -m 30 "$url/health")"
echo "metadata: $(curl -fsS -m 30 "$url/.well-known/oauth-protected-resource")"
