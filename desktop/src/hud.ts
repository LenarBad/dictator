import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatHotkey } from "./format.ts";

type AppStatus = "idle" | "recording" | "transcribing";

type HudFrame = {
  status: AppStatus;
  elapsed: number;
  bars: number[];
  hotkey: string;
};

const BAR_COUNT = 17;

// Matches the bar envelope in docs/hero.svg (heights / 34).
const HERO_BARS = [
  0.24, 0.65, 0.88, 0.53, 1, 0.71, 0.35, 0.82, 0.59, 0.94, 0.47, 0.76, 0.29, 0.65, 0.88, 0.53, 0.24,
];

function wave(): HTMLElement {
  return document.getElementById("wave") as HTMLElement;
}

function mountBars() {
  const el = wave();
  el.replaceChildren();
  for (let i = 0; i < BAR_COUNT; i++) {
    const span = document.createElement("span");
    span.style.setProperty("--i", String(i));
    el.append(span);
  }
}

function formatElapsed(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(total / 60);
  const rest = total % 60;
  return `${minutes}:${rest.toString().padStart(2, "0")}`;
}

let lastStatus: AppStatus | null = null;

function paint(frame: HudFrame, now: number) {
  document.body.dataset.status = frame.status;
  const pill = document.getElementById("pill") as HTMLButtonElement | null;
  if (pill) pill.disabled = frame.status === "transcribing";
  const time = document.getElementById("time");
  const hint = document.getElementById("hint");
  if (time) {
    time.textContent = frame.status === "transcribing" ? "…" : formatElapsed(frame.elapsed);
  }
  if (hint) {
    hint.textContent =
      frame.status === "transcribing" ? "Распознаём" : `${formatHotkey(frame.hotkey)} · стоп`;
  }

  const spans = wave().children;
  if (frame.status === "transcribing") {
    if (lastStatus !== "transcribing") {
      for (let i = 0; i < spans.length; i++) {
        (spans[i] as HTMLElement).style.transform = "";
      }
    }
    lastStatus = frame.status;
    return;
  }
  lastStatus = frame.status;

  for (let i = 0; i < spans.length; i++) {
    const value = Math.min(1, Math.max(0.16, frame.bars[i] ?? 0.16));
    const live = 0.1 * Math.sin(now / 140 + i * 0.58) * value;
    const h = Math.min(1, Math.max(0.16, value + live));
    (spans[i] as HTMLElement).style.transform = `scaleY(${h.toFixed(3)})`;
  }
}

function previewFrame(now: number): HudFrame {
  const phase = now / 260;
  return {
    status: "recording",
    elapsed: 12,
    bars: HERO_BARS.map((base, i) => {
      const pulse = 0.18 * Math.sin(phase + i * 0.62);
      return Math.min(1, Math.max(0.16, base + pulse));
    }),
    hotkey: "ctrl+shift+d",
  };
}

window.addEventListener("DOMContentLoaded", async () => {
  mountBars();
  document.getElementById("pill")?.addEventListener("click", async () => {
    try {
      await invoke("toggle_recording");
    } catch {
      /* preview or already stopping */
    }
  });

  let latest: HudFrame | null = null;
  let preview = false;
  try {
    latest = await invoke<HudFrame>("hud_snapshot");
  } catch {
    preview = true;
  }

  if (!preview) {
    await listen<HudFrame>("hud-frame", (event) => {
      latest = event.payload;
    });
    await listen<AppStatus>("status-changed", async () => {
      try {
        latest = await invoke<HudFrame>("hud_snapshot");
      } catch {
        /* window may be hidden */
      }
    });
  }

  const tick = (now: number) => {
    const frame = preview ? previewFrame(now) : latest;
    if (frame) paint(frame, now);
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
});
