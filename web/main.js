// Browser side of the web frontend: files, canvas, clock, audio, input.
// Everything else is in the wasm module (crates/pd-web).
//
// Presentation is a 2D canvas: the 640x480 RGBA picture is copied with
// putImageData and the browser scales the canvas element with
// `image-rendering: pixelated`. No WebGL/wgpu: one 1.2 MB copy per frame is
// far below budget and it avoids a GPU pipeline that would only do a scale.

import init, { Player, screen_width, screen_height, sample_rate, stored_mode, enhancement_list } from "./pkg/pd_web.js?v=dev";

const $ = (id) => document.getElementById(id);
const stage = $("stage"), canvas = $("screen"), loader = $("loader"), drop = $("drop");
const message = $("message"), startButton = $("start"), bar = $("bar"), statusLine = $("status");
const integerBox = $("integer");

// "?v=dev" is replaced with a hash of the build by web/package.sh, so that a new
// upload is never mixed with files a browser or host still has cached.
const wasm = await init({ module_or_path: new URL("./pkg/pd_web_bg.wasm?v=dev", import.meta.url) });

// ---------------------------------------------------------------- version

// Original or enhanced, and for enhanced one checkbox per improvement
// (all on unless switched off before; the game remembers both).
const custom = $("custom"), flagBox = $("flags");
const mode = () => document.querySelector('input[name="mode"]:checked').value;
const flagInputs = () => [...flagBox.querySelectorAll("input")];
const flagsOff = () => flagInputs().filter((i) => !i.checked).map((i) => i.value);

function syncVersion() {
  custom.hidden = mode() !== "enhanced";
  const off = flagsOff().length;
  $("custom-count").textContent = off ? `(${off} switched off)` : "(all on)";
}

document.querySelector(`input[name="mode"][value="${stored_mode()}"]`).checked = true;
{
  let group = null, list = null;
  for (const line of enhancement_list().split("\n")) {
    const [name, groupName, text, on] = line.split("\t");
    if (groupName !== group) {
      group = groupName;
      flagBox.append(Object.assign(document.createElement("h4"), { textContent: group }));
      list = flagBox.appendChild(document.createElement("div"));
    }
    const label = document.createElement("label");
    const box = Object.assign(document.createElement("input"), { type: "checkbox", value: name, checked: on === "1" });
    label.append(box, " " + text);
    list.append(label);
  }
}
document.getElementById("versions").addEventListener("change", syncVersion);
for (const [id, on] of [["flags-all", true], ["flags-none", false]]) {
  $(id).addEventListener("click", () => { for (const i of flagInputs()) i.checked = on; syncVersion(); });
}
syncVersion();
const W = screen_width(), H = screen_height(), RATE = sample_rate();

// ---------------------------------------------------------------- loading

const picked = { bin: null, cue: null };
let player = null;

function say(text, isError = false) {
  message.textContent = text;
  message.classList.toggle("error", isError);
}

function pick(files) {
  for (const f of files) {
    const name = f.name.toLowerCase();
    if (name.endsWith(".bin")) picked.bin = f;
    else if (name.endsWith(".cue")) picked.cue = f;
  }
  for (const kind of ["bin", "cue"]) {
    const slot = $("slot-" + kind);
    slot.textContent = picked[kind] ? picked[kind].name : `no .${kind} yet`;
    slot.classList.toggle("ready", !!picked[kind]);
  }
  if (picked.bin && picked.cue) load();
  else if (files.length) say(picked.bin ? "Now add the .cue file." : picked.cue ? "Now add the .bin file." : "Those are not .bin/.cue files.", !picked.bin && !picked.cue);
}

async function load() {
  say(`Reading ${picked.bin.name} (${(picked.bin.size / 1048576).toFixed(0)} MB)…`);
  try {
    const [bin, cue] = await Promise.all([picked.bin.arrayBuffer(), picked.cue.text()]);
    // Copies the image into wasm memory; the ArrayBuffer is garbage afterwards.
    const demo = new URLSearchParams(location.search).has("demo");
    player = new Player(new Uint8Array(bin), cue, mode(), flagsOff().join(","), demo);
  } catch (e) {
    say(`Could not load the disc image: ${e.message ?? e}`, true);
    return;
  }
  const info = player.info();
  console.info("pd-web:", info);
  say(info, info.includes("\nerror:"));
  $("keys").textContent = mode() === "enhanced" && !flagsOff().includes("modern_controls")
    ? "Mouse turn · W/S walk · A/D strafe · Click fire · 1–0 weapon · E jetpack · Tab map · Esc menu"
    : "Arrows move · Ctrl strafe · Space fire · Backspace weapon · Enter jetpack · Tab map · Esc menu";
  loader.hidden = true;
  canvas.hidden = false;
  bar.hidden = false;
  startButton.hidden = false;
  startButton.focus();
  layout();
}

$("files").addEventListener("change", (e) => pick(e.target.files));
for (const type of ["dragenter", "dragover"]) {
  window.addEventListener(type, (e) => { e.preventDefault(); drop.classList.add("over"); });
}
window.addEventListener("dragleave", () => drop.classList.remove("over"));
window.addEventListener("drop", (e) => {
  e.preventDefault();
  drop.classList.remove("over");
  if (!player) pick(e.dataTransfer.files);
});

// ---------------------------------------------------------------- picture

const ctx2d = canvas.getContext("2d", { alpha: false });
const image = ctx2d.createImageData(W, H);

// Largest 4:3 box that fits the stage (optionally a whole multiple of
// 640x480 in device pixels), centred by the stage's grid.
function layout() {
  const dpr = window.devicePixelRatio || 1;
  const availW = stage.clientWidth * dpr, availH = stage.clientHeight * dpr;
  let scale = Math.min(availW / W, availH / H);
  if (integerBox.checked && scale >= 1) scale = Math.floor(scale);
  canvas.style.width = `${(W * scale) / dpr}px`;
  canvas.style.height = `${(H * scale) / dpr}px`;
}
new ResizeObserver(layout).observe(stage);
integerBox.addEventListener("change", layout);

function toggleFullscreen() {
  if (document.fullscreenElement) document.exitFullscreen();
  else stage.requestFullscreen().catch((e) => console.warn("fullscreen refused:", e));
}
$("fullscreen").addEventListener("click", toggleFullscreen);

function present() {
  // Re-create the view every frame: the wasm memory buffer is replaced when it grows.
  image.data.set(new Uint8ClampedArray(wasm.memory.buffer, player.rgba_ptr(), W * H * 4));
  ctx2d.putImageData(image, 0, 0);
}

// ---------------------------------------------------------------- audio

// WebAudio output: chunks mixed by the wasm module are scheduled back to back
// as AudioBufferSourceNodes, keeping LEAD seconds queued ahead of the audio
// clock. Created only from a user gesture (the start button / key).
const CHUNK = 1024, LEAD = 0.1;
let audio = null, nextChunkAt = 0;
const stats = { frames: 0, audioChunks: 0, audioResyncs: 0, audioPeak: 0 };

function startAudio() {
  const AC = window.AudioContext || window.webkitAudioContext;
  if (!AC) return console.warn("no WebAudio; running silent");
  try {
    audio = new AC({ sampleRate: RATE, latencyHint: "interactive" });
  } catch {
    audio = new AC(); // odd device rates: buffers below are still 44.1 kHz and get resampled
  }
  audio.resume();
}

function pumpAudio() {
  if (!audio || audio.state !== "running") return;
  const now = audio.currentTime;
  if (nextChunkAt < now) {
    // Fell behind (first call, or the tab was in the background).
    if (nextChunkAt !== 0) stats.audioResyncs++;
    nextChunkAt = now + 0.02;
  }
  while (nextChunkAt < now + LEAD) {
    const pcm = new Int16Array(wasm.memory.buffer, player.mix_audio(CHUNK), CHUNK * 2);
    const buffer = audio.createBuffer(2, CHUNK, RATE);
    const left = buffer.getChannelData(0), right = buffer.getChannelData(1);
    for (let i = 0; i < CHUNK; i++) {
      left[i] = pcm[2 * i] / 32768;
      right[i] = pcm[2 * i + 1] / 32768;
      const a = Math.abs(left[i]);
      if (a > stats.audioPeak) stats.audioPeak = a;
    }
    const node = audio.createBufferSource();
    node.buffer = buffer;
    node.connect(audio.destination);
    node.start(nextChunkAt);
    nextChunkAt += CHUNK / RATE;
    stats.audioChunks++;
  }
}

// ---------------------------------------------------------------- input

// Help dialog: while it is open the game gets no keys and the pointer is free.
const help = $("help");
for (const b of document.querySelectorAll("[data-help]")) {
  b.addEventListener("click", () => {
    if (document.pointerLockElement) document.exitPointerLock();
    player?.release_all();
    help.showModal();
  });
}

window.addEventListener("keydown", (e) => {
  if (!running || help.open) return;
  if (e.altKey && (e.code === "Enter" || e.code === "NumpadEnter")) {
    if (!e.repeat) toggleFullscreen();
    e.preventDefault();
    return;
  }
  // Ctrl is the action key, so Ctrl+arrow etc. must reach the game; bound
  // keys are swallowed, everything else (Ctrl+R, ...) stays with the browser.
  if (e.metaKey) return;
  if (e.repeat) {
    // The game does its own auto-repeat; still keep the browser away from bound keys.
    if (!e.ctrlKey || e.code.startsWith("Arrow")) e.preventDefault();
    return;
  }
  // 1 = game button, 2 = a key the menus read as text (letters for names and cheat codes).
  const kind = player.key_event(e.code, true);
  if (kind === 1 || (kind === 2 && !e.ctrlKey && !e.altKey)) e.preventDefault();
});
window.addEventListener("keyup", (e) => {
  if (running && !help.open && player.key_event(e.code, false) === 1) e.preventDefault();
});
window.addEventListener("blur", () => player?.release_all());

// The left button is still down from the click that took the pointer lock.
let grabbing = false;

function mouse(e) {
  const r = canvas.getBoundingClientRect();
  if (r.width === 0) return;
  player.set_mouse_pos(Math.floor(((e.clientX - r.left) * W) / r.width), Math.floor(((e.clientY - r.top) * H) / r.height));
  player.set_mouse_buttons((e.buttons & 1) !== 0, (e.buttons & 2) !== 0);
}
for (const type of ["pointermove", "pointerdown", "pointerup"]) {
  canvas.addEventListener(type, (e) => {
    if (!running) return;
    if ((e.buttons & 1) === 0) grabbing = false;
    if (document.pointerLockElement === canvas || grabbing) {
      // Mouse look: relative movement only; the position stays where it was.
      if (type === "pointermove" && !grabbing) player.mouse_motion(e.movementX, e.movementY);
      player.set_mouse_buttons((e.buttons & 1) !== 0 && !grabbing, (e.buttons & 2) !== 0);
      return;
    }
    // Pointer lock needs a user gesture, so it is taken on the first click in the view.
    // That click only hands over the mouse: the game does not see it as a shot.
    if (type === "pointerdown" && player.wants_pointer_lock()) {
      canvas.requestPointerLock();
      grabbing = true;
      return;
    }
    mouse(e);
  });
}
// Menus and modal screens get the pointer back.
function syncPointerLock() {
  if (document.pointerLockElement === canvas && !player.wants_pointer_lock()) document.exitPointerLock();
}
// The browser takes the pointer back on Esc and keeps that key press to itself;
// the game should still see it, so that one Esc opens the menu.
document.addEventListener("pointerlockchange", () => {
  if (document.pointerLockElement || !running || help.open || !player.wants_pointer_lock()) return;
  player.key_event("Escape", true);
  setTimeout(() => player.key_event("Escape", false), 150);
});
canvas.addEventListener("contextmenu", (e) => e.preventDefault());

// Gamepad API, "standard" mapping -> bit index of Player.set_gamepad
// (order of pd_engine::input::Button::ALL).
const PAD_BUTTONS = [
  [12], [13], [14], [15], // up, down, left, right
  [0],  // action     (A / cross)
  [1],  // secondary  (B / circle)
  [5],  // inventory  (RB)
  [9],  // cancel     (start)
  [2],  // item       (X / square)
  [3],  // next item  (Y / triangle)
  [4],  // view       (LB)
  [], [], [], [],
  [8],  // pause      (select)
];
function pollGamepads() {
  let bits = 0, x = 0, y = 0;
  for (const pad of navigator.getGamepads?.() ?? []) {
    if (!pad || !pad.connected) continue;
    PAD_BUTTONS.forEach((indices, bit) => {
      if (indices.some((i) => pad.buttons[i]?.pressed)) bits |= 1 << bit;
    });
    if (Math.abs(pad.axes[0] ?? 0) > Math.abs(x)) x = pad.axes[0];
    if (Math.abs(pad.axes[1] ?? 0) > Math.abs(y)) y = pad.axes[1];
  }
  player.set_gamepad(bits, x, y);
}

// ---------------------------------------------------------------- loop

let running = false, last = 0;

function frame(now) {
  if (!running) return;
  // The engine clamps long gaps itself (background tab); u32 microseconds here.
  // The game stands still while the help is open (the music plays on).
  const dtUs = help.open ? 0 : Math.min(Math.max(now - last, 0) * 1000, 1e9);
  last = now;
  pollGamepads();
  player.frame(dtUs);
  syncPointerLock();
  present();
  pumpAudio();
  stats.frames++;
  if (stats.frames % 30 === 0) {
    const track = player.cd_track();
    statusLine.textContent = track ? `CD track ${track}` : "CD stopped";
  }
  if (player.quit_requested()) {
    running = false;
    audio?.close();
    statusLine.textContent = "Ended. Reload the page to play again.";
    return;
  }
  requestAnimationFrame(frame);
}

function start() {
  if (running || !player) return;
  startButton.hidden = true;
  startAudio();
  running = true;
  last = performance.now();
  requestAnimationFrame(frame);
}
startButton.addEventListener("click", start);
window.addEventListener("keydown", (e) => {
  if (!running && player && !e.repeat && !e.ctrlKey && !e.metaKey && !e.altKey) { e.preventDefault(); start(); }
}, true);

// For debugging from the console and for automated checks.
window.pd = { stats, get player() { return player; }, get audio() { return audio; }, get running() { return running; } };
