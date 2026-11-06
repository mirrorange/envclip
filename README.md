# envclip_

**Hand secrets to your agent. Not to the chat.**

[中文](README.zh-CN.md) · [Protocol](docs/PROTOCOL.md) · [Security](docs/SECURITY.md)

Cloud agents (Claude Managed Agents and others) run in sandboxes you cannot
reach. When a task needs a token, a database URL or a deploy key, the usual
option is to paste it into the chat, where it stays in the transcript
forever.

envclip seals environment variables on your machine and lets the sandbox
open them **once**:

- **End-to-end encrypted.** AES-256-GCM in your browser or CLI. The server
  stores ciphertext and nothing it can decrypt.
- **Burn after reading.** The first successful open deletes the ciphertext
  in the same SQL statement that returns it.
- **No echo.** Plaintext stays in the memory of a small `envclip` process.
  `envclip run -- cmd` injects it into a command without printing it.
- **Edge native.** One Cloudflare Worker, one D1 table, static assets. No
  servers to run.
- **One static binary.** About 2 MB, no runtime dependencies, Linux and macOS.

```
 you                        envclip edge               agent sandbox
 ───                        ────────────               ─────────────
 seal(vars) ── ciphertext ─▶ D1 row
 link + key ─────────────────────────────────────────▶ curl link | sh -s -- key
                            D1 row ◀── open ──────────  envclip open
                            (deleted) ── ciphertext ──▶ decrypt in memory
                                                       envclip run -- npm test
```

## Send

**Browser.** Open your envclip site, add variables or paste a `.env`,
pick an expiry, press **seal**. Copy the generated message to your agent.

**CLI.**

```sh
curl -fsSL https://<your-envclip>/install.sh | sh

envclip push API_TOKEN DATABASE_URL     # read from the current shell
envclip push -f .env.staging --ttl 10m  # or from a .env file
envclip push                            # or type them; values are hidden
```

`push` prints the link and the key on stderr and a ready-to-paste message on
stdout, so `envclip push API_TOKEN | pbcopy` puts the message straight on
your clipboard.

## Receive (what the agent does)

```sh
curl -fsSL https://<your-envclip>/s/<id> | sh -s -- <key>
```

That one line downloads the CLI (verified against its SHA-256), fetches and
burns the ciphertext, decrypts it, and hands the variables to the background
agent. `curl` only requests `/s/<id>`; the key is an argument to the local
`sh` and never reaches the server.

Then, in any later shell:

```sh
envclip run -- ./deploy.sh              # variables injected, nothing printed
eval "$(envclip env)" && git push       # or export them into this shell
envclip ls                              # names only
envclip clear                           # wipe and stop the agent
```

`envclip env` refuses to write to a terminal; it is meant for `eval`.

## CLI reference

| command | |
| ------- | - |
| `envclip push [NAME…] [-f FILE] [--ttl DUR]` | seal variables, print link, key and agent message |
| `envclip open <LINK\|ID> [--key KEY] [--hold DUR]` | fetch, burn, decrypt, hold in memory |
| `envclip run [NAME…] -- <cmd>` | exec `cmd` with held variables |
| `envclip env [NAME…]` | `export` lines for `eval` |
| `envclip ls` | list held names |
| `envclip clear` | wipe memory, stop the agent |
| `envclip revoke <LINK\|ID> [--key KEY]` | delete a clip before it is opened |

| option / env | |
| ------------ | - |
| `--server URL` · `ENVCLIP_SERVER` · `~/.config/envclip/server` | which envclip to talk to |
| `--ttl DUR` | server-side lifetime, `1m` … `7d`, default `1h` |
| `--hold DUR` | how long the agent keeps opened variables, default `12h` |
| `--lang en\|zh` · `ENVCLIP_LANG` · `LANG` | interface language |
| `ENVCLIP_SOCK` | agent socket path |

The key is read from `--key`, else from stdin when it is piped, else from a
hidden prompt.

## Self-host on Cloudflare

```sh
cd worker
npm ci
npx wrangler d1 create envclip          # paste the database_id into wrangler.jsonc
npm run db:migrate                      # apply migrations/ to the remote D1
npm run deploy
```

Then, in the GitHub repository:

1. Under *Settings → Secrets and variables → Actions → Variables*, set
   `ENVCLIP_SERVER` to your deployed URL. Release builds use it as the
   default server for `envclip push`.
2. Push a tag such as `v0.1.0`. The release workflow builds static binaries
   for `x86_64`/`aarch64` Linux (musl) and macOS and publishes them with
   `.sha256` files.

The Worker serves them under `/dl/…` by proxying GitHub Releases (see
`RELEASE_BASE` in `wrangler.jsonc`), so sandboxes only need to reach your
envclip host.

## Layout

```
cli/            Rust CLI: crypto, API client, in-memory agent, en/zh messages
worker/src/     Worker: API, installer script, binary proxy
worker/public/  Web UI: vanilla JS, WebCrypto, en/zh; no build step
worker/migrations/  D1 schema
testdata/       envelope test vector shared by both implementations
docs/           protocol and threat model
```

## Develop

```sh
cargo test                              # CLI, includes the shared vector
cd worker && npm ci && npm test         # web crypto and .env parser
npm run db:migrate:local && npm run dev # http://localhost:8787
ENVCLIP_SERVER=http://localhost:8787 cargo run -- push FOO
```

## License

[MIT](LICENSE)
