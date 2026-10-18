import { invoke } from "@tauri-apps/api/core";

interface StDevice {
  deviceId: string;
  label: string;
  deviceType?: string | null;
  capabilities: string[];
}

interface StStatus {
  hasToken: boolean;
  deviceId?: string | null;
  deviceLabel?: string | null;
  capabilities: string[];
}

interface StInput {
  id: string;
  name: string;
  current: boolean;
}

interface TvApp {
  appId: string;
  name: string;
}

const $ = <T extends HTMLElement>(selector: string): T =>
  document.querySelector(selector) as T;

const statusPill = $<HTMLDivElement>("#status-pill");
const statusText = $<HTMLSpanElement>("#status-text");
const connectedName = $<HTMLParagraphElement>("#connected-name");
const stSetup = $<HTMLDivElement>("#st-setup");
const stPanel = $<HTMLDivElement>("#st-panel");
const stForm = $<HTMLFormElement>("#st-form");
const stTokenInput = $<HTMLInputElement>("#st-token");
const stDeviceSelect = $<HTMLSelectElement>("#st-device");
const stUse = $<HTMLButtonElement>("#st-use");
const stRemove = $<HTMLButtonElement>("#st-remove");
const stStatusText = $<HTMLParagraphElement>("#st-status-text");
const appGrid = $<HTMLDivElement>("#app-grid");
const appCustomForm = $<HTMLFormElement>("#app-custom-form");
const appCustomId = $<HTMLInputElement>("#app-custom-id");
const remoteBody = $<HTMLDivElement>("#remote-body");
const remoteTitle = $<HTMLHeadingElement>("#remote-title");
const remoteHint = $<HTMLParagraphElement>("#remote-hint");
const inputOverlay = $<HTMLDivElement>("#input-overlay");
const inputList = $<HTMLDivElement>("#input-list");
const inputClose = $<HTMLButtonElement>("#input-close");
const toastStack = $<HTMLDivElement>("#toast-stack");

// Teclas físicas → ação na nuvem
const CLOUD_KEY_ACTIONS: Record<string, { action: string; arg?: string }> = {
  KEY_POWER: { action: "power_toggle" },
  KEY_VOLUP: { action: "volume_up" },
  KEY_VOLDOWN: { action: "volume_down" },
  KEY_MUTE: { action: "mute" },
  KEY_CHUP: { action: "channel_up" },
  KEY_CHDOWN: { action: "channel_down" },
  KEY_PLAY_BACK: { action: "play_pause" },
  KEY_REWIND: { action: "rewind" },
  KEY_FF: { action: "fast_forward" },
  KEY_UP: { action: "remote_key", arg: "UP" },
  KEY_DOWN: { action: "remote_key", arg: "DOWN" },
  KEY_LEFT: { action: "remote_key", arg: "LEFT" },
  KEY_RIGHT: { action: "remote_key", arg: "RIGHT" },
  KEY_ENTER: { action: "remote_key", arg: "OK" },
  KEY_RETURN: { action: "remote_key", arg: "BACK" },
  KEY_EXIT: { action: "remote_key", arg: "EXIT" },
  KEY_MENU: { action: "remote_key", arg: "MENU" },
  KEY_HOME: { action: "remote_key", arg: "HOME" },
};

// Capacidade(s) que habilitam cada tecla (basta uma existir na TV)
const CLOUD_KEY_CAPABILITY: Record<string, string[]> = {
  KEY_POWER: ["switch"],
  KEY_VOLUP: ["audioVolume"],
  KEY_VOLDOWN: ["audioVolume"],
  KEY_MUTE: ["samsungvd.remoteControl", "audioMute"],
  KEY_CHUP: ["tvChannel"],
  KEY_CHDOWN: ["tvChannel"],
  KEY_PLAY_BACK: ["samsungvd.remoteControl", "mediaPlayback"],
  KEY_REWIND: ["mediaPlayback", "samsungvd.remoteControl"],
  KEY_FF: ["mediaPlayback", "samsungvd.remoteControl"],
  KEY_SOURCE: ["samsungvd.mediaInputSource", "mediaInputSource", "samsungvd.remoteControl"],
  KEY_UP: ["samsungvd.remoteControl"],
  KEY_DOWN: ["samsungvd.remoteControl"],
  KEY_LEFT: ["samsungvd.remoteControl"],
  KEY_RIGHT: ["samsungvd.remoteControl"],
  KEY_ENTER: ["samsungvd.remoteControl"],
  KEY_RETURN: ["samsungvd.remoteControl"],
  KEY_EXIT: ["samsungvd.remoteControl"],
  KEY_MENU: ["samsungvd.remoteControl"],
  KEY_HOME: ["samsungvd.remoteControl"],
};

// Atalhos de apps (IDs da lista da comunidade; variam por região/ano)
const APP_PRESETS: TvApp[] = [
  { appId: "11101200001", name: "Netflix" },
  { appId: "111299001912", name: "YouTube" },
  { appId: "3201512006785", name: "Prime Video" },
  { appId: "3201901017640", name: "Disney+" },
  { appId: "3201908019022", name: "Globoplay" },
  { appId: "3201601007230", name: "HBO Max" },
  { appId: "3201606009684", name: "Spotify" },
  { appId: "org.tizen.browser", name: "Internet" },
];

let cloudActive = false;
let cloudCaps: string[] = [];

// ---------------------------------------------------------------------------
// Utilidades de UI
// ---------------------------------------------------------------------------

function esc(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function toast(message: string, type: "info" | "error" | "success" = "info") {
  const el = document.createElement("div");
  el.className = `toast ${type}`;
  el.textContent = message;
  el.addEventListener("click", () => el.remove());
  toastStack.append(el);
  setTimeout(() => el.remove(), 6000);
}

function setStatus(state: "idle" | "connecting" | "waiting" | "connected" | "error", text: string) {
  statusPill.dataset.state = state;
  statusText.textContent = text;
}

function refreshRemoteAvailability() {
  remoteBody.classList.toggle("disabled", !cloudActive);

  remoteBody.querySelectorAll<HTMLButtonElement>("button.key[data-key]").forEach((button) => {
    const key = button.dataset.key ?? "";
    const caps = CLOUD_KEY_CAPABILITY[key];
    const ok = cloudActive && caps !== undefined && caps.some((c) => cloudCaps.includes(c));
    button.classList.toggle("cloud-off", !ok);
  });
}

// ---------------------------------------------------------------------------
// SmartThings
// ---------------------------------------------------------------------------

function applyStStatus(status: StStatus) {
  stSetup.classList.toggle("hidden", status.hasToken);
  stPanel.classList.toggle("hidden", !status.hasToken);
  stRemove.classList.toggle("hidden", !status.hasToken);

  if (status.hasToken && status.deviceId) {
    cloudActive = true;
    cloudCaps = status.capabilities;
    const label = status.deviceLabel ?? "TV";
    stStatusText.textContent = `Nuvem ativa: ${label} • ${status.capabilities.length} capacidades.`;
    setStatus("connected", `Nuvem: ${label}`);
    connectedName.textContent = `${label} • SmartThings Cloud`;
    remoteTitle.textContent = label;
    remoteHint.textContent = "Controles enviados pela nuvem do SmartThings.";
  } else {
    cloudActive = false;
    cloudCaps = [];
    stStatusText.textContent = status.hasToken
      ? "Token salvo. Escolha a TV abaixo e clique em “Usar esta TV”."
      : "";
    setStatus("idle", status.hasToken ? "Escolha a TV" : "Não configurado");
    connectedName.textContent = "Modo nuvem (SmartThings)";
    remoteTitle.textContent = "Controle";
    remoteHint.textContent = "Conecte sua conta SmartThings para controlar a TV.";
  }

  refreshRemoteAvailability();
}

function fillStDevices(list: StDevice[]) {
  stDeviceSelect.innerHTML = list
    .map(
      (d) =>
        `<option value="${esc(d.deviceId)}">${esc(d.label)}${d.deviceType ? ` — ${esc(d.deviceType)}` : ""}</option>`,
    )
    .join("");
}

async function stExecute(action: string, arg?: string) {
  try {
    await invoke("st_execute", { action, arg: arg ?? null });
  } catch (error) {
    toast(String(error), "error");
  }
}

function launchAppById(appId: string) {
  void stExecute("launch_app", appId).then(() => toast("App enviado para a TV.", "success"));
}

// ---------------------------------------------------------------------------
// Apps
// ---------------------------------------------------------------------------

function renderApps(apps: TvApp[]) {
  appGrid.innerHTML = apps
    .map(
      (app) =>
        `<button class="app-tile" data-app-id="${esc(app.appId)}" title="${esc(app.name)}"><span class="app-fallback">${esc(app.name.slice(0, 1).toUpperCase() || "?")}</span><span class="app-name">${esc(app.name)}</span></button>`,
    )
    .join("");
}

// ---------------------------------------------------------------------------
// Teclas
// ---------------------------------------------------------------------------

function handleKey(key: string) {
  if (!cloudActive) return;

  if (key === "KEY_SOURCE") {
    void openInputPicker();
    return;
  }

  const entry = CLOUD_KEY_ACTIONS[key];
  if (!entry) {
    toast("Tecla indisponível neste modelo via nuvem.", "info");
    return;
  }
  void stExecute(entry.action, entry.arg);
}

async function openInputPicker() {
  if (!cloudActive) return;
  try {
    const inputs = await invoke<StInput[]>("st_inputs");
    inputList.innerHTML = inputs
      .map(
        (input) =>
          `<button class="input-item ${input.current ? "current" : ""}" data-input-id="${esc(input.id)}">${esc(input.name)}${input.current ? " ✓" : ""}</button>`,
      )
      .join("");
    inputOverlay.classList.remove("hidden");
  } catch (error) {
    toast(String(error), "error");
  }
}

// ---------------------------------------------------------------------------
// Eventos
// ---------------------------------------------------------------------------

remoteBody.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button.key");
  if (!button) return;
  const key = button.dataset.key;
  if (!key) return;

  button.classList.add("pressed");
  setTimeout(() => button.classList.remove("pressed"), 120);
  handleKey(key);
});

window.addEventListener("keydown", (event) => {
  if (!cloudActive) return;
  const target = event.target as HTMLElement | null;
  if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;

  const map: Record<string, string> = {
    ArrowUp: "KEY_UP",
    ArrowDown: "KEY_DOWN",
    ArrowLeft: "KEY_LEFT",
    ArrowRight: "KEY_RIGHT",
    Enter: "KEY_ENTER",
    Backspace: "KEY_RETURN",
    Escape: "KEY_EXIT",
    " ": "KEY_PLAY_BACK",
    "+": "KEY_VOLUP",
    "-": "KEY_VOLDOWN",
    m: "KEY_MUTE",
    h: "KEY_HOME",
    p: "KEY_PLAY_BACK",
  };

  const key = map[event.key] ?? map[event.key.toLowerCase()];
  if (!key) return;

  event.preventDefault();
  const button = remoteBody.querySelector<HTMLButtonElement>(`[data-key="${key}"]`);
  button?.classList.add("pressed");
  setTimeout(() => button?.classList.remove("pressed"), 120);
  handleKey(key);
});

inputClose.addEventListener("click", () => inputOverlay.classList.add("hidden"));
inputOverlay.addEventListener("click", (event) => {
  if (event.target === inputOverlay) inputOverlay.classList.add("hidden");
});
inputList.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-input-id]");
  const id = button?.dataset.inputId;
  if (!id) return;
  inputOverlay.classList.add("hidden");
  void stExecute("set_input", id);
});

appGrid.addEventListener("click", (event) => {
  const tile = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-app-id]");
  const appId = tile?.dataset.appId;
  if (!appId) return;
  launchAppById(appId);
});

appCustomForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const appId = appCustomId.value.trim();
  if (!appId) return;
  launchAppById(appId);
  appCustomId.value = "";
});

stForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const token = stTokenInput.value.trim();
  if (!token) return;
  try {
    const list = await invoke<StDevice[]>("st_set_token", { token });
    stTokenInput.value = "";
    fillStDevices(list);
    toast(`Token salvo. ${list.length} dispositivo(s) encontrado(s).`, "success");

    let status = await invoke<StStatus>("st_status");
    if (list.length === 1) {
      status = await invoke<StStatus>("st_select_device", { deviceId: list[0].deviceId });
    }
    applyStStatus(status);
  } catch (error) {
    toast(String(error), "error");
  }
});

stUse.addEventListener("click", async () => {
  const deviceId = stDeviceSelect.value;
  if (!deviceId) {
    toast("Escolha uma TV na lista.", "info");
    return;
  }
  try {
    const status = await invoke<StStatus>("st_select_device", { deviceId });
    applyStStatus(status);
    toast("TV conectada — o controle está pronto.", "success");
  } catch (error) {
    toast(String(error), "error");
  }
});

stRemove.addEventListener("click", async () => {
  try {
    await invoke("st_clear");
    applyStStatus(await invoke<StStatus>("st_status"));
    toast("Token do SmartThings removido.", "info");
  } catch (error) {
    toast(String(error), "error");
  }
});

// ---------------------------------------------------------------------------
// Inicialização
// ---------------------------------------------------------------------------

async function init() {
  renderApps(APP_PRESETS);

  try {
    const status = await invoke<StStatus>("st_status");
    applyStStatus(status);
    if (status.hasToken) {
      try {
        fillStDevices(await invoke<StDevice[]>("st_list_devices"));
      } catch (error) {
        toast(String(error), "error");
      }
    }
  } catch (error) {
    toast(String(error), "error");
  }
}

void init();
