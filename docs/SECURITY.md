# Security model

envclip is built for one job: moving a handful of secrets from a person to
an agent sandbox without leaving them in a chat transcript or on a server.
This page states what that does and does not cover.

## What the parties can see

| party | sees | cannot see |
| ----- | ---- | ---------- |
| envclip server / Cloudflare | `id`, ciphertext, `SHA-256(auth)`, size, timing, IPs; `auth` at open time | the key, the encryption key, plaintext |
| chat transcript / AI provider | link and key | plaintext, as long as the ciphertext is already burned and the agent does not print values |
| the agent sandbox | plaintext, by design | — |

`auth` and the encryption key are separate HKDF outputs of the clip key.
Seeing `auth` (or its hash) gives no information about the encryption key.

## Guarantees

- **Confidentiality from the server.** Encryption and decryption happen only
  on the two ends. AES-256-GCM authenticates the ciphertext and binds it to
  its `id`.
- **Single delivery.** Opening is one `DELETE … RETURNING` statement on D1,
  which runs it atomically. Two concurrent opens cannot both succeed.
- **Expiry.** A clip that is never opened is unreadable after its TTL and is
  removed by an hourly cron.
- **Only key holders can burn.** Opening and revoking require `auth`, so
  someone who guesses an `id` can neither read nor destroy the clip.
- **No disk.** The receiving CLI keeps plaintext in an in-memory agent behind
  a `0600` Unix socket inside a `0700` directory, and zeroes it on `clear` or
  expiry.

## Limits

- **The agent can read what you give it.** That is the point. envclip stops
  secrets from ending up in the chat by accident; it does not stop an agent
  that decides to print them. Commands run via `envclip run` see the values
  and may log them. Grant scoped, short-lived credentials.
- **The key is in the transcript.** Once the agent has opened the clip the
  key is useless: the ciphertext is gone. Before that, anyone who can read
  the transcript *and* reach the server first could open it instead. The
  agent would then get "not found", which is your signal to revoke and
  rotate. Keep TTLs short; `10m` is plenty for a live session.
- **Same-user processes in the sandbox** can connect to the agent socket.
- **The web UI is only as trustworthy as the origin serving it.** A
  compromised deployment could serve JavaScript that leaks what you type.
  The page uses a strict CSP and no third-party code, but for the highest
  assurance send from the CLI, whose binary you can verify.
- **Metadata is visible**: when clips are created and opened, from which IPs,
  and their approximate size.

## Reporting

Please open a private security advisory on the GitHub repository.
