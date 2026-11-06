// POSIX shell installer, served at /install.sh and (pre-bound to a drop) at
// /c/<id> for non-browser clients. Agents run:
//
//   curl -fsSL https://<host>/c/<id> | sh -s -- <key>
//
// The key is handed to `envclip claim` on stdin, never as an argument.

const q = (s: string) => `'${s.replace(/'/g, `'\\''`)}'`;

export function installScript(origin: string, drop = ""): string {
  return `#!/bin/sh
# envclip installer — ${origin}
#   curl -fsSL ${origin}/install.sh | sh                 install only
#   curl -fsSL ${origin}/c/<id> | sh -s -- <key>         install + claim a drop
set -eu

ORIGIN=${q(origin)}
DROP=${q(drop)}

say() { printf '%s\\n' "$*" >&2; }
die() { say "envclip: $*"; exit 1; }

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=darwin ;;
  *) die "unsupported OS: $(uname -s)" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "unsupported architecture: $(uname -m)" ;;
esac

fetch() {
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then wget -qO "$2" "$1"
  else die "need curl or wget"
  fi
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

bin=\${ENVCLIP_BIN:-}
if [ -z "$bin" ] && command -v envclip >/dev/null 2>&1; then bin=$(command -v envclip); fi

if [ -z "$bin" ]; then
  if [ -n "\${ENVCLIP_INSTALL_DIR:-}" ]; then dir=$ENVCLIP_INSTALL_DIR
  elif [ -w /usr/local/bin ]; then dir=/usr/local/bin
  else dir=$HOME/.local/bin
  fi
  mkdir -p "$dir"
  asset="envclip-$os-$arch"
  tmp=$(mktemp "\${TMPDIR:-/tmp}/envclip.XXXXXX")
  trap 'rm -f "$tmp" "$tmp.sha256"' EXIT
  say "· downloading $asset"
  fetch "$ORIGIN/dl/$asset" "$tmp" || die "download failed"
  if fetch "$ORIGIN/dl/$asset.sha256" "$tmp.sha256" 2>/dev/null; then
    want=$(cut -d' ' -f1 <"$tmp.sha256")
    got=$(sha256 "$tmp")
    if [ -n "$got" ] && [ "$got" != "$want" ]; then die "checksum mismatch for $asset"; fi
  fi
  chmod 755 "$tmp"
  mv "$tmp" "$dir/envclip"
  bin=$dir/envclip
  say "✓ installed $bin"
  case ":$PATH:" in
    *":$dir:"*) ;;
    *) say "  note: $dir is not on PATH — use $bin, or: export PATH=\\"$dir:\\$PATH\\"" ;;
  esac
fi

if [ -n "$DROP" ]; then
  key=\${1:-\${ENVCLIP_KEY:-}}
  if [ -z "$key" ]; then
    say "next: $bin claim $ORIGIN/c/$DROP --key <KEY>"
    exit 0
  fi
  printf '%s' "$key" | ENVCLIP_KEY= ENVCLIP_SERVER="$ORIGIN" "$bin" claim "$ORIGIN/c/$DROP"
else
  say "✓ envclip is ready — run: $bin help"
fi
`;
}
