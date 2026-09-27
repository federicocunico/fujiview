import type { LibraryIndex, Project, ShotDecision } from "./types";

export function createProject(index: LibraryIndex): Project {
  const decisions: Record<string, ShotDecision> = {};
  for (const shot of index.shots) decisions[shot.key] = { key: shot.key, rating: 0, winnerPreset: null, reviewed: false };
  const now = new Date().toISOString();
  return {
    schemaVersion: 1,
    name: index.root.split(/[\\/]/).filter(Boolean).at(-1) || "Nuovo progetto",
    sourceRoot: index.root,
    presets: index.presets,
    panelPresets: index.presets.slice(0, 4),
    decisions,
    lastShotKey: index.shots[0]?.key ?? null,
    createdAt: now,
    updatedAt: now
  };
}

export function mergeProject(project: Project, index: LibraryIndex): Project {
  const decisions = { ...project.decisions };
  for (const shot of index.shots) decisions[shot.key] ??= { key: shot.key, rating: 0, winnerPreset: null, reviewed: false };
  return { ...project, sourceRoot: index.root, presets: index.presets, decisions, updatedAt: new Date().toISOString() };
}

