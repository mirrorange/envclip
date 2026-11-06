import { api, isId, json, sweep, type Env } from "./api";
import { installScript } from "./script";

const ASSET_RE = /^envclip-(x86_64|aarch64)-(unknown-linux-musl|apple-darwin)(\.sha256)?$/;

const SECURITY_HEADERS: Record<string, string> = {
  "content-security-policy":
    "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; " +
    "manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
  "x-content-type-options": "nosniff",
  "referrer-policy": "no-referrer",
  "permissions-policy": "camera=(), microphone=(), geolocation=(), interest-cohort=()",
  "cross-origin-opener-policy": "same-origin",
  "strict-transport-security": "max-age=31536000",
};

function harden(res: Response): Response {
  const out = new Response(res.body, res);
  for (const [k, v] of Object.entries(SECURITY_HEADERS)) out.headers.set(k, v);
  return out;
}

const shell = (body: string) =>
  new Response(body, {
    headers: { "content-type": "text/x-shellscript; charset=utf-8", "cache-control": "no-store" },
  });

const wantsHtml = (req: Request) => (req.headers.get("accept") ?? "").includes("text/html");

async function download(asset: string, env: Env): Promise<Response> {
  if (!ASSET_RE.test(asset)) return json({ error: "not_found" }, 404);
  const upstream = await fetch(`${env.RELEASE_BASE}/${asset}`, {
    cf: { cacheEverything: true, cacheTtl: 300 },
  });
  if (!upstream.ok) return json({ error: "upstream", status: upstream.status }, 502);
  return new Response(upstream.body, {
    headers: {
      "content-type": asset.endsWith(".sha256") ? "text/plain; charset=utf-8" : "application/octet-stream",
      "cache-control": "public, max-age=300",
    },
  });
}

async function route(req: Request, env: Env): Promise<Response> {
  const url = new URL(req.url);
  const path = url.pathname;

  if (path.startsWith("/api/")) return api(req, env, path);

  if (path === "/install.sh") return shell(installScript(url.origin));

  const drop = path.match(/^\/s\/([^/]+)$/);
  if (drop) {
    const id = drop[1].toLowerCase();
    if (!isId(id)) return new Response("not found\n", { status: 404 });
    if (wantsHtml(req)) return env.ASSETS.fetch(new Request(new URL("/drop", url), req));
    return shell(installScript(url.origin, id));
  }

  const dl = path.match(/^\/dl\/([^/]+)$/);
  if (dl) return download(dl[1], env);

  return env.ASSETS.fetch(req);
}

export default {
  async fetch(req, env) {
    return harden(await route(req, env));
  },
  async scheduled(_event, env) {
    await sweep(env);
  },
} satisfies ExportedHandler<Env>;
