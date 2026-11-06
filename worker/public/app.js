import * as ec from "./crypto.js";
import { STRINGS, detectLang } from "./i18n.js";

const $ = (id) => document.getElementById(id);
const MAX_CIPHERTEXT = 90_000;
let lang = detectLang();
const t = (k, ...a) => {
  const v = STRINGS[lang][k] ?? STRINGS.en[k];
  return typeof v === "function" ? v(...a) : v;
};

// ── i18n ───────────────────────────────────────────────────
function applyLang() {
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  for (const el of document.querySelectorAll("[data-i18n]")) el.textContent = t(el.dataset.i18n);
  for (const el of document.querySelectorAll("[data-i18n-ph]")) el.placeholder = t(el.dataset.i18nPh);
  for (const el of document.querySelectorAll(".lang")) el.classList.toggle("on", el.dataset.lang === lang);
  for (const row of $("rows").children) localizeRow(row);
  if (current) renderResult();
}
for (const b of document.querySelectorAll(".lang")) {
  b.addEventListener("click", () => {
    lang = b.dataset.lang;
    localStorage.setItem("envclip.lang", lang);
    applyLang();
  });
}

const human = (s) =>
  s < 120 ? `${s}s` : s < 7200 ? `${Math.round(s / 60)}m` : s < 172800 ? `${Math.round(s / 3600)}h` : `${Math.round(s / 86400)}d`;

function say(id, text, kind = "") {
  const el = $(id);
  el.textContent = text;
  el.className = `msg ${kind}`;
}

// ── compose rows ───────────────────────────────────────────
function localizeRow(row) {
  row.querySelector(".k").placeholder = t("name_ph");
  row.querySelector(".v").placeholder = t("value_ph");
  const eye = row.querySelector(".eye");
  eye.textContent = row.querySelector(".v").type === "password" ? t("show") : t("hide");
  row.querySelector(".x").title = t("remove");
}

function addRow(name = "", value = "") {
  const row = document.createElement("div");
  row.className = "row";
  row.innerHTML = `<input class="k" spellcheck="false" autocapitalize="characters">
    <span class="eq">=</span>
    <input class="v" type="password" spellcheck="false" autocomplete="new-password">
    <button type="button" class="eye ghost"></button>
    <button type="button" class="x ghost" aria-label="remove">×</button>`;
  const k = row.querySelector(".k"), v = row.querySelector(".v");
  k.value = name;
  setValue(row, value);
  // Pasting a whole .env into a name field explodes it into rows.
  k.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text") ?? "";
    if (!text.includes("=")) return;
    e.preventDefault();
    importEnv(text, row);
  });
  k.addEventListener("input", () => {
    k.value = k.value.replace(/\s/g, "");
  });
  row.querySelector(".eye").addEventListener("click", () => {
    v.type = v.type === "password" ? "text" : "password";
    localizeRow(row);
  });
  row.querySelector(".x").addEventListener("click", () => {
    row.remove();
    if (!$("rows").children.length) addRow();
  });
  localizeRow(row);
  $("rows").append(row);
  return row;
}

// <input> cannot hold newlines, so multi-line values (PEM keys, …) are kept on
// the row object and shown read-only with ⏎ markers.
function setValue(row, value) {
  const v = row.querySelector(".v");
  row.multi = value.includes("\n") ? value : null;
  v.readOnly = row.multi !== null;
  v.value = row.multi === null ? value : value.replace(/\n/g, "⏎");
}

const rowValue = (row) => row.multi ?? row.querySelector(".v").value;

function importEnv(text, replace) {
  let pairs;
  try {
    pairs = ec.parseDotenv(text);
  } catch (e) {
    say("compose-msg", e.message, "err");
    return false;
  }
  if (replace && !replace.querySelector(".v").value) replace.remove();
  for (const r of [...$("rows").children]) {
    if (!r.querySelector(".k").value && !r.querySelector(".v").value) r.remove();
  }
  for (const [k, v] of pairs) {
    const existing = [...$("rows").children].find((r) => r.querySelector(".k").value === k);
    if (existing) setValue(existing, v);
    else addRow(k, v);
  }
  if (!$("rows").children.length) addRow();
  say("compose-msg", "");
  return true;
}

$("add").addEventListener("click", () => addRow().querySelector(".k").focus());
$("paste-toggle").addEventListener("click", () => {
  $("paste-box").hidden = !$("paste-box").hidden;
  if (!$("paste-box").hidden) $("paste").focus();
});
$("paste-cancel").addEventListener("click", () => {
  $("paste").value = "";
  $("paste-box").hidden = true;
});
$("paste-import").addEventListener("click", () => {
  if (importEnv($("paste").value)) {
    $("paste").value = "";
    $("paste-box").hidden = true;
  }
});

// ── seal & upload ──────────────────────────────────────────
let current = null; // { url, key, id, keys, n, ttl }

async function api(method, path, { body, token } = {}) {
  const headers = {};
  if (body) headers["content-type"] = "application/json";
  if (token) headers.authorization = `Bearer ${token}`;
  const r = await fetch(`/api/v1${path}`, { method, headers, body: body && JSON.stringify(body) });
  const data = r.status === 204 ? null : await r.json().catch(() => null);
  return { status: r.status, data };
}

$("compose-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const env = [];
  const seen = new Set();
  for (const row of $("rows").children) {
    const k = row.querySelector(".k").value.trim();
    const v = rowValue(row);
    if (!k && !v) continue;
    if (!ec.validName(k)) return say("compose-msg", t("bad_name", k), "err");
    if (seen.has(k)) return say("compose-msg", t("dupe", k), "err");
    seen.add(k);
    env.push([k, v]);
  }
  if (!env.length) return say("compose-msg", t("empty"), "err");

  const btn = $("seal");
  btn.disabled = true;
  say("compose-msg", t("sealing"));
  try {
    const secret = ec.generateSecret();
    const keys = await ec.deriveKeys(secret);
    const payload = ec.encodePayload(env);
    const ciphertext = await ec.seal(keys, payload);
    payload.fill(0);
    if (ciphertext.length > MAX_CIPHERTEXT) throw new Error(t("too_large"));
    const ttl = Number(new FormData(e.target).get("ttl"));
    const { status, data } = await api("POST", "/drops", { body: { ciphertext, claim_hash: keys.claimHash, ttl } });
    if (status !== 201) throw new Error(data?.error ?? `HTTP ${status}`);
    current = { id: data.id, url: `${location.origin}/c/${data.id}`, key: ec.encodeSecret(secret), keys, n: env.length, ttl };
    secret.fill(0);
    // Wipe the form: the plaintext should not linger in the DOM.
    $("rows").replaceChildren();
    addRow();
    say("compose-msg", "");
    renderResult();
    $("compose").hidden = true;
    $("result").hidden = false;
  } catch (err) {
    say("compose-msg", t("net", err.message), "err");
  } finally {
    btn.disabled = false;
  }
});

function renderResult() {
  $("sealed-line").textContent = `✓ ${t("sealed", current.n, human(current.ttl))}`;
  $("out-link").textContent = current.url;
  $("out-key").textContent = current.key;
  $("out-msg").textContent = t("agent_msg", current.url, current.key);
}

async function checkStatus(id, keys, msgId) {
  const { status, data } = await api("GET", `/drops/${id}`, { token: keys.claimToken });
  if (status === 200) say(msgId, t("waiting", human(Math.max(0, data.expires_at - Math.floor(Date.now() / 1000)))), "ok");
  else if (status === 404) say(msgId, t("gone"));
  else say(msgId, t("net", data?.error ?? status), "err");
}

async function revoke(id, keys, msgId) {
  const { status, data } = await api("DELETE", `/drops/${id}`, { token: keys.claimToken });
  if (status === 204) say(msgId, t("revoked"), "ok");
  else if (status === 404) say(msgId, t("gone"));
  else say(msgId, t("net", data?.error ?? status), "err");
}

$("r-status").addEventListener("click", () => checkStatus(current.id, current.keys, "result-msg"));
$("r-revoke").addEventListener("click", () => revoke(current.id, current.keys, "result-msg"));
$("r-again").addEventListener("click", () => {
  current = null;
  for (const id of ["out-link", "out-key", "out-msg", "result-msg"]) $(id).textContent = "";
  $("result").hidden = true;
  $("compose").hidden = false;
  $("rows").querySelector(".k")?.focus();
});

// ── copy buttons ───────────────────────────────────────────
document.addEventListener("click", async (e) => {
  const b = e.target.closest("[data-copy]");
  if (!b) return;
  await navigator.clipboard.writeText($(b.dataset.copy).textContent);
  b.textContent = t("copied");
  b.classList.add("done");
  setTimeout(() => {
    b.textContent = t(b.dataset.i18n);
    b.classList.remove("done");
  }, 1400);
});

// ── drop landing: /c/<id> opened in a browser ──────────────
const dropMatch = /^\/c\/([0-9a-z]{1,64})\/?$/.exec(location.pathname);
if (dropMatch) {
  const id = dropMatch[1];
  $("compose").hidden = true;
  $("drop").hidden = false;
  $("drop-id").textContent = id;
  $("drop-cmd").textContent = `curl -fsSL ${location.origin}/c/${id} | sh -s -- <key>`;
  const withKey = async (fn) => {
    const secret = ec.parseSecret($("drop-key").value);
    if (!secret) return say("drop-msg", t("bad_key"), "err");
    const keys = await ec.deriveKeys(secret);
    secret.fill(0);
    await fn(id, keys, "drop-msg");
  };
  $("drop-form").addEventListener("submit", (e) => {
    e.preventDefault();
    withKey(checkStatus);
  });
  $("d-revoke").addEventListener("click", () => withKey(revoke));
}

for (const el of document.querySelectorAll(".origin")) el.textContent = location.origin;
addRow();
applyLang();
