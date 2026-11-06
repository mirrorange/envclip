// envclip wire crypto v1 on WebCrypto. Mirrors cli/src/crypto.rs byte for byte;
// see docs/PROTOCOL.md. Runs in browsers and in Node ≥ 20 (globalThis.crypto).

const ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz";
const V1 = new TextEncoder().encode("envclip/v1");
const PAD_BLOCK = 256;
const subtle = globalThis.crypto.subtle;
const te = new TextEncoder();

export function b64url(bytes) {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function unb64url(s) {
  const b = atob(s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4));
  return Uint8Array.from(b, (c) => c.charCodeAt(0));
}

const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");

export function generateSecret() {
  return crypto.getRandomValues(new Uint8Array(16));
}

/** 16 bytes → 26 lowercase Crockford base32 chars. */
export function encodeSecret(bytes) {
  let out = "", acc = 0, bits = 0;
  for (const b of bytes) {
    acc = (acc << 8) | b;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      out += ALPHABET[(acc >> bits) & 31];
    }
    acc &= (1 << bits) - 1;
  }
  if (bits > 0) out += ALPHABET[(acc << (5 - bits)) & 31];
  return out;
}

/** Lenient inverse of encodeSecret; returns null for anything malformed. */
export function parseSecret(str) {
  const s = str.trim().replace(/^ek_/, "").toLowerCase().replace(/[-\s]/g, "")
    .replace(/o/g, "0").replace(/[il]/g, "1");
  if (s.length !== 26) return null;
  const out = [];
  let acc = 0, bits = 0;
  for (const c of s) {
    const v = ALPHABET.indexOf(c);
    if (v < 0) return null;
    acc = (acc << 5) | v;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((acc >> bits) & 0xff);
      acc &= (1 << bits) - 1;
    }
  }
  return acc === 0 && out.length === 16 ? Uint8Array.from(out) : null;
}

async function hkdf(ikm, info) {
  const base = await subtle.importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
  return new Uint8Array(
    await subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt: V1, info: te.encode(info) }, base, 256),
  );
}

/** → { encKey, claimToken, claimHash } */
export async function deriveKeys(secret) {
  const encRaw = await hkdf(secret, "enc");
  const claim = await hkdf(secret, "claim");
  const encKey = await subtle.importKey("raw", encRaw, "AES-GCM", false, ["encrypt", "decrypt"]);
  encRaw.fill(0);
  return { encKey, claimToken: b64url(claim), claimHash: hex(await subtle.digest("SHA-256", claim)) };
}

export async function seal(keys, plaintext) {
  const padded = new Uint8Array(Math.max(1, Math.ceil(plaintext.length / PAD_BLOCK)) * PAD_BLOCK).fill(0x20);
  padded.set(plaintext);
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(
    await subtle.encrypt({ name: "AES-GCM", iv: nonce, additionalData: V1 }, keys.encKey, padded),
  );
  padded.fill(0);
  const out = new Uint8Array(1 + 12 + ct.length);
  out[0] = 1;
  out.set(nonce, 1);
  out.set(ct, 13);
  return b64url(out);
}

export async function open(keys, sealed) {
  const raw = unb64url(sealed);
  if (raw.length < 29 || raw[0] !== 1) throw new Error("bad envelope");
  return new Uint8Array(
    await subtle.decrypt({ name: "AES-GCM", iv: raw.slice(1, 13), additionalData: V1 }, keys.encKey, raw.slice(13)),
  );
}

/** [[name, value], ...] → sealed payload bytes, exactly like the CLI. */
export function encodePayload(env) {
  return te.encode(JSON.stringify({ v: 1, env }));
}

export const validName = (n) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(n);

/** Minimal .env parser, same rules as cli/src/dotenv.rs. Throws on bad input. */
export function parseDotenv(src) {
  const out = new Map();
  const lines = src.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    let t = lines[i].trimStart();
    if (!t || t.startsWith("#")) continue;
    if (t.startsWith("export ")) t = t.slice(7).trimStart();
    const eq = t.indexOf("=");
    if (eq < 0) throw new Error(`line ${i + 1}: KEY=VALUE`);
    const k = t.slice(0, eq).trim();
    if (!validName(k)) throw new Error(`line ${i + 1}: ${k}`);
    let rest = t.slice(eq + 1).trimStart();
    let v;
    if (rest.startsWith("'")) {
      const end = rest.indexOf("'", 1);
      if (end < 0) throw new Error(`line ${i + 1}: '`);
      v = rest.slice(1, end);
    } else if (rest.startsWith('"')) {
      v = "";
      let cur = rest.slice(1), closed = false;
      for (;;) {
        for (let j = 0; j < cur.length; j++) {
          const c = cur[j];
          if (c === '"') { closed = true; break; }
          if (c === "\\" && j + 1 < cur.length) {
            const e = cur[++j];
            v += e === "n" ? "\n" : e === "t" ? "\t" : e === "r" ? "\r" : e;
          } else v += c;
        }
        if (closed) break;
        if (++i >= lines.length) throw new Error(`line ${i}: "`);
        v += "\n";
        cur = lines[i];
      }
    } else {
      const hash = rest.indexOf(" #");
      v = (hash >= 0 ? rest.slice(0, hash) : rest).trimEnd();
    }
    out.set(k, v);
  }
  return [...out.entries()];
}
