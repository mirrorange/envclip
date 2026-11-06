// The envclip drop API. The server is a dumb, forgetful mailbox: it stores
// ciphertext plus SHA-256(claim_token) and hands the ciphertext out exactly
// once to whoever proves knowledge of the claim token.

import type { Env } from "./index";

const MIN_TTL = 60;
const MAX_TTL = 7 * 24 * 3600;
const MAX_CIPHERTEXT = 90_000; // ≈ 64 KiB of sealed bytes, base64url encoded
const MAX_BODY = 128 * 1024;
const ID_ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz";
const ID_LEN = 12;

const B64URL = /^[A-Za-z0-9_-]+$/;
const HEX64 = /^[0-9a-f]{64}$/;
const ID_RE = /^[0-9a-z]{1,64}$/;

export function json(data: unknown, status = 200, headers: HeadersInit = {}): Response {
  return new Response(data === null ? null : JSON.stringify(data), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      "x-content-type-options": "nosniff",
      "referrer-policy": "no-referrer",
      ...headers,
    },
  });
}

const fail = (status: number, error: string) => json({ error }, status);
const notFound = () => fail(404, "not_found");
const now = () => Math.floor(Date.now() / 1000);

function newId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(ID_LEN));
  let id = "";
  for (const b of bytes) id += ID_ALPHABET[b & 31];
  return id;
}

async function sha256hex(s: string): Promise<string> {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Hash of the bearer token, or null when the header is missing/malformed. */
async function claimHash(req: Request): Promise<string | null> {
  const m = /^Bearer ([A-Za-z0-9_-]{43})$/.exec(req.headers.get("authorization") ?? "");
  if (!m) return null;
  // The token is base64url(32 bytes); hash the raw bytes like the clients do.
  const raw = Uint8Array.from(atob(m[1].replace(/-/g, "+").replace(/_/g, "/") + "="), (c) => c.charCodeAt(0));
  const d = await crypto.subtle.digest("SHA-256", raw);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function limited(limiter: RateLimit | undefined, req: Request): Promise<boolean> {
  if (!limiter) return false;
  // Key on a hash so raw client IPs never reach the limiter's storage.
  const ip = req.headers.get("cf-connecting-ip") ?? "local";
  const { success } = await limiter.limit({ key: await sha256hex(ip) });
  return !success;
}

async function create(req: Request, env: Env): Promise<Response> {
  if (await limited(env.CREATE_LIMIT, req)) return fail(429, "rate_limited");
  const len = Number(req.headers.get("content-length") ?? "0");
  if (len > MAX_BODY) return fail(413, "too_large");
  const text = await req.text();
  if (text.length > MAX_BODY) return fail(413, "too_large");

  let body: { ciphertext?: unknown; claim_hash?: unknown; ttl?: unknown };
  try {
    body = JSON.parse(text);
  } catch {
    return fail(400, "invalid_json");
  }
  const { ciphertext, claim_hash } = body;
  const ttl = body.ttl ?? 24 * 3600;
  if (typeof ciphertext !== "string" || !B64URL.test(ciphertext) || ciphertext.length < 40)
    return fail(400, "invalid_ciphertext");
  if (ciphertext.length > MAX_CIPHERTEXT) return fail(413, "too_large");
  if (typeof claim_hash !== "string" || !HEX64.test(claim_hash)) return fail(400, "invalid_claim_hash");
  if (typeof ttl !== "number" || !Number.isInteger(ttl) || ttl < MIN_TTL || ttl > MAX_TTL)
    return fail(400, "invalid_ttl");

  const created = now();
  const expires_at = created + ttl;
  for (let attempt = 0; attempt < 3; attempt++) {
    const id = newId();
    const r = await env.DB.prepare(
      "INSERT INTO drops (id, ciphertext, claim_hash, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (id) DO NOTHING",
    )
      .bind(id, ciphertext, claim_hash, created, expires_at)
      .run();
    if (r.meta.changes === 1) return json({ id, expires_at }, 201);
  }
  return fail(503, "id_exhausted");
}

/** Atomically read-and-delete: the ciphertext exists for exactly one claim. */
async function claim(req: Request, env: Env, id: string): Promise<Response> {
  if (await limited(env.CLAIM_LIMIT, req)) return fail(429, "rate_limited");
  const hash = await claimHash(req);
  if (!hash) return fail(401, "unauthorized");
  const row = await env.DB.prepare(
    "DELETE FROM drops WHERE id = ?1 AND claim_hash = ?2 AND expires_at > ?3 RETURNING ciphertext",
  )
    .bind(id, hash, now())
    .first<{ ciphertext: string }>();
  return row ? json({ ciphertext: row.ciphertext }) : notFound();
}

async function status(req: Request, env: Env, id: string): Promise<Response> {
  const hash = await claimHash(req);
  if (!hash) return fail(401, "unauthorized");
  const row = await env.DB.prepare(
    "SELECT created_at, expires_at FROM drops WHERE id = ?1 AND claim_hash = ?2 AND expires_at > ?3",
  )
    .bind(id, hash, now())
    .first<{ created_at: number; expires_at: number }>();
  return row ? json(row) : notFound();
}

async function revoke(req: Request, env: Env, id: string): Promise<Response> {
  const hash = await claimHash(req);
  if (!hash) return fail(401, "unauthorized");
  const r = await env.DB.prepare("DELETE FROM drops WHERE id = ?1 AND claim_hash = ?2")
    .bind(id, hash)
    .run();
  return r.meta.changes ? json(null, 204) : notFound();
}

export async function handleApi(req: Request, env: Env, path: string): Promise<Response> {
  if (path === "/api/v1/drops") {
    return req.method === "POST" ? create(req, env) : fail(405, "method_not_allowed");
  }
  const m = /^\/api\/v1\/drops\/([^/]+)(\/claim)?$/.exec(path);
  if (!m || !ID_RE.test(m[1])) return notFound();
  const [, id, isClaim] = m;
  if (isClaim) return req.method === "POST" ? claim(req, env, id) : fail(405, "method_not_allowed");
  if (req.method === "GET") return status(req, env, id);
  if (req.method === "DELETE") return revoke(req, env, id);
  return fail(405, "method_not_allowed");
}

export async function purgeExpired(env: Env): Promise<void> {
  await env.DB.prepare("DELETE FROM drops WHERE expires_at <= ?1").bind(now()).run();
}
