-- One row per sealed drop. The server only ever holds ciphertext and a hash
-- of the claim token; rows are deleted on claim, revoke, or expiry.
CREATE TABLE drops (
  id          TEXT    PRIMARY KEY,
  ciphertext  TEXT    NOT NULL,
  claim_hash  TEXT    NOT NULL,
  created_at  INTEGER NOT NULL,
  expires_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX drops_expires_at ON drops (expires_at);
