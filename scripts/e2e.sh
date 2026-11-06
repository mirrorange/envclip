#!/usr/bin/env bash
# End-to-end test: local Worker + D1 (wrangler dev), the Rust CLI, the
# installer script and the browser crypto (via Node) talking to each other.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
port=${PORT:-8799}
work=$(mktemp -d)
chmod 700 "$work"
export ENVCLIP_SERVER="http://127.0.0.1:$port" ENVCLIP_SOCK="$work/agent.sock"
export NO_PROXY=127.0.0.1 no_proxy=127.0.0.1 NO_COLOR=1 ENVCLIP_LANG=en

pass=0
ok() { pass=$((pass + 1)); printf '  \033[32m✓\033[0m %s\n' "$1"; }
die() { printf '  \033[31m✗\033[0m %s\n' "$1"; [ -f "$work/wrangler.log" ] && tail -20 "$work/wrangler.log"; exit 1; }
field() { python3 -c "import json,sys; print(json.load(sys.stdin)['$1'])"; }

echo "· building cli"
cargo build -q --manifest-path "$root/cli/Cargo.toml"
B="$root/cli/target/debug/envclip"

echo "· starting worker on :$port"
cd "$root/worker"
npx wrangler d1 migrations apply envclip --local --persist-to "$work/state" >/dev/null 2>&1
npx wrangler dev --port "$port" --ip 127.0.0.1 --persist-to "$work/state" >"$work/wrangler.log" 2>&1 &
wpid=$!
cleanup() { "$B" clear >/dev/null 2>&1 || true; kill "$wpid" 2>/dev/null || true; rm -rf "$work"; }
trap cleanup EXIT
for _ in $(seq 60); do curl -fs "$ENVCLIP_SERVER/" >/dev/null 2>&1 && break; sleep 0.5; done
curl -fs "$ENVCLIP_SERVER/" >/dev/null || die "worker did not start"

echo "· cli → cli"
export E2E_SECRET='p@ss "w0rd" $HOME
line2'
out=$("$B" send E2E_SECRET --ttl 10m --json)
url=$(field url <<<"$out")
key=$(field key <<<"$out")
"$B" status "$url" --key "$key" 2>&1 | grep -q "waiting" && ok "status: waiting" || die "status"
"$B" claim "$url" --key 00000000000000000000000000 2>/dev/null && die "wrong key claimed"
"$B" status "$url" --key "$key" 2>&1 | grep -q "waiting" && ok "wrong key does not burn the drop" || die "burned"
unset E2E_SECRET
printf '%s' "$key" | "$B" claim "$url" 2>/dev/null && ok "claim (key on stdin)" || die "claim"
printf '%s' "$key" | "$B" claim "$url" 2>/dev/null && die "second claim succeeded"
ok "second claim refused (burned)"
[ "$("$B" ls 2>/dev/null)" = "E2E_SECRET" ] && ok "ls shows names only" || die "ls"
"$B" env | grep -q 'line2' && die "env output leaked plaintext"
ok "env output holds no plaintext"
eval "$("$B" env)"
[ "$E2E_SECRET" = 'p@ss "w0rd" $HOME
line2' ] && ok "eval env restores exact value" || die "eval value mismatch"
unset E2E_SECRET
got=$("$B" run -- sh -c 'printf %s "$E2E_SECRET"')
[ "$got" = 'p@ss "w0rd" $HOME
line2' ] && ok "run injects exact value" || die "run value mismatch"

echo "· web crypto → installer → cli"
read -r wurl wkey < <(node test/send.mjs "$ENVCLIP_SERVER" WEB_TOKEN=from-browser GIT_PASS=x)
curl -fsSL "$wurl" | ENVCLIP_BIN="$B" sh -s -- "$wkey" 2>/dev/null && ok "installer claims the drop" || die "installer"
[ "$("$B" get WEB_TOKEN)" = "from-browser" ] && ok "browser-sealed value decrypts in rust" || die "interop"
[ "$("$B" ls 2>/dev/null | wc -l)" -eq 3 ] && ok "agent merges drops" || die "merge"

echo "· revoke"
out=$(printf 'X=1\n' | "$B" send --json)
url=$(field url <<<"$out"); key=$(field key <<<"$out")
"$B" revoke "$url" --key "$key" 2>/dev/null && ok "revoke" || die "revoke"
"$B" claim "$url" --key "$key" 2>/dev/null && die "claimed after revoke"
ok "revoked drop cannot be claimed"

echo "· api hardening"
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$ENVCLIP_SERVER/api/v1/drops" -d '{"ciphertext":"x","claim_hash":"y"}')
[ "$code" = 400 ] && ok "rejects malformed drops" || die "malformed accepted ($code)"
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$ENVCLIP_SERVER/api/v1/drops/abc/claim")
[ "$code" = 401 ] && ok "claim requires a token" || die "claim without token ($code)"

"$B" clear >/dev/null 2>&1
"$B" ls >/dev/null 2>&1 && die "agent still running after clear"
ok "clear stops the agent"

echo "all $pass checks passed"
