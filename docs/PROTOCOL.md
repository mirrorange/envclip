# envclip protocol v1

This document specifies everything needed to write a compatible client. The
reference implementations are `cli/src/crypto.rs` (Rust) and
`worker/public/crypto.js` (WebCrypto); both are tested against the vector at
the end of this file.

## Key material

```
secret       16 bytes from a CSPRNG, fresh per drop
key string   Crockford base32 of secret, lowercase, 26 chars, no padding
             alphabet 0123456789abcdefghjkmnpqrstvwxyz
             (the final char carries 3 data bits; the 2 trailing bits are 0)
             parsers ignore case, '-', spaces and an optional "ek_" prefix,
             and map o→0, i/l→1

PRK          HKDF-Extract(SHA-256, salt = "envclip/v1", IKM = secret)
enc_key      HKDF-Expand(PRK, info = "enc",   L = 32)    AES-256-GCM key
claim_token  HKDF-Expand(PRK, info = "claim", L = 32)    bearer secret
claim_hash   lowercase hex(SHA-256(claim_token))         stored by the server
```

The two HKDF outputs are independent: the server, which eventually sees
`claim_token`, learns nothing about `enc_key`.

## Payload

```json
{"v":1,"env":[["NAME","value"],["OTHER","multi\nline"]]}
```

UTF-8 JSON, names match `[A-Za-z_][A-Za-z0-9_]*`, order is preserved, later
duplicates win. Before encryption the JSON is right-padded with ASCII spaces
to a multiple of 256 bytes (minimum 256) so ciphertext length only reveals a
coarse size class. JSON parsers ignore the trailing whitespace.

## Envelope

```
sealed = base64url_nopad( 0x01 || nonce[12] || AES-256-GCM(enc_key, nonce, aad = "envclip/v1", padded) )
```

`0x01` is the envelope version. `nonce` is random per drop. The GCM tag (16
bytes) is appended by the cipher as usual.

## HTTP API

All bodies are JSON. Every response carries `cache-control: no-store`.
Authenticated routes use `Authorization: Bearer <base64url_nopad(claim_token)>`
(43 chars); the server hashes the decoded 32 bytes and compares with the stored
`claim_hash`.

| method | path | auth | result |
|---|---|---|---|
| `POST` | `/api/v1/drops` | – | `201 {"id","expires_at"}` |
| `POST` | `/api/v1/drops/:id/claim` | bearer | `200 {"ciphertext"}`, row deleted |
| `GET` | `/api/v1/drops/:id` | bearer | `200 {"created_at","expires_at"}` |
| `DELETE` | `/api/v1/drops/:id` | bearer | `204`, row deleted |

Create body: `{"ciphertext": sealed, "claim_hash": hex64, "ttl": seconds}`
with `60 ≤ ttl ≤ 604800` (default 86400) and `ciphertext` ≤ 90 000 chars.

Wrong token, already claimed, revoked and expired are all the same
`404 {"error":"not_found"}` — a probe cannot tell them apart, and a wrong key
never destroys a drop. A claim is one statement:

```sql
DELETE FROM drops WHERE id = ? AND claim_hash = ? AND expires_at > ? RETURNING ciphertext
```

so at most one request ever receives the ciphertext. A cron trigger deletes
expired rows every 15 minutes. Errors: `400 invalid_*`, `401 unauthorized`,
`413 too_large`, `429 rate_limited`.

Non-API routes:

| path | |
|---|---|
| `/` | web app (static assets) |
| `/c/:id` | web landing page if `Accept` includes `text/html`, otherwise a POSIX `sh` installer bound to the drop: `curl -fsSL …/c/:id \| sh -s -- <key>` |
| `/install.sh` | installer only |
| `/dl/envclip-{linux,darwin}-{x86_64,aarch64}[.sha256]` | 302 to the release asset |

## Agent

`envclip claim` hands the decrypted variables to a detached `envclip __agent`
process (started with a scrubbed environment, `setsid`, stdio on `/dev/null`).
The first batch travels over the spawn pipe; later batches over the socket.

- Socket: `$ENVCLIP_SOCK`, else `$XDG_RUNTIME_DIR/envclip/agent.sock`, else
  `$TMPDIR/envclip-<uid>/agent.sock`. The directory must be owned by the user
  with mode `0700`, the socket is `0600`, and every connection's peer uid
  (`SO_PEERCRED` / `getpeereid`) must equal the agent's uid.
- Hardening: `RLIMIT_CORE = 0`; on Linux `PR_SET_DUMPABLE = 0`, which blocks
  same-uid `ptrace` and `/proc/<pid>/mem` access (root is out of scope).
- Protocol: one JSON line request, one JSON line response.
  `put {env,src,hold}`, `list`, `get {names}`, `rm {names}`, `stop`.
- Lifetime: `--hold` (default 12 h), extended by later claims. On expiry,
  `clear`, or when the last variable is removed, values are zeroized and the
  agent exits.

`envclip env` prints, for each name,

```sh
NAME="$('/path/to/envclip' get NAME && echo .)" && export NAME="${NAME%.}"
```

The trailing `.` preserves values that end in newlines. The text itself never
contains a secret; the value only exists inside the evaluating shell.

## Threat model

In scope:

- **Server compromise / operator curiosity.** Only ciphertext, claim hashes,
  timestamps and padded sizes are stored; none of it decrypts.
- **Transcript exposure.** The chat holds a one-time capability, never a value.
  After the sandbox claims, the capability is dead. A third party that races
  the agent with the link and key would get the values — but then the agent's
  claim fails visibly, which is the signal to revoke and rotate.
- **Accidental echo.** No envclip command prints values except `get`, which
  exists for pipes and is never suggested to the agent.
- **Link-only attackers.** Without the key they cannot read, burn or even
  confirm the existence of a drop.

Out of scope:

- A malicious or prompt-injected agent that deliberately prints or exfiltrates
  values it was given. Scope and expire your credentials accordingly.
- Root on the sandbox host, or a compromised CLI binary / web origin. For the
  web sender you trust the JavaScript served by your deployment; the CLI avoids
  that and also avoids the clipboard.
- SQLite page reuse: deleted rows may persist in free pages of the D1 database
  until overwritten. They are still ciphertext.

## Test vector

```
secret       0x42 × 16
key string   89144gj289144gj289144gj288
claim_token  5afZeGlOYEASJ0SfNwMNRCBmGES_1QpG7JIxmyAPvS8
claim_hash   7d6867f5035879db744c92ef7ddf44a1d87f68ed2be2d14a73dc6a3037585534
enc_key      c81f207627f27d4c848db1a04317eb316b9d48b662d78fb65b2173ab7aa4af48
```
