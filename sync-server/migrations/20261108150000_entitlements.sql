-- Takt Sync, the hosted service's yearly subscription (docs/sync.md,
-- "Subscription"). Bought in the iPhone or Android app and reported by
-- RevenueCat's webhook; read only when the server runs with
-- REQUIRE_SUBSCRIPTION, so a self-hosted server never looks here.

CREATE TABLE entitlements (
  account_id UUID PRIMARY KEY,
  active BOOLEAN NOT NULL,
  -- When the paid year runs out; NULL for one that doesn't (a comp, or a
  -- non-renewing purchase with no end).
  expires_at TIMESTAMPTZ,
  -- 'app_store', 'play_store' and the like from RevenueCat, or 'comp' for
  -- one granted by hand, which no webhook event overwrites.
  source TEXT NOT NULL,
  product TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- RevenueCat retries a webhook until it is answered; each event once.
CREATE TABLE entitlement_events (
  id TEXT PRIMARY KEY,
  received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE entitlements ENABLE ROW LEVEL SECURITY;
ALTER TABLE entitlement_events ENABLE ROW LEVEL SECURITY;
