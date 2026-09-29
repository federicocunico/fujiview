import type { LibraryIndex, Project, ShotDecision } from "./types";

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
    panelPresets: orderedPresets.slice(0, 4),
    decisions,
    lastShotKey: index.shots[0]?.key ?? null,
    createdAt: now,
    updatedAt: now
  };
}

export function mergeProject(project: Project, index: LibraryIndex): Project {
  const decisions = { ...project.decisions };
  for (const shot of index.shots) decisions[shot.key] ??= { key: shot.key, rating: 0, winnerPreset: null, reviewed: false };
  const retainedPanels = project.panelPresets.filter((preset, position) =>
    index.presets.includes(preset) && project.panelPresets.indexOf(preset) === position && preset !== index.guidePreset
  );
  const panelPresets = [index.guidePreset, ...retainedPanels, ...index.presets.filter((preset) =>
    preset !== index.guidePreset && !retainedPanels.includes(preset)
  )].slice(0, 4);
  return {
    ...project,
    sourceRoot: index.root,
    presets: index.presets,
    guidePreset: index.guidePreset,
    panelPresets,
    decisions,
    updatedAt: new Date().toISOString()
  };
}
