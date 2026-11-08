-- Shared lists (docs/shared-lists-plan.md). A row now belongs to a space
-- rather than to an account. Every account has a personal space whose id is
-- the account's own, which is what every row synced before this belongs to;
-- sharing a list makes a space of its own that its members read and write.
--
-- Personal spaces have no `spaces` row: an account's personal space is
-- implied by the account, so nothing has to be made when one signs up.

CREATE TABLE spaces (
  id UUID PRIMARY KEY,
  owner_account_id UUID NOT NULL,
  -- The id of the shared list, as the owner's devices know it. Opaque here,
  -- like every other row id.
  list_id TEXT NOT NULL,
  -- The list's name when it was shared, for an invite's preview. The list's
  -- own row carries the name the members see.
  name TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (owner_account_id, list_id)
);

CREATE TABLE space_members (
  space_id UUID NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
  account_id UUID NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'member')),
  -- The email the member signed in with, so the others can see who is in.
  email TEXT,
  joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (space_id, account_id)
);
CREATE INDEX space_members_account ON space_members(account_id);

CREATE TABLE space_invites (
  code TEXT PRIMARY KEY,
  space_id UUID NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
  created_by UUID NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  used_by UUID,
  used_at TIMESTAMPTZ
);
CREATE INDEX space_invites_space ON space_invites(space_id);

-- Every existing row is in its account's personal space.
ALTER TABLE rows ADD COLUMN space_id UUID;
UPDATE rows SET space_id = account_id;
ALTER TABLE rows ALTER COLUMN space_id SET NOT NULL;
ALTER TABLE rows DROP CONSTRAINT rows_pkey;
ALTER TABLE rows ADD PRIMARY KEY (space_id, table_name, row_id);
DROP INDEX rows_account_seq;
CREATE INDEX rows_space_seq ON rows(space_id, seq);
-- `account_id` stays: on a personal row it is the owner, on a shared one the
-- account that last wrote it.

ALTER TABLE spaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE space_members ENABLE ROW LEVEL SECURITY;
ALTER TABLE space_invites ENABLE ROW LEVEL SECURITY;
