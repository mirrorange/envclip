import { apply, t, toggle } from "./i18n.js";

const $ = (id) => document.getElementById(id);
const id = location.pathname.split("/").pop().toLowerCase();
let state = "";

function render() {
  $("d-id").textContent = id;
  $("d-cmd").textContent = `curl -fsSL ${location.origin}/s/${id} | sh -s -- <key>`;
  if (state) {
    $("d-status").className = `status ${state}`;
    $("d-status").lastElementChild.textContent = t(`status.${state}`);
  }
}

async function check() {
  try {
    const res = await fetch(`/api/clips/${id}`, { cache: "no-store" });
    state = res.ok ? "sealed" : "gone";
  } catch {}
  render();
}

apply();
render();
check();
$("lang").onclick = toggle;
document.addEventListener("langchange", render);
