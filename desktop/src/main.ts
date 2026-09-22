import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { comboFromKeyboardEvent, formatHotkey, modifierTokens } from "./format.ts";

type AppStatus = "idle" | "recording" | "transcribing";

type Settings = {
  hotkey: string;
  paste_enabled: boolean;
  microphone_name: string | null;
  model_name: string;
  preload_model: boolean;
  max_recording_seconds: number;
};

type MicrophoneInfo = {
  name: string;
  kind: "builtin" | "bluetooth" | "virtual" | "other" | string;
};

type UiState = {
  status: AppStatus;
  status_label: string;
  settings: Settings;
  recording_wired: boolean;
  engine_ready: boolean;
  engine_error: string | null;
  microphones: MicrophoneInfo[];
  accessibility_trusted: boolean;
  microphone_trusted: boolean;
  app_version: string;
};

const STATUS_LABEL: Record<AppStatus, string> = {
  idle: "Ожидание",
  recording: "Запись",
  transcribing: "Распознавание",
};

const TOGGLE_LABEL: Record<AppStatus, string> = {
  idle: "Записать",
  recording: "Стоп",
  transcribing: "Распознаём…",
};

let capturingHotkey = false;
let saveTimer: number | undefined;
let toastTimer: number | undefined;
let persistInFlight = false;
let persistQueued = false;

function $(selector: string): Element | null {
  return document.querySelector(selector);
}

function setText(selector: string, value: string) {
  const el = $(selector);
  if (el) el.textContent = value;
}

function setCapturing(active: boolean) {
  capturingHotkey = active;
  const button = document.querySelector<HTMLButtonElement>("#hotkey-display");
  if (!button) return;
  button.classList.toggle("capturing", active);
  if (active) {
    button.textContent = "Нажмите…";
    button.focus();
  } else {
    const value = document.querySelector<HTMLInputElement>("#hotkey")?.value ?? "";
    button.textContent = formatHotkey(value);
  }
}

async function startHotkeyCapture() {
  setCapturing(true);
  try {
    await invoke("pause_hotkey");
  } catch {
    /* preview */
  }
}

async function cancelHotkeyCapture() {
  if (!capturingHotkey) return;
  setCapturing(false);
  try {
    await invoke("resume_hotkey");
  } catch {
    /* preview */
  }
}

function paintCapturePreview(event: KeyboardEvent) {
  const button = document.querySelector<HTMLButtonElement>("#hotkey-display");
  if (!button) return;
  const mods = modifierTokens(event);
  button.textContent = mods.length > 0 ? formatHotkey(mods.join("+")) : "Нажмите…";
}

function micOptionLabel(mic: MicrophoneInfo): string {
  if (mic.kind === "bluetooth") return `${mic.name} · Bluetooth`;
  if (mic.kind === "virtual") return `${mic.name} · виртуальный`;
  return mic.name;
}

function fillMicrophones(state: UiState) {
  const select = document.querySelector<HTMLSelectElement>("#microphone_name");
  if (!select) return;
  const current = state.settings.microphone_name ?? "";
  const mics = [...state.microphones];
  if (current && !mics.some((mic) => mic.name === current)) {
    mics.unshift({ name: current, kind: "other" });
  }
  select.replaceChildren();
  for (const mic of mics) {
    const option = document.createElement("option");
    option.value = mic.name;
    option.textContent = micOptionLabel(mic);
    option.dataset.kind = mic.kind;
    select.append(option);
  }
  if (current) select.value = current;
  else if (mics[0]) select.value = mics[0].name;
  updateMicHint();
}

function updateMicHint() {
  const select = document.querySelector<HTMLSelectElement>("#microphone_name");
  const hint = document.querySelector<HTMLElement>("#mic-hint");
  if (!select || !hint) return;
  const kind = select.selectedOptions[0]?.dataset.kind;
  if (kind === "bluetooth") {
    hint.hidden = false;
    hint.textContent = "Наушники могут зависнуть — лучше встроенный микрофон.";
    hint.classList.add("warn");
  } else if (kind === "virtual") {
    hint.hidden = false;
    hint.textContent = "Виртуальный кабель. Лучше встроенный микрофон.";
    hint.classList.add("warn");
  } else {
    hint.hidden = true;
    hint.textContent = "";
    hint.classList.remove("warn");
  }
}

function applyStatus(status: AppStatus, label: string, hotkey?: string) {
  document.body.dataset.status = status;
  setText("#status-line", label);
  if (hotkey !== undefined) {
    setText("#status-meta", `${formatHotkey(hotkey)} · клик по иконке — меню`);
  }
  const toggle = document.querySelector<HTMLButtonElement>("#toggle");
  if (toggle) {
    toggle.dataset.state = status;
    toggle.disabled = status === "transcribing";
  }
  setText("#toggle-label", TOGGLE_LABEL[status]);
}

function applyEngine(state: Pick<UiState, "engine_ready" | "engine_error">) {
  const engine = state.engine_ready
    ? "Модель готова"
    : state.engine_error
      ? state.engine_error
      : "Модель загружается…";
  setText("#engine-line", engine);
}

function applyPermissions(state: Pick<UiState, "accessibility_trusted" | "microphone_trusted">) {
  const section = document.querySelector<HTMLElement>("#permissions");
  const micRow = document.querySelector<HTMLElement>("#perm-mic");
  const axRow = document.querySelector<HTMLElement>("#perm-ax");
  const micDot = document.querySelector<HTMLElement>("#mic-dot");
  const axDot = document.querySelector<HTMLElement>("#ax-dot");
  const micOk = state.microphone_trusted;
  const axOk = state.accessibility_trusted;

  if (micOk) {
    setText("#mic-status", "Выдан Dictator");
    micDot?.setAttribute("data-state", "ok");
  } else {
    setText("#mic-status", "Нужен, чтобы писать голос");
    micDot?.setAttribute("data-state", "info");
  }
  if (axOk) {
    setText("#ax-status", "Выдан Dictator");
    axDot?.setAttribute("data-state", "ok");
  } else {
    setText("#ax-status", "Добавьте Dictator в системных настройках");
    axDot?.setAttribute("data-state", "warn");
  }

  if (section) section.hidden = micOk && axOk;
  if (micRow) {
    micRow.hidden = micOk;
    micRow.classList.toggle("solo", !micOk && axOk);
  }
  if (axRow) {
    axRow.hidden = axOk;
    axRow.classList.toggle("solo", !axOk && micOk);
  }
}

function fillForm(state: UiState) {
  const hotkey = document.querySelector<HTMLInputElement>("#hotkey");
  const paste = document.querySelector<HTMLInputElement>("#paste_enabled");
  const preload = document.querySelector<HTMLInputElement>("#preload_model");
  const limit = document.querySelector<HTMLInputElement>("#max_recording_seconds");
  if (hotkey) hotkey.value = state.settings.hotkey;
  setText("#hotkey-display", formatHotkey(state.settings.hotkey));
  if (paste) paste.checked = state.settings.paste_enabled;
  if (preload) preload.checked = state.settings.preload_model;
  if (limit) limit.value = String(state.settings.max_recording_seconds);
  fillMicrophones(state);
  setText("#app-version", state.app_version);
  applyPermissions(state);
  applyEngine(state);
  applyStatus(state.status, state.status_label, state.settings.hotkey);
}

function readForm(): Settings {
  const hotkey = document.querySelector<HTMLInputElement>("#hotkey")?.value.trim() ?? "";
  const paste = document.querySelector<HTMLInputElement>("#paste_enabled")?.checked ?? true;
  const preload = document.querySelector<HTMLInputElement>("#preload_model")?.checked ?? true;
  const mic = document.querySelector<HTMLSelectElement>("#microphone_name")?.value.trim() ?? "";
  const limit = Number(
    document.querySelector<HTMLInputElement>("#max_recording_seconds")?.value ?? "180",
  );
  return {
    hotkey,
    paste_enabled: paste,
    microphone_name: mic === "" ? null : mic,
    model_name: "v3_e2e_rnnt",
    preload_model: preload,
    max_recording_seconds: Number.isFinite(limit) ? limit : 180,
  };
}

function showFormStatus(message: string, error = false) {
  const el = document.querySelector<HTMLElement>("#form-status");
  if (!el) return;
  window.clearTimeout(toastTimer);
  el.hidden = !message;
  el.classList.toggle("error", error);
  el.textContent = message;
  if (message && !error) {
    toastTimer = window.setTimeout(() => {
      el.hidden = true;
    }, 1600);
  }
}

function queueSave() {
  if (capturingHotkey) return;
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    void persist();
  }, 280);
}

async function persist() {
  if (capturingHotkey) return;
  if (persistInFlight) {
    persistQueued = true;
    return;
  }
  persistInFlight = true;
  try {
    const saved = await invoke<Settings>("save_settings", { settings: readForm() });
    const hotkey = document.querySelector<HTMLInputElement>("#hotkey");
    if (hotkey) hotkey.value = saved.hotkey;
    if (!capturingHotkey) {
      setText("#hotkey-display", formatHotkey(saved.hotkey));
    }
    applyStatus(
      (document.body.dataset.status as AppStatus) ?? "idle",
      document.querySelector("#status-line")?.textContent || STATUS_LABEL.idle,
      saved.hotkey,
    );
    showFormStatus("Сохранено");
  } catch (error) {
    showFormStatus(String(error), true);
    try {
      await invoke("resume_hotkey");
      await reload();
    } catch {
      /* preview */
    }
  } finally {
    persistInFlight = false;
    if (persistQueued) {
      persistQueued = false;
      void persist();
    }
  }
}

async function reload() {
  const state = await invoke<UiState>("get_state");
  fillForm(state);
}

async function refreshPermissions() {
  try {
    const state = await invoke<UiState>("get_state");
    applyPermissions(state);
    applyEngine(state);
  } catch {
    /* preview */
  }
}

function previewState(): UiState {
  return {
    status: "idle",
    status_label: "Ожидание",
    settings: {
      hotkey: "ctrl+shift+d",
      paste_enabled: true,
      microphone_name: "MacBook Pro Microphone",
      model_name: "v3_e2e_rnnt",
      preload_model: true,
      max_recording_seconds: 180,
    },
    recording_wired: true,
    engine_ready: true,
    engine_error: null,
    microphones: [
      { name: "MacBook Pro Microphone", kind: "builtin" },
      { name: "AirPods Pro", kind: "bluetooth" },
    ],
    accessibility_trusted: false,
    microphone_trusted: false,
    app_version: "0.3.0",
  };
}

window.addEventListener("DOMContentLoaded", async () => {
  try {
    await reload();
  } catch {
    fillForm(previewState());
    showFormStatus("Превью без приложения — запись и сохранение здесь не работают.", true);
  }

  document.querySelector("#settings-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void persist();
  });
  document.querySelector("#settings-form")?.addEventListener("change", () => queueSave());
  document.querySelector("#settings-form")?.addEventListener("input", (event) => {
    const target = event.target as HTMLElement | null;
    if (target?.id === "max_recording_seconds") queueSave();
  });

  document.querySelector("#toggle")?.addEventListener("click", async () => {
    const status = await invoke<AppStatus>("toggle_recording");
    applyStatus(status, STATUS_LABEL[status]);
  });

  document.querySelector("#open-mic")?.addEventListener("click", () => {
    void invoke("open_permission", { kind: "microphone" });
  });
  document.querySelector("#open-ax")?.addEventListener("click", () => {
    void invoke("open_permission", { kind: "accessibility" });
  });

  document.querySelector("#microphone_name")?.addEventListener("change", updateMicHint);

  document.querySelector("#hotkey-display")?.addEventListener("click", (event) => {
    event.preventDefault();
    void startHotkeyCapture();
  });

  window.addEventListener(
    "keydown",
    (event) => {
      if (!capturingHotkey) return;
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        void cancelHotkeyCapture();
        return;
      }
      const combo = comboFromKeyboardEvent(event);
      if (!combo) {
        paintCapturePreview(event);
        return;
      }
      const hidden = document.querySelector<HTMLInputElement>("#hotkey");
      if (hidden) hidden.value = combo;
      setCapturing(false);
      void persist();
    },
    true,
  );

  window.addEventListener(
    "keyup",
    (event) => {
      if (!capturingHotkey) return;
      paintCapturePreview(event);
    },
    true,
  );

  window.addEventListener("mousedown", (event) => {
    if (!capturingHotkey) return;
    const button = document.querySelector("#hotkey-display");
    if (button && !button.contains(event.target as Node)) {
      void cancelHotkeyCapture();
    }
  });

  await listen<AppStatus>("status-changed", async (event) => {
    const status = event.payload;
    applyStatus(status, STATUS_LABEL[status]);
    try {
      const state = await invoke<UiState>("get_state");
      applyEngine(state);
      applyPermissions(state);
      applyStatus(state.status, state.status_label, state.settings.hotkey);
    } catch {
      /* settings window may be hidden */
    }
  });

  try {
    await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) void refreshPermissions();
      else void cancelHotkeyCapture();
    });
  } catch {
    /* browser preview */
  }
});
