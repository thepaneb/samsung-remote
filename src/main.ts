import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface TvDevice {
  ip: string;
  name?: string | null;
  model?: string | null;
  modelName?: string | null;
  mac?: string | null;
  powerState?: string | null;
  token?: string | null;
  paired: boolean;
  reachable: boolean;
}

interface TvApp {
  appId: string;
  name: string;
  icon?: string | null;
  appType?: number | null;
}

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

type StatusState = "connecting" | "waiting" | "connected" | "disconnected" | "error";

interface Status {
  state: StatusState;
  ip?: string | null;
  message?: string | null;
  device?: TvDevice | null;
}

const $ = <T extends HTMLElement>(selector: string): T =>
  document.querySelector(selector) as T;

const statusPill = $<HTMLDivElement>("#status-pill");
const statusText = $<HTMLSpanElement>("#status-text");
const connectedName = $<HTMLParagraphElement>("#connected-name");
const discoverBtn = $<HTMLButtonElement>("#discover-btn");
const ipForm = $<HTMLFormElement>("#ip-form");
const ipInput = $<HTMLInputElement>("#ip-input");
const deviceList = $<HTMLUListElement>("#device-list");
const deviceEmpty = $<HTMLParagraphElement>("#device-empty");
const appsRefresh = $<HTMLButtonElement>("#apps-refresh");
const appGrid = $<HTMLDivElement>("#app-grid");
const appsEmpty = $<HTMLParagraphElement>("#apps-empty");
const textForm = $<HTMLFormElement>("#text-form");
const textInput = $<HTMLInputElement>("#text-input");
const textSend = $<HTMLButtonElement>("#text-send");
const remoteBody = $<HTMLDivElement>("#remote-body");
const remoteTitle = $<HTMLHeadingElement>("#remote-title");
const remoteHint = $<HTMLParagraphElement>("#remote-hint");
const disconnectBtn = $<HTMLButtonElement>("#disconnect-btn");
const numpad = $<HTMLDivElement>("#numpad");
const numpadToggle = $<HTMLButtonElement>("#numpad-toggle");
const toastStack = $<HTMLDivElement>("#toast-stack");
const stSetup = $<HTMLDivElement>("#st-setup");
const stPanel = $<HTMLDivElement>("#st-panel");
const stForm = $<HTMLFormElement>("#st-form");
const stTokenInput = $<HTMLInputElement>("#st-token");
const stDeviceSelect = $<HTMLSelectElement>("#st-device");
const stUse = $<HTMLButtonElement>("#st-use");
const stRemove = $<HTMLButtonElement>("#st-remove");
const stStatusText = $<HTMLParagraphElement>("#st-status-text");
const appCustomForm = $<HTMLFormElement>("#app-custom-form");
const appCustomId = $<HTMLInputElement>("#app-custom-id");

// Teclas que o modo nuvem consegue enviar (mapeadas para capacidades SmartThings)
const CLOUD_KEY_ACTIONS: Record<string, string> = {
  KEY_POWER: "power_toggle",
  KEY_VOLUP: "volume_up",
  KEY_VOLDOWN: "volume_down",
  KEY_MUTE: "mute_toggle",
  KEY_CHUP: "channel_up",
  KEY_CHDOWN: "channel_down",
  KEY_PLAY_BACK: "play_pause",
  KEY_REWIND: "rewind",
  KEY_FF: "fast_forward",
  KEY_SOURCE: "cycle_input",
};

const CLOUD_KEY_CAPABILITY: Record<string, string> = {
  KEY_POWER: "switch",
  KEY_VOLUP: "audioVolume",
  KEY_VOLDOWN: "audioVolume",
  KEY_MUTE: "audioMute",
  KEY_CHUP: "tvChannel",
  KEY_CHDOWN: "tvChannel",
  KEY_PLAY_BACK: "mediaPlayback",
  KEY_REWIND: "mediaPlayback",
  KEY_FF: "mediaPlayback",
  KEY_SOURCE: "mediaInputSource",
};

// Atalhos comuns (usados no modo nuvem, onde não é possível listar os apps da TV)
const APP_PRESETS: TvApp[] = [
  { appId: "3201512006785", name: "Netflix" },
  { appId: "111299001912", name: "YouTube" },
  { appId: "3201910019365", name: "Prime Video" },
  { appId: "3201606009684", name: "Spotify" },
];

const devices = new Map<string, TvDevice>();
let sessionIp: string | null = null;
let waitingToastShown = false;
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

function setStatus(state: StatusState | "idle", text: string) {
  statusPill.dataset.state = state;
  statusText.textContent = text;
}

function setRemoteEnabled(_enabled: boolean) {
  refreshRemoteAvailability();
}

function refreshRemoteAvailability() {
  const local = sessionIp !== null;
  const usable = local || cloudActive;

  remoteBody.classList.toggle("disabled", !usable);
  disconnectBtn.disabled = !local;
  appsRefresh.disabled = !usable;
  textInput.disabled = !local;
  textSend.disabled = !local;
  appCustomForm.classList.toggle("hidden", !usable);

  remoteBody.querySelectorAll<HTMLButtonElement>("button.key[data-key]").forEach((button) => {
    const key = button.dataset.key ?? "";
    const capability = CLOUD_KEY_CAPABILITY[key];
    const cloudOk = cloudActive && capability !== undefined && cloudCaps.includes(capability);
    button.classList.toggle("cloud-off", !local && !cloudOk);
  });
}

// ---------------------------------------------------------------------------
// Lista de TVs
// ---------------------------------------------------------------------------

function upsertDevice(device: TvDevice) {
  const previous = devices.get(device.ip);
  const merged: TvDevice = {
    ...previous,
    ...device,
    paired: device.paired || previous?.paired || false,
    token: device.token ?? previous?.token ?? null,
  };
  devices.set(device.ip, merged);
  renderDevices();
}

function deviceRow(device: TvDevice): string {
  const label = device.name?.trim() || "TV Samsung";
  const details = [device.modelName || device.model, device.ip, device.mac]
    .filter(Boolean)
    .join(" • ");
  const isConnected = sessionIp === device.ip;

  const badges = [
    device.paired ? '<span class="badge ok">pareado</span>' : "",
    !device.reachable ? '<span class="badge warn">offline</span>' : "",
    isConnected ? '<span class="badge connected">conectado</span>' : "",
  ].join("");

  const wakeBtn = device.mac
    ? `<button class="btn icon" data-action="wol" data-mac="${esc(device.mac)}" title="Ligar via Wake-on-LAN">⏻</button>`
    : "";

  return `
    <div class="device-main">
      <strong>${esc(label)}</strong>
      <span class="device-details">${esc(details)}</span>
      <span class="badges">${badges}</span>
    </div>
    <div class="device-actions">
      ${wakeBtn}
      <button class="btn primary" data-action="connect" data-ip="${esc(device.ip)}" ${isConnected ? "disabled" : ""}>Conectar</button>
      <button class="btn ghost" data-action="forget" data-ip="${esc(device.ip)}" title="Esquecer TV">✕</button>
    </div>`;
}

function renderDevices() {
  const list = [...devices.values()].sort((a, b) => {
    const score = (d: TvDevice) =>
      (sessionIp === d.ip ? 4 : 0) + (d.paired ? 2 : 0) + (d.reachable ? 1 : 0);
    return score(b) - score(a) || (a.name ?? "").localeCompare(b.name ?? "");
  });

  deviceList.innerHTML = list
    .map((d) => `<li class="device" data-ip="${esc(d.ip)}">${deviceRow(d)}</li>`)
    .join("");
  deviceEmpty.classList.toggle("hidden", list.length > 0);
}

// ---------------------------------------------------------------------------
// Conexão / status
// ---------------------------------------------------------------------------

function applyStatus(status: Status) {
  switch (status.state) {
    case "connecting":
      setStatus("connecting", status.message ?? "Conectando…");
      break;

    case "waiting":
      setStatus("waiting", "Aguardando permissão na TV…");
      if (!waitingToastShown) {
        waitingToastShown = true;
        toast(
          "Olhe para a TV: se aparecer o aviso “Permitir conexão?”, aceite com o controle físico.",
          "info",
        );
      }
      break;

    case "connected": {
      waitingToastShown = false;
      const device = status.device ?? null;
      sessionIp = device?.ip ?? status.ip ?? sessionIp;
      if (device) upsertDevice(device);
      setStatus("connected", "Conectado");
      connectedName.textContent = device?.name
        ? `${device.name} (${device.ip})`
        : `Conectado a ${sessionIp ?? ""}`;
      remoteTitle.textContent = device?.name ?? "Controle";
      remoteHint.textContent = `Controles enviados para ${sessionIp ?? "a TV"}.`;
      setRemoteEnabled(true);
      renderDevices();
      void loadApps();
      break;
    }

    case "disconnected":
      sessionIp = null;
      waitingToastShown = false;
      setStatus("idle", "Desconectado");
      connectedName.textContent = "Nenhuma TV conectada";
      remoteTitle.textContent = "Controle";
      remoteHint.textContent = "Conecte-se a uma TV para usar o controle.";
      setRemoteEnabled(false);
      appGrid.innerHTML = "";
      appsEmpty.textContent = "Conecte-se a uma TV para carregar os aplicativos.";
      appsEmpty.classList.remove("hidden");
      renderDevices();
      break;

    case "error":
      setStatus("error", status.message ?? "Erro");
      if (status.message) toast(status.message, "error");
      renderDevices();
      break;
  }
}

async function connect(ip: string) {
  setStatus("connecting", "Conectando…");
  try {
    await invoke<TvDevice>("connect_tv", { ip });
  } catch (error) {
    setStatus("idle", "Desconectado");
    toast(String(error), "error");
  }
}

// ---------------------------------------------------------------------------
// Comandos para a TV
// ---------------------------------------------------------------------------

async function sendKey(key: string) {
  if (!sessionIp) return;
  try {
    await invoke("send_key", { key });
  } catch (error) {
    toast(String(error), "error");
  }
}

// ---------------------------------------------------------------------------
// Modo nuvem (SmartThings)
// ---------------------------------------------------------------------------

function applyStStatus(status: StStatus) {
  stSetup.classList.toggle("hidden", status.hasToken);
  stPanel.classList.toggle("hidden", !status.hasToken);
  stRemove.classList.toggle("hidden", !status.hasToken);

  if (status.hasToken && status.deviceId) {
    cloudActive = true;
    cloudCaps = status.capabilities;
    stStatusText.textContent = `Nuvem ativa: ${status.deviceLabel ?? "TV"} • ${status.capabilities.length} capacidades.`;
  } else {
    cloudActive = false;
    cloudCaps = [];
    stStatusText.textContent = status.hasToken
      ? "Token salvo. Escolha a TV abaixo e clique em “Usar esta TV”."
      : "";
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

function launchAppById(appId: string, appType?: number | null) {
  if (sessionIp) {
    return invoke("launch_app", { appId, appType: appType ?? null })
      .then(() => toast("App iniciado na TV.", "success"))
      .catch((error) => toast(String(error), "error"));
  }
  if (cloudActive) {
    void stExecute("launch_app", appId);
    return Promise.resolve();
  }
  toast("Conecte primeiro uma TV (local ou nuvem).", "info");
  return Promise.resolve();
}

// ---------------------------------------------------------------------------
// Apps
// ---------------------------------------------------------------------------

function renderApps(apps: TvApp[]) {
  appsEmpty.classList.add("hidden");
  appGrid.innerHTML = apps
    .map((app) => {
      const icon = app.icon
        ? `<img src="${esc(app.icon)}" alt="" data-name="${esc(app.name)}" loading="lazy" />`
        : `<span class="app-fallback">${esc(app.name.slice(0, 1).toUpperCase() || "?")}</span>`;
      return `<button class="app-tile" data-app-id="${esc(app.appId)}" data-app-type="${app.appType ?? ""}" title="${esc(app.name)}">${icon}<span class="app-name">${esc(app.name)}</span></button>`;
    })
    .join("");

  // ícones da TV podem falhar (URLs exigem a TV online): troca pela inicial
  appGrid.querySelectorAll<HTMLImageElement>("img").forEach((img) => {
    img.addEventListener("error", () => {
      const span = document.createElement("span");
      span.className = "app-fallback";
      span.textContent = (img.dataset.name ?? "?").slice(0, 1).toUpperCase();
      img.replaceWith(span);
    });
  });
}

async function loadApps() {
  if (!sessionIp) return;
  appGrid.innerHTML = '<p class="empty">Carregando apps…</p>';
  try {
    const apps = await invoke<TvApp[]>("list_apps", { ip: sessionIp });
    renderApps(apps);
  } catch (error) {
    appGrid.innerHTML = "";
    appsEmpty.textContent = String(error);
    appsEmpty.classList.remove("hidden");
  }
}

function loadPresetApps() {
  renderApps(APP_PRESETS);
  appCustomForm.classList.remove("hidden");
  toast("No modo nuvem exibimos atalhos comuns. Use o campo de ID para abrir outros apps.", "info");
}

// ---------------------------------------------------------------------------
// Eventos
// ---------------------------------------------------------------------------

listen<Status>("tv://status", (event) => applyStatus(event.payload));
listen<TvDevice>("tv://discovered", (event) => upsertDevice(event.payload));
listen<string>("tv://message", () => {
  /* mensagens brutas da TV — úteis para depuração */
});

deviceList.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-action]");
  if (!button) return;

  const action = button.dataset.action;
  const ip = button.dataset.ip;
  const mac = button.dataset.mac;

  if (action === "connect" && ip) void connect(ip);
  if (action === "forget" && ip) {
    void invoke("forget_device", { ip })
      .then(() => {
        devices.delete(ip);
        renderDevices();
      })
      .catch((error) => toast(String(error), "error"));
  }
  if (action === "wol" && mac) {
    void invoke("wake_on_lan", { mac })
      .then(() => toast("Pacote Wake-on-LAN enviado. A TV pode levar alguns segundos para acender.", "success"))
      .catch((error) => toast(String(error), "error"));
  }
});

remoteBody.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button.key");
  if (!button) return;

  if (button.id === "numpad-toggle") {
    numpad.classList.toggle("hidden");
    return;
  }

  const key = button.dataset.key;
  if (!key) return;

  button.classList.add("pressed");
  setTimeout(() => button.classList.remove("pressed"), 120);

  if (sessionIp) {
    void sendKey(key);
    return;
  }

  if (cloudActive) {
    const action = CLOUD_KEY_ACTIONS[key];
    if (!action) {
      toast("Tecla indisponível no modo nuvem (SmartThings).", "info");
      return;
    }
    void stExecute(action);
  }
});

numpadToggle.addEventListener("click", () => numpad.classList.toggle("hidden"));

appGrid.addEventListener("click", (event) => {
  const tile = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-app-id]");
  const appId = tile?.dataset.appId;
  if (!appId) return;
  const appType = tile?.dataset.appType ? Number(tile.dataset.appType) : null;
  void launchAppById(appId, appType);
});

appCustomForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const appId = appCustomId.value.trim();
  if (!appId) return;
  void launchAppById(appId);
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
    toast("Modo nuvem ativo — as teclas suportadas já funcionam.", "success");
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

appsRefresh.addEventListener("click", () => {
  if (sessionIp) {
    void loadApps();
    return;
  }
  if (cloudActive) {
    loadPresetApps();
  }
});

disconnectBtn.addEventListener("click", () => {
  void invoke("disconnect_tv").catch((error) => toast(String(error), "error"));
});

discoverBtn.addEventListener("click", async () => {
  discoverBtn.disabled = true;
  discoverBtn.textContent = "Buscando…";
  try {
    const found = await invoke<TvDevice[]>("discover_tvs", { timeoutMs: 4500 });
    found.forEach(upsertDevice);
    if (found.length === 0) {
      toast("Nenhuma TV respondeu. Confirme que ela está ligada e na mesma rede.", "info");
    }
  } catch (error) {
    toast(String(error), "error");
  } finally {
    discoverBtn.disabled = false;
    discoverBtn.textContent = "Buscar TVs";
  }
});

ipForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const ip = ipInput.value.trim();
  if (!ip) return;
  try {
    const device = await invoke<TvDevice>("probe_tv", { ip });
    upsertDevice(device);
    ipInput.value = "";
  } catch (error) {
    toast(String(error), "error");
  }
});

textForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const text = textInput.value;
  if (!text) return;
  try {
    await invoke("send_text", { text });
    textInput.value = "";
  } catch (error) {
    toast(String(error), "error");
  }
});

window.addEventListener("keydown", (event) => {
  if (!sessionIp && !cloudActive) return;
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

  if (sessionIp) {
    void sendKey(key);
    return;
  }
  const cloudAction = CLOUD_KEY_ACTIONS[key];
  if (cloudAction) {
    void stExecute(cloudAction);
  } else {
    toast("Tecla indisponível no modo nuvem (SmartThings).", "info");
  }
});

// ---------------------------------------------------------------------------
// Inicialização
// ---------------------------------------------------------------------------

async function init() {
  renderDevices();

  try {
    const saved = await invoke<TvDevice[]>("list_saved_devices");
    saved.forEach((device) => devices.set(device.ip, { ...device, reachable: false }));
    renderDevices();
  } catch (error) {
    toast(String(error), "error");
  }

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

  // enquanto nada estiver disponível, garante o estado "desligado"
  if (sessionIp === null && !cloudActive) {
    refreshRemoteAvailability();
  }
}

void init();
