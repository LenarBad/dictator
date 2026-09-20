import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
  settings_path: string;
  recording_wired: boolean;
  engine_ready: boolean;
  engine_error: string | null;
  microphones: MicrophoneInfo[];
  accessibility_trusted: boolean;
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

const MOD_GLYPHS: Record<string, string> = {
  ctrl: "⌃",
  control: "⌃",
  shift: "⇧",
  alt: "⌥",
  option: "⌥",
  cmd: "⌘",
  command: "⌘",
  meta: "⌘",
  super: "⌘",
  space: "Space",
};

let capturingHotkey = false;

function $(selector: string): Element | null {
  return document.querySelector(selector);
}

function setText(selector: string, value: string) {
  const el = $(selector);
  if (el) el.textContent = value;
}

function formatHotkey(combo: string): string {
  if (!combo.trim()) return "—";
  return combo
    .split("+")
    .map((part) => {
      const token = part.trim().toLowerCase();
      return MOD_GLYPHS[token] ?? token.toUpperCase();
    })
    .join("");
}

function keyToken(event: KeyboardEvent): string | null {
  if (event.key === " ") return "space";
  if (/^f\d{1,2}$/i.test(event.key)) return event.key.toLowerCase();
  if (event.key.length === 1 && /[a-z0-9]/i.test(event.key)) return event.key.toLowerCase();
  return null;
}

function setCapturing(active: boolean) {
  capturingHotkey = active;
  const button = document.querySelector<HTMLButtonElement>("#hotkey-display");
  if (!button) return;
  button.classList.toggle("capturing", active);
  if (active) {
    button.textContent = "Нажмите…";
  } else {
    const value = document.querySelector<HTMLInputElement>("#hotkey")?.value ?? "";
    button.textContent = formatHotkey(value);
  }
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
  setText("#settings-path", state.settings_path);
  const axDot = document.querySelector<HTMLElement>("#ax-dot");
  if (state.accessibility_trusted) {
    setText("#ax-status", "Выдан Dictator");
    axDot?.setAttribute("data-state", "ok");
  } else {
    setText("#ax-status", "Добавьте Dictator в системных настройках");
    axDot?.setAttribute("data-state", "warn");
  }
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
  el.hidden = !message;
  el.classList.toggle("error", error);
  el.textContent = message;
}

async function reload() {
  const state = await invoke<UiState>("get_state");
  fillForm(state);
}

window.addEventListener("DOMContentLoaded", async () => {
  try {
    await reload();
  } catch {
    fillForm({
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
      settings_path: "~/Library/Application Support/dictator/settings.json",
      recording_wired: true,
      engine_ready: true,
      engine_error: null,
      microphones: [
        { name: "MacBook Pro Microphone", kind: "builtin" },
        { name: "AirPods Pro", kind: "bluetooth" },
      ],
      accessibility_trusted: false,
    });
    showFormStatus("Превью без приложения — запись и сохранение здесь не работают.", true);
  }

  document.querySelector("#settings-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      await invoke("save_settings", { settings: readForm() });
      showFormStatus("Сохранено. Хоткей действует сразу.");
      await reload();
    } catch (error) {
      showFormStatus(String(error), true);
    }
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

  document.querySelector("#microphone_name")?.addEventListener("change", async () => {
    updateMicHint();
    try {
      await invoke("save_settings", { settings: readForm() });
      showFormStatus("Микрофон сохранён.");
    } catch (error) {
      showFormStatus(String(error), true);
    }
  });

  document.querySelector("#hotkey-display")?.addEventListener("click", (event) => {
    event.preventDefault();
    setCapturing(true);
  });

  window.addEventListener("keydown", (event) => {
    if (!capturingHotkey) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setCapturing(false);
      return;
    }
    const token = keyToken(event);
    if (!token) return;
    const parts: string[] = [];
    if (event.ctrlKey) parts.push("ctrl");
    if (event.altKey) parts.push("alt");
    if (event.shiftKey) parts.push("shift");
    if (event.metaKey) parts.push("cmd");
    if (parts.length === 0) return;
    parts.push(token);
    const combo = parts.join("+");
    const hidden = document.querySelector<HTMLInputElement>("#hotkey");
    if (hidden) hidden.value = combo;
    setCapturing(false);
  });

  window.addEventListener("mousedown", (event) => {
    if (!capturingHotkey) return;
    const button = document.querySelector("#hotkey-display");
    if (button && !button.contains(event.target as Node)) {
      setCapturing(false);
    }
  });

  await listen<AppStatus>("status-changed", async (event) => {
    const status = event.payload;
    applyStatus(status, STATUS_LABEL[status]);
    try {
      const state = await invoke<UiState>("get_state");
      applyEngine(state);
      applyStatus(state.status, state.status_label, state.settings.hotkey);
    } catch {
      /* settings window may be hidden */
    }
  });
});
