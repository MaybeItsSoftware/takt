#!/usr/bin/env bash
# Deploys sync-server/ to Cloud Run as a new revision of `takt-sync`, which
# serves api.takt.maybeitssoftware.co.uk.
#
# Cloud Build builds sync-server/Dockerfile from that directory alone
# (sync-server/.gcloudignore keeps target/ out of the upload). The service's
# environment — DATABASE_URL (the Supabase session pooler), SUPABASE_URL,
# SUPABASE_SECRET_KEY, RESEND_API_KEY, MAIL_FROM — is kept from the running
# revision, so this changes code only.
#
# Afterwards /health reports {"ok":true,"realtime":true} when the server can
# broadcast on Supabase Realtime; `false` means the apps long-poll, and the
# revision's log says why.
#
#   ./scripts/deploy_sync_server.sh
#   TAKT_GCP_PROJECT=… TAKT_GCP_REGION=… ./scripts/deploy_sync_server.sh
set -euo pipefail

project="${TAKT_GCP_PROJECT:-takt-511416}"
region="${TAKT_GCP_REGION:-europe-west1}"
service="${TAKT_SYNC_SERVICE:-takt-sync}"

cd "$(dirname "$0")/../sync-server"
gcloud run deploy "$service" --source . --project "$project" --region "$region" --quiet

url=$(gcloud run services describe "$service" --project "$project" --region "$region" --format='value(status.url)')
echo "health: $(curl -fsS -m 30 "$url/health")"
