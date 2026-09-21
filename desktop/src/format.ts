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

export function formatHotkey(combo: string): string {
  if (!combo.trim()) return "—";
  return combo
    .split("+")
    .map((part) => {
      const token = part.trim().toLowerCase();
      if (!token) return "";
      return MOD_GLYPHS[token] ?? token.toUpperCase();
    })
    .join("");
}

/** Physical key, independent of keyboard layout (`KeyD` stays `d` on a Russian layout). */
export function physicalKeyToken(code: string): string | null {
  if (code === "Space") return "space";
  const fn = /^F(\d{1,2})$/.exec(code);
  if (fn) {
    const index = Number(fn[1]);
    if (index >= 1 && index <= 12) return `f${index}`;
  }
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1].toLowerCase();
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  return null;
}

export function modifierTokens(event: KeyboardEvent): string[] {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  if (event.metaKey) parts.push("cmd");
  return parts;
}

export function comboFromKeyboardEvent(event: KeyboardEvent): string | null {
  if (event.repeat) return null;
  const token = physicalKeyToken(event.code);
  if (!token) return null;
  const mods = modifierTokens(event);
  if (mods.length === 0) return null;
  return [...mods, token].join("+");
}
