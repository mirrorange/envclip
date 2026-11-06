# envclip protocol · v1

envclip moves a small set of environment variables from a person to an agent
sandbox. The server only ever stores ciphertext, hands it out once, and forgets it.

```
 you (cli / browser)              envclip edge (Worker + D1)            agent sandbox
 ───────────────────              ──────────────────────────            ─────────────
 key  = random(32)
 enc  = HKDF(key, "enc")
 auth = HKDF(key, "auth")
 blob = AES-256-GCM(enc, vars)
        ── POST {id, sha256(auth), blob} ──▶  store row
 share  link + key  ─────────────────────────────────────────────────▶  curl link | sh -s -- key
                                          ◀── POST {auth} ──────────── envclip open
                                   DELETE … RETURNING blob ──────────▶  decrypt in memory
                                          (row is gone)                 envclip agent holds vars
                                                                         envclip run -- cmd
```

## Identifiers

| item | format |
| ---- | ------ |
| `id`  | 12 chars, alphabet `0123456789abcdefghjkmnpqrstvwxyz` (60 bits), chosen by the client |
| `key` | 32 random bytes, base64url without padding (43 chars) |

The `id` is not secret. The `key` never reaches the server.

## Key derivation

```
enc  = HKDF-SHA256(ikm = key, salt = "envclip/v1", info = "enc",  L = 32)
auth = HKDF-SHA256(ikm = key, salt = "envclip/v1", info = "auth", L = 32)
auth_hash = hex(SHA-256(auth))
```

`auth` proves knowledge of the key without revealing it: the server can check
it against `auth_hash`, but cannot derive `enc` from either value. Without
`auth`, nobody (including someone who guesses an `id`) can read or burn a clip.

## Envelope

```
plaintext = UTF-8 JSON  {"v":1,"vars":[["NAME","value"], …]}
nonce     = random(12)
aad       = "envclip:v1:" + id
blob      = base64url( 0x01 ‖ nonce ‖ AES-256-GCM(enc, nonce, plaintext, aad) )
```

The leading byte is the envelope version. Binding `id` as associated data
stops a ciphertext from being replayed under a different id.

Variable names match `[A-Za-z_][A-Za-z0-9_]*`. Values are arbitrary UTF-8
without NUL bytes.

## HTTP API

All bodies are JSON. Errors are `{"error": "<code>"}`.

| method | path | body | success |
| ------ | ---- | ---- | ------- |
| `POST`   | `/api/clips`           | `{id, auth_hash, blob, ttl}` | `201 {id, expires_at}` |
| `GET`    | `/api/clips/:id`       | —                            | `200 {status:"sealed", expires_at}` |
| `POST`   | `/api/clips/:id/open`  | `{auth}`                     | `200 {blob}` — the row is deleted |
| `DELETE` | `/api/clips/:id`       | `{auth}`                     | `204` |

* `ttl` is in seconds, `60 … 604800` (1 minute to 7 days).
* `blob` is at most 64 KiB.
* `expires_at` is a Unix timestamp in seconds.
* `open` is a single `DELETE … WHERE id = ? AND auth_hash = ? AND expires_at > ? RETURNING blob`
  statement, so exactly one caller can ever receive the blob.
* Unknown id, expired clip, already opened clip and wrong key all return
  `404 {"error":"not_found"}`. The server does not reveal which one it was.
* Non-`GET` API calls are rate limited per IP (`429 {"error":"rate_limited"}`).
* A cron trigger removes expired rows every hour.

## Delivery endpoints

| path | purpose |
| ---- | ------- |
| `/s/:id` | `curl`/`wget` get a POSIX shell script that installs the CLI and runs `envclip open`. Browsers get a page explaining what the link is. |
| `/install.sh` | install the CLI only |
| `/dl/:asset` | release binaries and `.sha256` files, proxied from GitHub Releases |

The share one-liner is

```sh
curl -fsSL https://<host>/s/<id> | sh -s -- <key>
```

`curl` only fetches `/s/<id>`; the key is an argument to the local `sh`, so it
never travels to the server.

## Agent-side storage

`envclip open` decrypts in memory and hands the variables to a background
`envclip` process (the *agent*) over a Unix socket at

```
$ENVCLIP_SOCK  or  $XDG_RUNTIME_DIR/envclip/agent.sock  or  ~/.envclip/agent.sock
```

The socket directory is `0700` and the socket `0600`. Nothing is written to
disk. The agent exits after its TTL (default 12 h) or on `envclip clear`, and
zeroes its memory on the way out.

Socket requests and responses are one JSON object per line:

```
{"op":"put","vars":[["NAME","value"]]}   → {"ok":true}
{"op":"get","names":["NAME"]}            → {"ok":true,"vars":[["NAME","value"]]}
{"op":"list"}                            → {"ok":true,"names":["NAME"],"pid":1,"expires_at":0}
{"op":"clear"}                           → {"ok":true}
```
