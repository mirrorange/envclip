-- one row per sealed clip; deleted on open, revoke or expiry
CREATE TABLE clips (
  id         TEXT    PRIMARY KEY,
  auth_hash  TEXT    NOT NULL,
  blob       TEXT    NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
) STRICT;

CREATE INDEX clips_expires_at ON clips (expires_at);
