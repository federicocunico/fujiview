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

// ---- Camera variant generation (backend contract: src-tauri/src/variants.rs) ----

export type CameraState = "disconnected" | "notReady" | "busy" | "ready" | "error" | "unresponsive";
export type CameraStatus = { state: CameraState; model: string | null; firmware: string | null; message: string | null; simulated: boolean };

/** id is "asshot" for "Come scattato" or "C1".."C7" for the camera custom banks. */
export type CameraPreset = {
  id: string;
  bank: number | null;
  name: string;
  folderName: string;
  filmSimulation: string | null;
  summary: string;
  unappliedProperties: string[];
};

export type VariantRequest = {
  /** A folder (top-level .RAF files are used) or explicit .RAF file paths. */
  sources: string[];
  /** Defaults to "<source folder>/FujiView varianti" when null. */
  outputDir: string | null;
  presetIds: string[];
  overwrite: boolean;
};

export type PlannedPreset = { id: string; folderName: string };
export type VariantPlan = {
  sourceRoot: string;
  outputDir: string;
  rafCount: number;
  presets: PlannedPreset[];
  totalConversions: number;
  pendingConversions: number;
  alreadyDone: number;
  estimatedBytes: number;
  freeBytes: number;
  enoughSpace: boolean;
  estimatedSeconds: number;
  warnings: string[];
};

export type VariantJobState = "running" | "cancelling" | "paused" | "finished" | "cancelled" | "failed";
export type VariantProgress = {
  state: VariantJobState;
  total: number;
  completed: number;
  skipped: number;
  failed: number;
  currentRaf: string | null;
  currentPreset: string | null;
  etaSeconds: number | null;
  message: string | null;
};
export type VariantFailure = { raf: string; preset: string | null; reason: string };
export type VariantReport = {
  state: VariantJobState;
  outputDir: string;
  converted: number;
  skipped: number;
  failures: VariantFailure[];
  /** Set when the job stopped early (disconnected camera, low disk, fatal camera error). */
  pausedReason: string | null;
};

export type CheckStatus = "ok" | "warning" | "error" | "pending" | "skipped" | "info";
export type DiagnosticCheck = { id: string; label: string; status: CheckStatus; detail: string | null; hint: string | null };
export type CameraDiagnostics = {
  checks: DiagnosticCheck[];
  model: string | null;
  firmware: string | null;
  simulated: boolean;
  /** Milliseconds since the Unix epoch. */
  checkedAt: number;
  /** No check failed or was skipped: generation may start. */
  ready: boolean;
  /** A step hung or looks like a frozen camera: power-cycle needed. */
  unresponsive: boolean;
  running: boolean;
};
export type CameraLogLine = { at: number; level: "info" | "warn" | "error"; message: string };
