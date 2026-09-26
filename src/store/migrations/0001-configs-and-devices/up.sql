CREATE TABLE configs (
  key        TEXT NOT NULL,
  scope      TEXT NOT NULL DEFAULT '',
  id         TEXT NOT NULL DEFAULT '',
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (key, scope, id)
) WITHOUT ROWID;

CREATE TABLE devices (
  device_id             TEXT PRIMARY KEY,
  certificate_der       BLOB NOT NULL,
  protocol_version      INTEGER NOT NULL,
  name                  TEXT,
  device_type           TEXT,
  incoming_capabilities TEXT,
  outgoing_capabilities TEXT,
  paired_at             INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);
