// POSIX sh installer. With an id it also opens the clip:
//   curl -fsSL https://host/s/<id> | sh -s -- <key>
// The key is an argument to the local sh and never reaches the server.

const q = (s: string) => `'${s.replace(/'/g, `'\\''`)}'`;

export function installScript(origin: string, id?: string): string {
  const open = id
    ? `
  link="$host/s/"${q(id)}
  if [ $# -ge 1 ]; then
    printf '%s\\n' "$1" | "$bin" open "$link"
  elif [ -r /dev/tty ] && [ -t 2 ]; then
    "$bin" open "$link" </dev/tty
  else
    die "missing key. usage: curl -fsSL $link | sh -s -- <key>"
  fi`
    : `
  say "ready. run: envclip --help"`;

  return `#!/bin/sh
# envclip installer · https://github.com/mirrorange/envclip
# wrapped in a function so a truncated download never runs half a script
set -eu

main() {
  host=${q(origin)}

  case "$(uname -s)" in
    Linux) os=unknown-linux-musl ;;
    Darwin) os=apple-darwin ;;
    *) die "unsupported OS: $(uname -s)" ;;
  esac
  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "unsupported architecture: $(uname -m)" ;;
  esac
  asset="envclip-$arch-$os"

  bin=$(command -v envclip 2>/dev/null || true)
  if [ -z "$bin" ] || [ "\${ENVCLIP_REINSTALL:-}" = 1 ]; then
    dir="\${ENVCLIP_BIN_DIR:-$HOME/.local/bin}"
    mkdir -p "$dir"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "downloading $asset"
    fetch "$host/dl/$asset" "$tmp/envclip"
    fetch "$host/dl/$asset.sha256" "$tmp/envclip.sha256"
    want=$(cut -d ' ' -f 1 <"$tmp/envclip.sha256")
    if command -v sha256sum >/dev/null 2>&1; then
      got=$(sha256sum "$tmp/envclip" | cut -d ' ' -f 1)
    elif command -v shasum >/dev/null 2>&1; then
      got=$(shasum -a 256 "$tmp/envclip" | cut -d ' ' -f 1)
    else
      die "need sha256sum or shasum to verify the download"
    fi
    [ "$got" = "$want" ] || die "checksum mismatch for $asset"
    chmod 755 "$tmp/envclip"
    mv "$tmp/envclip" "$dir/envclip"
    bin="$dir/envclip"
    say "installed $bin"
    case ":$PATH:" in
      *":$dir:"*) ;;
      *) say "$dir is not on PATH; call $bin directly or add it to PATH" ;;
    esac
  fi

  conf="\${XDG_CONFIG_HOME:-$HOME/.config}/envclip"
  if [ ! -e "$conf/server" ]; then
    mkdir -p "$conf"
    printf '%s\\n' "$host" >"$conf/server"
  fi
${open}
}

say() { printf 'envclip: %s\\n' "$*" >&2; }
die() { say "$*"; exit 1; }
fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --proto '=https,http' "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$2" "$1"
  else
    die "need curl or wget"
  fi
}

main "$@"
`;
}
