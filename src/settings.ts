import { invoke } from "@tauri-apps/api/core";

/** App-wide preferences, saved to settings.json in the app data folder. */
export type Settings = {
  shotSidebarOpen: boolean;
  infoSidebarOpen: boolean;
  /** Folder and optional .fujiview file open when the app was last closed, reopened at startup. */
  lastRoot: string | null;
  lastProjectPath: string | null;
  /** Camera presets ticked in the variant generator (ids such as "C1", "asshot"). */
  generatorPresetIds: string[] | null;
};

export const DEFAULT_SETTINGS: Settings = {
  shotSidebarOpen: true,
  infoSidebarOpen: true,
  lastRoot: null,
  lastProjectPath: null,
  generatorPresetIds: null,
};

/** Fills missing or wrongly typed fields from the defaults (older or hand-edited files). */
export function normalizeSettings(raw: unknown): Settings {
  const value = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const bool = (key: keyof Settings) => typeof value[key] === "boolean" ? value[key] as boolean : DEFAULT_SETTINGS[key] as boolean;
  const text = (key: keyof Settings) => typeof value[key] === "string" && value[key] ? value[key] as string : null;
  const ids = Array.isArray(value.generatorPresetIds) ? value.generatorPresetIds.filter((id): id is string => typeof id === "string") : null;
  return {
    shotSidebarOpen: bool("shotSidebarOpen"),
    infoSidebarOpen: bool("infoSidebarOpen"),
    lastRoot: text("lastRoot"),
    lastProjectPath: text("lastProjectPath"),
    generatorPresetIds: ids,
  };
}

let current: Promise<Settings> | undefined;
let pending: Settings | undefined;
let timer: number | undefined;

export function loadSettings(): Promise<Settings> {
  current ??= invoke<unknown>("load_settings").then(normalizeSettings, () => ({ ...DEFAULT_SETTINGS }));
  return current;
}

/** Merges a change into the settings and writes them shortly after (coalescing bursts). */
export async function updateSettings(patch: Partial<Settings>): Promise<void> {
  const base = pending ?? await loadSettings();
  pending = { ...base, ...patch };
  current = Promise.resolve(pending);
  window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    const settings = pending;
    pending = undefined;
    if (settings) void invoke("save_settings", { settings }).catch(() => undefined);
  }, 300);
}
