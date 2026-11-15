#!/usr/bin/env bash
# Sets Takt Sync's billing environment on the hosted sync server (Cloud Run
# service `takt-sync`) from ~/.config/takt/revenuecat.env, leaving the rest
# of the service's environment as it is. docs/sync.md, "Subscription", says
# what each variable does.
#
# revenuecat.env lines read here:
#   REVENUECAT_WEBHOOK_SECRET=…   the Authorization header RevenueCat sends
#   COMP_ACCOUNTS=<uuid>,<uuid>   accounts that sync free (optional)
#   REQUIRE_SUBSCRIPTION=true     charge for hosted sync (optional)
# The same file holds REVENUECAT_IOS_KEY and TAKT_REVENUECAT_ANDROID_KEY for
# scripts/build_app_store.sh and scripts/build_play_bundle.sh.
#
# Turn REQUIRE_SUBSCRIPTION on only once phone builds that can sell Takt
# Sync are live in both stores: from then on, an account without it stops
# syncing.
#
#   ./scripts/configure_billing.sh
#   TAKT_BILLING_ENV=… ./scripts/configure_billing.sh
set -euo pipefail

project="${TAKT_GCP_PROJECT:-takt-511416}"
region="${TAKT_GCP_REGION:-europe-west1}"
service="${TAKT_SYNC_SERVICE:-takt-sync}"
env_file="${TAKT_BILLING_ENV:-$HOME/.config/takt/revenuecat.env}"

if [[ ! -f "$env_file" ]]; then
  echo "error: $env_file doesn't exist." >&2
  exit 1
fi

read_var() { sed -n "s/^$1=//p" "$env_file" | head -n1 | tr -d '[:space:]'; }

secret="$(read_var REVENUECAT_WEBHOOK_SECRET)"
comp="$(read_var COMP_ACCOUNTS)"
required="$(read_var REQUIRE_SUBSCRIPTION)"
required="${required:-false}"

if [[ -z "$secret" ]]; then
  echo "error: no REVENUECAT_WEBHOOK_SECRET in $env_file." >&2
  exit 1
fi
if [[ "$required" == "true" && -z "$comp" ]]; then
  echo "warning: charging with no COMP_ACCOUNTS, so your own account needs a subscription too." >&2
fi

# COMP_ACCOUNTS holds commas, so the list uses `|` as gcloud's separator.
gcloud run services update "$service" --project "$project" --region "$region" --quiet \
  --update-env-vars "^|^REVENUECAT_WEBHOOK_SECRET=$secret|COMP_ACCOUNTS=$comp|REQUIRE_SUBSCRIPTION=$required"

url=$(gcloud run services describe "$service" --project "$project" --region "$region" --format='value(status.url)')
echo "health: $(curl -fsS -m 30 "$url/health")"

# Without the secret the webhook is a 404; with it, an empty event is
# accepted or rejected, but the route is there.
code=$(curl -s -o /dev/null -w '%{http_code}' -m 30 -X POST "$url/v1/webhooks/revenuecat" \
  -H "Authorization: Bearer $secret" -H 'content-type: application/json' -d '{}')
if [[ "$code" == "404" ]]; then
  echo "error: the webhook still answers 404." >&2
  exit 1
fi
echo "webhook: answering ($code). REQUIRE_SUBSCRIPTION=$required."
