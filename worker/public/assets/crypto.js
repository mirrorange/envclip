// envclip envelope v1 — see docs/PROTOCOL.md. Runs in browsers and Node ≥ 20.

const te = new TextEncoder();
const td = new TextDecoder("utf-8", { fatal: true });

export const ID_ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz";
export const NAME_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;
const SALT = te.encode("envclip/v1");
const VERSION = 1;

export function b64url(bytes) {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function fromB64url(s) {
  if (!/^[A-Za-z0-9_-]*$/.test(s)) throw new Error("bad base64url");
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(b64 + "=".repeat((4 - (b64.length % 4)) % 4));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
const random = (n) => crypto.getRandomValues(new Uint8Array(n));

export function newId() {
  // 32 divides 256, so masking keeps the distribution uniform
  return [...random(12)].map((b) => ID_ALPHABET[b & 31]).join("");
}

export const newKey = () => b64url(random(32));

function keyBytes(key) {
  const k = fromB64url(key.trim());
  if (k.length !== 32) throw new Error("bad key");
  return k;
}

async function derive(key, info) {
  const ikm = await crypto.subtle.importKey("raw", keyBytes(key), "HKDF", false, ["deriveBits"]);
  const bits = await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: SALT, info: te.encode(info) },
    ikm,
    256,
  );
  return new Uint8Array(bits);
}

async function encKey(key, usage) {
  return crypto.subtle.importKey("raw", await derive(key, "enc"), "AES-GCM", false, [usage]);
}

export async function authToken(key) {
  return b64url(await derive(key, "auth"));
}

export async function authHash(key) {
  return hex(await crypto.subtle.digest("SHA-256", await derive(key, "auth")));
}

const aad = (id) => te.encode(`envclip:v1:${id}`);

/** vars: [[name, value], …] → { id, key, auth_hash, blob } */
export async function seal(vars, { id = newId(), key = newKey(), nonce = random(12) } = {}) {
  for (const [name, value] of vars) {
    if (!NAME_RE.test(name)) throw new Error(`bad name: ${name}`);
    if (typeof value !== "string" || value.includes("\0")) throw new Error(`bad value: ${name}`);
  }
  const plain = te.encode(JSON.stringify({ v: 1, vars }));
  const ct = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: "AES-GCM", iv: nonce, additionalData: aad(id) },
      await encKey(key, "encrypt"),
      plain,
    ),
  );
  const out = new Uint8Array(1 + nonce.length + ct.length);
  out[0] = VERSION;
  out.set(nonce, 1);
  out.set(ct, 1 + nonce.length);
  return { id, key, auth_hash: await authHash(key), blob: b64url(out) };
}

/** → [[name, value], …] */
export async function unseal(id, key, blob) {
  const raw = fromB64url(blob);
  if (raw.length < 1 + 12 + 16 || raw[0] !== VERSION) throw new Error("bad envelope");
  const plain = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: raw.subarray(1, 13), additionalData: aad(id) },
    await encKey(key, "decrypt"),
    raw.subarray(13),
  );
  const doc = JSON.parse(td.decode(plain));
  if (doc.v !== 1 || !Array.isArray(doc.vars)) throw new Error("bad payload");
  return doc.vars;
}
