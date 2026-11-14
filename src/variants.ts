import type { CameraDiagnostics, CameraLogLine, CameraState, CameraStatus, DiagnosticCheck, VariantProgress } from "./types";

export function formatBytes(bytes: number): string {
  if (bytes >= 1073741824) return `${(bytes / 1073741824).toFixed(1)} GB`;
  return `${(bytes / 1048576).toFixed(bytes >= 104857600 ? 0 : 1)} MB`;
}

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "—";
  const total = Math.round(seconds);
  if (total < 60) return `${total} s`;
  const minutes = Math.round(total / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}

function parentOf(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return cut > 0 ? trimmed.slice(0, cut) : trimmed;
}

/** Label shown when no custom output folder is chosen. Files use their parent folder. */
export function defaultOutputLabel(sources: string[]): string {
  if (!sources.length) return "";
  const first = sources[0];
  const base = /\.raf$/i.test(first) ? parentOf(first) : first.replace(/[\\/]+$/, "");
  return `${base}/FujiView varianti`;
}

export function progressPercent(progress: Pick<VariantProgress, "total" | "completed" | "skipped" | "failed"> | undefined): number {
  if (!progress || progress.total <= 0) return 0;
  const done = progress.completed + progress.skipped + progress.failed;
  return Math.min(100, Math.max(0, (done / progress.total) * 100));
}

export function togglePreset(selected: string[], id: string): string[] {
  return selected.includes(id) ? selected.filter((item) => item !== id) : [...selected, id];
}

export function cameraMessage(status: CameraStatus | undefined): string {
  if (!status) return "Controllo della camera…";
  const simulated = status.simulated ? " (simulata)" : "";
  switch (status.state) {
    case "disconnected": return `Nessuna camera rilevata${simulated}`;
    case "notReady": return `${status.message ?? "Camera non pronta"}${simulated}. Imposta USB RAW CONV./BACKUP RESTORE nel menu della camera`;
    case "busy": return `Camera occupata: chiudi X RAW Studio${simulated}`;
    case "ready": return `${status.model ?? "Camera"} · firmware ${status.firmware ?? "?"} · pronta per la conversione${simulated}`;
    case "error": return `${status.message ?? "Errore camera"}${simulated}`;
    case "unresponsive": return `In attesa della camera… se non risponde, spegni e riaccendi la camera${simulated}`;
  }
}

export const FROZEN_ADVICE = "Spegni e riaccendi la camera, poi premi Ricontrolla.";

/** Short label for the toolbar chip and the panel header. */
export function stateLabel(state: CameraState | undefined): string {
  switch (state) {
    case "ready": return "pronta";
    case "disconnected": return "disconnessa";
    case "busy": return "occupata";
    case "error": return "errore";
    case "unresponsive": return "in attesa";
    case "notReady": return "non pronta";
    default: return "controllo…";
  }
}

/** Generation needs a ready camera and no failing checklist. */
export function canGenerate(status: CameraStatus | undefined, diagnostics: CameraDiagnostics | undefined): boolean {
  if (status?.state !== "ready") return false;
  if (!diagnostics || diagnostics.running) return true;
  return diagnostics.ready;
}

/** The check the user should act on first: errors before warnings. */
export function firstProblem(diagnostics: CameraDiagnostics | undefined): DiagnosticCheck | undefined {
  if (!diagnostics) return undefined;
  return diagnostics.checks.find((check) => check.status === "error") ?? diagnostics.checks.find((check) => check.status === "warning");
}

/** Headline for the checklist; frozen cameras get the power-cycle advice. */
export function diagnosticsHeadline(diagnostics: CameraDiagnostics | undefined): string {
  if (!diagnostics) return "Controllo della camera…";
  if (diagnostics.running) return "Controllo in corso…";
  if (diagnostics.unresponsive) return `La camera non risponde. ${FROZEN_ADVICE}`;
  if (diagnostics.ready) return "Tutto pronto per la conversione";
  const problem = firstProblem(diagnostics);
  return problem?.hint ?? problem?.detail ?? "Ci sono problemi da risolvere";
}

/** Delay before the next cheap status poll; null means do not poll. */
export function nextPollDelay(state: CameraState | undefined, hidden: boolean): number | null {
  if (hidden) return null;
  return state === "unresponsive" ? 15000 : 5000;
}

function two(value: number): string { return String(value).padStart(2, "0"); }

export function formatClock(ms: number): string {
  const date = new Date(ms);
  return `${two(date.getHours())}:${two(date.getMinutes())}:${two(date.getSeconds())}`;
}

export function logToText(lines: CameraLogLine[]): string {
  return lines.map((line) => `${formatClock(line.at)} [${line.level}] ${line.message}`).join("\n");
}

/** Append a line keeping the most recent `limit` (mirrors the backend ring). */
export function appendLog(lines: CameraLogLine[], line: CameraLogLine, limit = 200): CameraLogLine[] {
  const next = [...lines, line];
  return next.length > limit ? next.slice(next.length - limit) : next;
}
