

export interface Settings {
  accent: [string, string];
  refreshMs: number;
  modFolder: string | null;
  timeoutSecs: number;
  clearOnInject: boolean;
}

export const ACCENTS: { name: string; colors: [string, string] }[] = [
  { name: "Blue", colors: ["#3d9bff", "#57d6ff"] },
  { name: "Cyan", colors: ["#22d3ee", "#67e8f9"] },
  { name: "Violet", colors: ["#8b5cf6", "#a78bfa"] },
  { name: "Emerald", colors: ["#10b981", "#34d399"] },
  { name: "Amber", colors: ["#f59e0b", "#fbbf24"] },
  { name: "Rose", colors: ["#f43f5e", "#fb7185"] },
];

export const DEFAULT_SETTINGS: Settings = {
  accent: ["#3d9bff", "#57d6ff"],
  refreshMs: 2000,
  modFolder: null,
  timeoutSecs: 40,
  clearOnInject: true,
};

export function loadSettings(): Settings {
  try {
    const s = JSON.parse(localStorage.getItem("zyringe.settings") || "{}");
    return { ...DEFAULT_SETTINGS, ...s };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(s: Settings) {
  localStorage.setItem("zyringe.settings", JSON.stringify(s));
}

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}


export function applyAccent([a, a2]: [string, string]) {
  const [r, g, b] = hexToRgb(a);
  const root = document.documentElement.style;
  root.setProperty("--accent", a);
  root.setProperty("--accent-2", a2);
  root.setProperty("--accent-soft", `rgba(${r}, ${g}, ${b}, 0.12)`);
  root.setProperty("--accent-glow", `rgba(${r}, ${g}, ${b}, 0.32)`);
  root.setProperty("--accent-faint", `rgba(${r}, ${g}, ${b}, 0.05)`);
}
