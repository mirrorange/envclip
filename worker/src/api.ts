export interface Env {
  DB: D1Database;
  ASSETS: Fetcher;
  RELEASE_BASE: string;
  LIMITER?: RateLimit;
}

const ID_RE = /^[0-9abcdefghjkmnpqrstvwxyz]{12}$/;
const HASH_RE = /^[0-9a-f]{64}$/;
const B64URL_RE = /^[A-Za-z0-9_-]+$/;

export const MIN_TTL = 60;
export const MAX_TTL = 7 * 24 * 3600;
export const MAX_BLOB = 64 * 1024;

const now = () => Math.floor(Date.now() / 1000);

export function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
  });
}

const fail = (status: number, error: string) => json({ error }, status);

export const isId = (id: string) => ID_RE.test(id);

async function body(req: Request): Promise<Record<string, unknown> | null> {
  if (Number(req.headers.get("content-length") ?? 0) > MAX_BLOB * 2) return null;
  try {
    const v = await req.json();
    return v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function b64urlDecode(s: string): Uint8Array {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(b64 + "=".repeat((4 - (b64.length % 4)) % 4));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

async function authHash(auth: unknown): Promise<string | null> {
  if (typeof auth !== "string" || auth.length !== 43 || !B64URL_RE.test(auth)) return null;
  const digest = await crypto.subtle.digest("SHA-256", b64urlDecode(auth));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function create(req: Request, env: Env): Promise<Response> {
  const b = await body(req);
  if (!b) return fail(400, "bad_request");
  const { id, auth_hash, blob } = b;
  const ttl = b.ttl ?? 3600;
  if (typeof id !== "string" || !isId(id)) return fail(400, "bad_id");
  if (typeof auth_hash !== "string" || !HASH_RE.test(auth_hash)) return fail(400, "bad_auth_hash");
  if (typeof blob !== "string" || !B64URL_RE.test(blob)) return fail(400, "bad_blob");
  if (blob.length > MAX_BLOB) return fail(413, "too_large");
  if (typeof ttl !== "number" || !Number.isInteger(ttl) || ttl < MIN_TTL || ttl > MAX_TTL)
    return fail(400, "bad_ttl");

  const t = now();
  const expires_at = t + ttl;
  const res = await env.DB.prepare(
    "INSERT INTO clips (id, auth_hash, blob, created_at, expires_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT (id) DO NOTHING",
  )
    .bind(id, auth_hash, blob, t, expires_at)
    .run();
  if (!res.meta.changes) return fail(409, "id_taken");
  return json({ id, expires_at }, 201);
}

async function status(id: string, env: Env): Promise<Response> {
  const row = await env.DB.prepare("SELECT expires_at FROM clips WHERE id = ? AND expires_at > ?")
    .bind(id, now())
    .first<{ expires_at: number }>();
  if (!row) return fail(404, "not_found");
  return json({ status: "sealed", expires_at: row.expires_at });
}

async function open(req: Request, id: string, env: Env): Promise<Response> {
  const b = await body(req);
  const hash = b && (await authHash(b.auth));
  if (!hash) return fail(400, "bad_auth");
  // a single statement: exactly one caller can ever receive the blob
  const row = await env.DB.prepare(
    "DELETE FROM clips WHERE id = ? AND auth_hash = ? AND expires_at > ? RETURNING blob",
  )
    .bind(id, hash, now())
    .first<{ blob: string }>();
  if (!row) return fail(404, "not_found");
  return json({ blob: row.blob });
}

async function revoke(req: Request, id: string, env: Env): Promise<Response> {
  const b = await body(req);
  const hash = b && (await authHash(b.auth));
  if (!hash) return fail(400, "bad_auth");
  const res = await env.DB.prepare("DELETE FROM clips WHERE id = ? AND auth_hash = ?").bind(id, hash).run();
  if (!res.meta.changes) return fail(404, "not_found");
  return new Response(null, { status: 204 });
}

export async function api(req: Request, env: Env, path: string): Promise<Response> {
  if (req.method !== "GET" && env.LIMITER) {
    const ip = req.headers.get("cf-connecting-ip") ?? "local";
    const { success } = await env.LIMITER.limit({ key: ip });
    if (!success) return fail(429, "rate_limited");
  }
  if (path === "/api/clips") {
    return req.method === "POST" ? create(req, env) : fail(405, "method_not_allowed");
  }
  const m = path.match(/^\/api\/clips\/([^/]+)(\/open)?$/);
  if (!m) return fail(404, "no_route");
  const [, id, isOpen] = m;
  if (!isId(id)) return fail(404, "not_found");
  if (isOpen) return req.method === "POST" ? open(req, id, env) : fail(405, "method_not_allowed");
  if (req.method === "GET") return status(id, env);
  if (req.method === "DELETE") return revoke(req, id, env);
  return fail(405, "method_not_allowed");
}

export async function sweep(env: Env): Promise<void> {
  await env.DB.prepare("DELETE FROM clips WHERE expires_at <= ?").bind(now()).run();
}
