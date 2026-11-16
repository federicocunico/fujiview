import type { LibraryIndex, Project, ShotDecision } from "./types";

/** One panel per preset, up to the number keys 1–9. */
export const MAX_PANELS = 9;

/** Grid for n panels: a single row up to 3, then the most square layout (4 → 2×2, 5–6 → 3×2, 7–9 → 3×3). */
export function panelGrid(count: number): { columns: number; rows: number } {
  const n = Math.max(1, count);
  const columns = n <= 3 ? n : Math.ceil(Math.sqrt(n));
  return { columns, rows: Math.ceil(n / columns) };
}

export function createProject(index: LibraryIndex): Project {
  const decisions: Record<string, ShotDecision> = {};
  for (const shot of index.shots) decisions[shot.key] = { key: shot.key, rating: 0, winnerPreset: null, reviewed: false };
  const now = new Date().toISOString();
  const orderedPresets = [index.guidePreset, ...index.presets.filter((preset) => preset !== index.guidePreset)];
  return {
    schemaVersion: 1,
    name: index.root.split(/[\\/]/).filter(Boolean).at(-1) || "Nuovo progetto",
    sourceRoot: index.root,
    presets: index.presets,
    guidePreset: index.guidePreset,
    panelPresets: orderedPresets.slice(0, MAX_PANELS),
    hiddenPresets: [],
    decisions,
    lastShotKey: index.shots[0]?.key ?? null,
    createdAt: now,
    updatedAt: now
  };
}

export function mergeProject(project: Project, index: LibraryIndex): Project {
  const decisions = { ...project.decisions };
  for (const shot of index.shots) decisions[shot.key] ??= { key: shot.key, rating: 0, winnerPreset: null, reviewed: false };
  // Presets the user hid stay hidden; presets new to the folder are shown.
  let hidden = (project.hiddenPresets ?? []).filter((preset) => index.presets.includes(preset));
  if (hidden.length >= index.presets.length) hidden = hidden.filter((preset) => preset !== index.guidePreset);
  const visible = (preset: string) => !hidden.includes(preset);
  const retainedPanels = project.panelPresets.filter((preset, position) =>
    index.presets.includes(preset) && project.panelPresets.indexOf(preset) === position && preset !== index.guidePreset && visible(preset)
  );
  const panelPresets = [
    ...(visible(index.guidePreset) ? [index.guidePreset] : []),
    ...retainedPanels,
    ...index.presets.filter((preset) => preset !== index.guidePreset && !retainedPanels.includes(preset) && visible(preset))
  ].slice(0, MAX_PANELS);
  return {
    ...project,
    sourceRoot: index.root,
    presets: index.presets,
    guidePreset: index.guidePreset,
    panelPresets,
    hiddenPresets: hidden,
    decisions,
    updatedAt: new Date().toISOString()
  };
}

/** Show or hide a preset's panel. The last visible panel cannot be hidden; at most MAX_PANELS are shown. */
export function togglePresetVisibility(project: Project, preset: string): Project {
  const shown = project.panelPresets.includes(preset);
  if (shown && project.panelPresets.length <= 1) return project;
  if (!shown && project.panelPresets.length >= MAX_PANELS) return project;
  const hidden = project.hiddenPresets ?? [];
  return shown
    ? { ...project, panelPresets: project.panelPresets.filter((item) => item !== preset), hiddenPresets: [...hidden.filter((item) => item !== preset), preset], updatedAt: new Date().toISOString() }
    : { ...project, panelPresets: [...project.panelPresets, preset], hiddenPresets: hidden.filter((item) => item !== preset), updatedAt: new Date().toISOString() };
}

/** Show every preset again (up to MAX_PANELS), keeping the current panel order first. */
export function showAllPresets(project: Project): Project {
  const panelPresets = [...project.panelPresets, ...project.presets.filter((preset) => !project.panelPresets.includes(preset))].slice(0, MAX_PANELS);
  return { ...project, panelPresets, hiddenPresets: [], updatedAt: new Date().toISOString() };
}
