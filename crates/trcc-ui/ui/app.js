"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

let view = null;
let shotUrl = null;
let busy = 0;

// Sections open in place under their row, one at a time.
let openSec = null;
const secs = [...document.querySelectorAll("[data-sec]")];
function toggleSection(name) {
  openSec = openSec === name ? null : name;
  for (const b of secs) {
    const on = b.dataset.sec === openSec;
    b.setAttribute("aria-expanded", String(on));
    $("sec-" + b.dataset.sec).hidden = !on;
  }
  if (openSec) {
    const b = secs.find((x) => x.dataset.sec === openSec);
    requestAnimationFrame(() => $("settings").scrollTo({ top: b.parentElement.offsetTop }));
  }
}
secs.forEach((b) => b.addEventListener("click", () => toggleSection(b.dataset.sec)));
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !openSec) return;
  const b = secs.find((x) => x.dataset.sec === openSec);
  toggleSection(openSec);
  b.focus();
});

function note(text, error = false) {
  $("onscreen").textContent = text;
  $("onscreen").classList.toggle("error", error);
}

function renderStatus() {
  const s = $("status");
  s.replaceChildren();
  s.classList.toggle("warn", !view.trayRunning);
  if (view.trayRunning) {
    s.textContent = "Running in the tray";
    return;
  }
  s.append("Tray app stopped");
  const start = document.createElement("button");
  start.className = "btn";
  start.textContent = "Start";
  start.addEventListener("click", () => apply(invoke("start_tray"), false));
  s.append(start);
}

function render() {
  $("v-image").textContent = view.image ?? "None";
  $("fname").textContent = view.image ?? "No picture set";
  $("finfo").textContent = view.imageInfo ?? "";
  $("v-rot").textContent = view.rotation + "°";
  for (const r of document.querySelectorAll('input[name="fit"]')) r.checked = r.value === view.fit;
  for (const r of document.querySelectorAll('input[name="rot"]')) r.checked = Number(r.value) === view.rotation;
  $("boot").checked = view.startWithWindows;
  renderStatus();
}

async function refreshPreview() {
  if (!view.image) {
    $("shot").hidden = $("thumb").hidden = true;
    $("empty").hidden = false;
    note("");
    return;
  }
  const n = ++busy;
  note("Updating screen…");
  try {
    const buf = await invoke("preview");
    if (n !== busy) return;
    const url = URL.createObjectURL(new Blob([buf], { type: "image/jpeg" }));
    if (shotUrl) URL.revokeObjectURL(shotUrl);
    shotUrl = url;
    $("shot").src = $("thumb").src = url;
    $("shot").hidden = $("thumb").hidden = false;
    $("empty").hidden = true;
    const kb = (buf.byteLength / 1024).toFixed(1);
    note(view.trayRunning ? `On screen, ${kb} KB` : `Not on screen while the tray app is stopped`, !view.trayRunning);
  } catch (e) {
    if (n === busy) note(String(e), true);
  }
}

// Every change goes through here: save, redraw, and refresh the preview if needed.
async function apply(request, redraw = true) {
  try {
    const v = await request;
    if (!v) return;
    view = v;
    render();
    if (redraw) await refreshPreview();
  } catch (e) {
    note(String(e), true);
    if (view) render();
  }
}

for (const r of document.querySelectorAll('input[name="fit"]')) {
  r.addEventListener("change", () => apply(invoke("set_fit", { fit: r.value })));
}
for (const r of document.querySelectorAll('input[name="rot"]')) {
  r.addEventListener("change", () => apply(invoke("set_rotation", { degrees: Number(r.value) })));
}
$("boot").addEventListener("change", (e) => apply(invoke("set_start_with_windows", { enabled: e.target.checked }), false));
$("browse").addEventListener("click", () => apply(invoke("pick_image")));

listen("drag", (e) => document.body.classList.toggle("drop", e.payload));
listen("dropped", (e) => {
  if ("Ok" in e.payload) apply(Promise.resolve(e.payload.Ok));
  else note(e.payload.Err, true);
});

// The tray app can be quit from its own menu while this window is open.
setInterval(async () => {
  try {
    const v = await invoke("state");
    // The settings file can also change by hand or from another window.
    const changed = ["trayRunning", "image", "imageInfo", "fit", "rotation"].some((k) => v[k] !== view[k]);
    view = v;
    render();
    if (changed) refreshPreview();
  } catch (_) {}
}, 3000);

apply(invoke("state"));
