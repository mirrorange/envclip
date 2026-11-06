import { handleApi, json, purgeExpired } from "./api";
import { installScript } from "./install";

export interface Env {
  DB: D1Database;
  ASSETS: Fetcher;
  RELEASES_URL: string;
  CREATE_LIMIT?: RateLimit;
  CLAIM_LIMIT?: RateLimit;
}

const ASSET_RE = /^envclip-(linux|darwin)-(x86_64|aarch64)(\.sha256)?$/;

function script(body: string): Response {
  return new Response(body, {
    headers: {
      "content-type": "text/x-shellscript; charset=utf-8",
      "cache-control": "no-store",
      "x-content-type-options": "nosniff",
    },
  });
}

/** Browsers get the web app; curl, wget and agents get the installer. */
function wantsHtml(req: Request): boolean {
  return (req.headers.get("accept") ?? "").includes("text/html");
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    const path = url.pathname;

    if (path.startsWith("/api/")) return handleApi(req, env, path);

    if (path === "/install.sh") return script(installScript(url.origin));

    const drop = /^\/c\/([0-9a-z]{1,64})\/?$/.exec(path);
    if (drop) {
      if (wantsHtml(req)) return env.ASSETS.fetch(new Request(new URL("/", url), req));
      return script(installScript(url.origin, drop[1]));
    }

    const dl = /^\/dl\/([^/]+)$/.exec(path);
    if (dl && ASSET_RE.test(dl[1])) {
      return Response.redirect(`${env.RELEASES_URL.replace(/\/$/, "")}/${dl[1]}`, 302);
    }

    return json({ error: "not_found" }, 404);
  },

  async scheduled(_event: ScheduledController, env: Env): Promise<void> {
    await purgeExpired(env);
  },
} satisfies ExportedHandler<Env>;
