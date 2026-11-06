import { NAME_RE, authToken, seal } from "./crypto.js";
import { parseDotenv } from "./dotenv.js";
import { apply, t, toggle } from "./i18n.js";

const $ = (id) => document.getElementById(id);
const rows = $("rows");
const origin = location.origin;
let clip = null; // { id, key, expires_at }
let poll = 0;

function addRow(name = "", value = "") {
  const row = $("row-tpl").content.firstElementChild.cloneNode(true);
  apply(row);
  const [k, v] = row.querySelectorAll("input");
  k.value = name;
  v.value = value;
  k.addEventListener("input", () => {
    k.value = k.value.toUpperCase().replace(/[^A-Z0-9_]/g, "_");
    k.classList.remove("bad");
  });
  // pasting NAME=value into the name field splits it
  k.addEventListener("paste", (e) => {
    const text = e.clipboardData.getData("text");
    if (!text.includes("=")) return;
    e.preventDefault();
    importDotenv(text, row);
  });
  row.querySelector(".eye").onclick = () => (v.type = v.type === "password" ? "text" : "password");
  row.querySelector(".del").onclick = () => {
    row.remove();
    if (!rows.children.length) addRow();
  };
  rows.append(row);
  return row;
}

function importDotenv(text, replace) {
  try {
    const vars = parseDotenv(text);
    if (replace && vars.length) replace.remove();
    for (const r of [...rows.children]) {
      const [k, v] = r.querySelectorAll("input");
      if (!k.value && !v.value) r.remove();
    }
    for (const [n, val] of vars) addRow(n, val);
    if (!rows.children.length) addRow();
    error("");
    return true;
  } catch (e) {
    error(e.message);
    return false;
  }
}

function error(msg) {
  $("error").textContent = msg;
}

function collect() {
  const vars = [];
  const seen = new Set();
  for (const r of rows.children) {
    const [k, v] = r.querySelectorAll("input");
    const name = k.value.trim();
    if (!name && !v.value) continue;
    if (!NAME_RE.test(name)) throw (k.classList.add("bad"), new Error(t("err.name") + (name || "∅")));
    if (seen.has(name)) throw (k.classList.add("bad"), new Error(t("err.dup") + name));
    seen.add(name);
    vars.push([name, v.value]);
  }
  if (!vars.length) throw new Error(t("err.empty"));
  return vars;
}

async function post(path, body, method = "POST") {
  const res = await fetch(path, {
    method,
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = res.status === 204 ? {} : await res.json().catch(() => ({}));
  return { res, data };
}

async function onSeal() {
  const btn = $("seal");
  let vars;
  try {
    vars = collect();
  } catch (e) {
    return error(e.message);
  }
  error("");
  btn.disabled = true;
  btn.firstElementChild.textContent = t("compose.sealing");
  try {
    const ttl = Number(document.querySelector("input[name=ttl]:checked").value);
    for (let attempt = 0; attempt < 3; attempt++) {
      const s = await seal(vars);
      const { res, data } = await post("/api/clips", { id: s.id, auth_hash: s.auth_hash, blob: s.blob, ttl });
      if (res.status === 409) continue;
      if (!res.ok) return error(t("err.server") + (data.error || res.status));
      clip = { id: s.id, key: s.key, expires_at: data.expires_at };
      for (const r of rows.children) for (const i of r.querySelectorAll("input")) i.value = "";
      return showResult();
    }
    error(t("err.server") + "id_taken");
  } catch (e) {
    error(e instanceof TypeError ? t("err.network") : e.message);
  } finally {
    btn.disabled = false;
    btn.firstElementChild.textContent = t("compose.seal");
  }
}

const command = () => `curl -fsSL ${origin}/s/${clip.id} | sh -s -- ${clip.key}`;

function renderResult() {
  if (!clip) return;
  $("r-id").textContent = clip.id;
  $("r-link").textContent = `${origin}/s/${clip.id}`;
  $("r-key").textContent = clip.key;
  $("r-prompt").textContent = t("prompt")(command());
  $("r-reveal").textContent = t($("r-key").classList.contains("shown") ? "result.hide" : "result.show");
}

function setStatus(state) {
  const el = $("r-status");
  el.className = `status ${state}`;
  el.lastElementChild.textContent = t(`status.${state}`);
  el.dataset.state = state;
}

async function check() {
  if (!clip) return;
  try {
    const res = await fetch(`/api/clips/${clip.id}`, { cache: "no-store" });
    if (res.status === 404) {
      if ($("r-status").dataset.state !== "revoked") setStatus("gone");
      clearInterval(poll);
    }
  } catch {}
}

function showResult() {
  $("compose").hidden = true;
  $("result").hidden = false;
  $("r-key").classList.remove("shown");
  renderResult();
  setStatus("sealed");
  clearInterval(poll);
  poll = setInterval(check, 4000);
  $("result").scrollIntoView({ behavior: "smooth", block: "start" });
}

function reset() {
  clip = null;
  clearInterval(poll);
  $("result").hidden = true;
  $("compose").hidden = false;
  rows.replaceChildren();
  addRow();
}

async function onRevoke() {
  if (!clip) return;
  const { res } = await post(`/api/clips/${clip.id}`, { auth: await authToken(clip.key) }, "DELETE");
  clearInterval(poll);
  setStatus(res.ok ? "revoked" : "gone");
}

async function copy(btn) {
  const id = btn.dataset.copy;
  const text = id === "r-key" ? clip.key : $(id).textContent;
  await navigator.clipboard.writeText(text);
  btn.textContent = t("result.copied");
  setTimeout(() => (btn.textContent = t("result.copy")), 1200);
}

// wiring
apply();
for (const el of document.querySelectorAll(".origin")) el.textContent = origin;
addRow();
$("lang").onclick = toggle;
$("add").onclick = () => addRow().querySelector("input").focus();
$("import").onclick = () => {
  $("importer").hidden = !$("importer").hidden;
  if (!$("importer").hidden) $("dotenv").focus();
};
$("parse").onclick = () => {
  if (importDotenv($("dotenv").value)) {
    $("dotenv").value = "";
    $("importer").hidden = true;
  }
};
$("seal").onclick = onSeal;
$("r-new").onclick = reset;
$("r-revoke").onclick = onRevoke;
$("r-reveal").onclick = () => {
  $("r-key").classList.toggle("shown");
  renderResult();
};
for (const b of document.querySelectorAll("[data-copy]")) b.onclick = () => copy(b);
document.addEventListener("langchange", () => {
  renderResult();
  if (clip) setStatus($("r-status").dataset.state);
  for (const b of document.querySelectorAll("[data-copy]")) b.textContent = t("result.copy");
});
rows.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onSeal();
});
