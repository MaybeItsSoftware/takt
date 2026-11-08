-- v37_shared_lists: what this device knows of the shared lists its account
-- is in (docs/shared-lists-plan.md). None of it syncs: the server sends the
-- spaces with every page of changes, and the rest is this device's record of
-- where it last sent or received each row.
-- `sync_spaces` is one row per shared list, as the last page described it.
-- `sync_row_spaces` names the shared space each row was last pushed to or
-- pulled from; a row with no entry is in the account's own space. A row
-- whose space changes (a task moved into or out of a shared list) is found
-- by comparing this with where the row belongs now.
-- `sync_push_plan` holds the spaces a push in flight assigns its rows to,
-- so they are recorded only once the server has accepted it.
CREATE TABLE sync_spaces (
  spaceId TEXT PRIMARY KEY NOT NULL,
  listId TEXT NOT NULL,
  role TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  membersJSON TEXT NOT NULL DEFAULT '[]'
);
-- statement
CREATE UNIQUE INDEX sync_spaces_on_listId ON sync_spaces(listId);
-- statement
CREATE TABLE sync_row_spaces (
  tableName TEXT NOT NULL,
  rowId TEXT NOT NULL,
  spaceId TEXT NOT NULL,
  PRIMARY KEY (tableName, rowId)
);
-- statement
CREATE INDEX sync_row_spaces_on_spaceId ON sync_row_spaces(spaceId);
-- statement
CREATE TABLE sync_push_plan (
  tableName TEXT NOT NULL,
  rowId TEXT NOT NULL,
  spaceId TEXT,
  PRIMARY KEY (tableName, rowId)
);
