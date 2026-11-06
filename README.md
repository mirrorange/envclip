<div align="center">

```
envclip_
```

**Hand secrets to your AI agent. Not to the chat.**

End-to-end encrypted · burn after reading · never echoed

[English](README.md) · [简体中文](README.zh-CN.md)

</div>

---

Cloud agents (Claude Managed Agents, hosted coding agents, …) run in sandboxes
you can't SSH into. When a task needs a credential — a private API token, a
password for a self-hosted Git remote — the only channel is usually the chat,
where the secret is stored in the transcript forever.

envclip is a tiny edge-native drop box for environment variables:

- **Zero-knowledge server.** Values are encrypted on your machine (CLI) or in
  your browser (web). The Cloudflare Worker only stores ciphertext.
- **Burn after reading.** A drop can be claimed exactly once, then the
  ciphertext is deleted. Unclaimed drops expire (1 min – 7 days).
- **No echo.** In the sandbox, plaintext lives only in the memory of a small
  envclip agent process. Commands get the values injected; nothing prints them.
- **One static binary.** Rust, ~2 MB, no runtime dependencies.

## How it works

```
 you                                 envclip edge (Workers + D1)              agent sandbox
 ───                                 ───────────────────────────              ─────────────
 envclip send API_TOKEN
   ├─ encrypt with a fresh key ──▶   stores ciphertext + H(claim token)
   └─ prints link + key
 paste link + key into chat ───────────────────────────────────────────────▶  curl -fsSL <link> | sh -s -- <key>
                                     DELETE … RETURNING ciphertext   ◀──────  ├─ installs envclip, claims the drop
                                     (row is gone for good)                   └─ decrypts into agent memory
                                                                              envclip run -- git push
                                                                              eval "$(envclip env)" && ./test.sh
```

1. **Seal** — envclip generates a random 128-bit key, encrypts the variables
   with AES-256-GCM and uploads the ciphertext. The key never leaves you.
2. **Hand off** — you paste the link and the key into the agent chat (envclip
   prints a ready-made message).
3. **Claim** — the agent runs one command. It installs envclip, proves it knows
   the key, receives the ciphertext and the server deletes it atomically.
4. **Use** — the decrypted values stay in envclip's memory. `envclip run`
   injects them into a command; `envclip env` emits shell code that pulls them
   at eval time, so even its output contains no secret.

## Quick start

### Send — from the web

Open your envclip site, add variables (or paste a `.env`), press **seal & upload**,
and copy the message into your agent chat.

### Send — from the CLI

```sh
curl -fsSL https://<your-envclip>/install.sh | sh

# straight from your current shell — no copy/paste of the values at all
envclip send OPENAI_API_KEY GITEA_TOKEN --ttl 1h

# or from a file / stdin / hidden interactive prompts
envclip send -f .env.agent
op read op://vault/db/password | sed 's/^/DB_PASSWORD=/' | envclip send
envclip send
```

`send` prints the link, the key, and a message for the agent:

```text
I'm sharing secrets with you via envclip (end-to-end encrypted, one-time).
Install envclip and load them into memory (run once):

  curl -fsSL https://<your-envclip>/c/7q2m9xk4bd1z | sh -s -- 3fk1v8hy...

Then use them without ever printing them:
  envclip run -- <command>              # inject into a single command
  eval "$(envclip env)" && <command>    # or export into the current shell
`envclip ls` lists the names. Never echo, log or commit the values.
```

### Use — inside the agent sandbox

```sh
envclip run -- git push origin main               # one command, values injected
envclip run GITEA_TOKEN -- ./deploy.sh            # only some variables
eval "$(envclip env)" && curl -H "Authorization: Bearer $API_TOKEN" …
envclip ls                                        # names only
envclip clear                                     # wipe memory, stop the agent
```

> Many agent harnesses start a fresh shell for every command, so prefer
> `envclip run -- …` or `eval "$(envclip env)" && …` within the same command.

## CLI reference

| command | |
|---|---|
| `envclip send [NAME…] [-f FILE] [--ttl 24h] [--server URL] [--json]` | encrypt + upload. `NAME` reads `$NAME` from your shell (prompts hidden if unset). `-f -` reads stdin. |
| `envclip status <link> [--key K]` | is the drop still waiting? |
| `envclip revoke <link> [--key K]` | destroy an unclaimed drop |
| `envclip claim <link> [--key K] [--hold 12h]` | fetch, burn, decrypt into the in-memory agent |
| `envclip run [NAME…] -- <cmd…>` | exec a command with the variables in its environment |
| `envclip env [NAME…] [--shell sh\|fish]` | shell code for `eval`; contains no plaintext |
| `envclip get <NAME>` | print one raw value (for pipes — this *does* print it) |
| `envclip ls` | list loaded names and the agent's remaining lifetime |
| `envclip clear [NAME…]` | forget some variables, or wipe all and stop the agent |

The key is read from `--key`, `$ENVCLIP_KEY`, stdin (when piped) or a hidden
prompt — in that order. Durations accept `90s`, `15m`, `12h`, `7d`.

| env | |
|---|---|
| `ENVCLIP_SERVER` | server for `send` and bare drop IDs |
| `ENVCLIP_KEY` | key for `claim` / `status` / `revoke` |
| `ENVCLIP_LANG` | `en` or `zh` (defaults to `LANG`) |
| `ENVCLIP_SOCK` | agent socket path (default `$XDG_RUNTIME_DIR/envclip/agent.sock` or `$TMPDIR/envclip-<uid>/agent.sock`) |
| `NO_COLOR` | disable colours |

## Security

What each party can see:

| | ciphertext | key | plaintext |
|---|---|---|---|
| envclip server | ✓ (until claimed / expired) | ✗ | ✗ |
| network observer | TLS | ✗ | ✗ |
| AI provider / chat transcript | ✗ | ✓ link + key | ✗ — values are never printed |
| agent sandbox | once | ✓ | ✓ in envclip's memory and in commands you inject into |

- The **server** stores ciphertext and `SHA-256(claim_token)` only. The claim
  token is derived from the key with HKDF, independently of the encryption key,
  so the server can authorise a claim without learning anything that decrypts.
- A **wrong key does not burn** a drop; the claim is a single
  `DELETE … RETURNING` guarded by the claim hash, so exactly one holder of the
  key ever receives the ciphertext.
- The **chat transcript** necessarily contains the link and key — that's how
  the sandbox gets them. They are a one-time capability: once the sandbox has
  claimed, they are worthless. If anyone else used them first, the agent's
  claim fails loudly (`drop not found`) and you know to rotate the secret.
- In the **sandbox**, values live in a detached agent process (non-dumpable,
  core dumps disabled, user-private socket with peer-uid check, scrubbed
  environment, zeroized on exit). envclip prevents accidental disclosure; it
  cannot stop an agent that deliberately runs `echo $TOKEN`. Grant secrets with
  the least privilege and shortest life you can.

Full wire format and threat model: [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Self-hosting on Cloudflare

```sh
cd worker
npm ci
npx wrangler d1 create envclip          # paste the database_id into wrangler.jsonc
npm run migrate:remote
npm run deploy                          # → https://envclip.<you>.workers.dev
```

Then point the CLI at it:

- set the repository variable `ENVCLIP_SERVER` (e.g. `https://envclip.example.com`)
  so release binaries default to your server, and push a `v*` tag — the
  `release` workflow builds static Linux (musl) and macOS binaries that
  `/install.sh` and `/c/<id>` download via `/dl/*`;
- or just `export ENVCLIP_SERVER=https://…` on machines that send.

`RELEASES_URL` in `wrangler.jsonc` controls where `/dl/*` redirects. To deploy
from CI, add `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` secrets and set the
variable `DEPLOY_WORKER=true`.

## Development

```sh
cd cli && cargo test                 # crypto vectors, parsers
cd worker && npm test                # browser crypto against the same vectors
./scripts/e2e.sh                     # wrangler dev + local D1 + CLI + installer + web crypto
```

```
cli/        Rust CLI and in-memory agent
worker/     Cloudflare Worker (API, installer), D1 migrations, static web app in public/
docs/       protocol and threat model
scripts/    end-to-end test
```

## License

MIT
