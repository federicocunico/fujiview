export type Variant = {
  preset: string;
  path: string;
  fileName: string;
  extension: string;
  bytes: number;
};

export type IndexedShot = { key: string; displayName: string; variants: Variant[] };
export type LibraryIndex = { root: string; presets: string[]; guidePreset: string; shots: IndexedShot[]; warnings: string[] };
export type ShotDecision = { key: string; rating: number; winnerPreset: string | null; reviewed: boolean };

export type Project = {
  schemaVersion: 1;
  name: string;
  sourceRoot: string;
  presets: string[];
  guidePreset: string;
  panelPresets: string[];
  decisions: Record<string, ShotDecision>;
  lastShotKey: string | null;
  createdAt: string;
  updatedAt: string;
};

export type PreviewResult = { cachePath: string; width: number; height: number; cached: boolean };
export type ExportItem = { sourcePath: string; outputName: string; rating: number };
export type ExportReport = { exported: number; skipped: number; errors: string[] };
