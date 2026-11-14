import { describe, expect, it } from "vitest";
import type { CameraDiagnostics } from "./types";
import { appendLog, canGenerate, diagnosticsHeadline, firstProblem, formatClock, logToText, nextPollDelay, stateLabel, cameraMessage, defaultOutputLabel, formatBytes, formatDuration, progressPercent, togglePreset } from "./variants";

const base = { model: null, firmware: null, message: null, simulated: false };

describe("variants helpers", () => {
  it("formats bytes", () => {
    expect(formatBytes(5 * 1073741824)).toBe("5.0 GB");
    expect(formatBytes(1610612736)).toBe("1.5 GB");
    expect(formatBytes(10485760)).toBe("10.0 MB");
  });
  it("formats durations", () => {
    expect(formatDuration(null)).toBe("—");
    expect(formatDuration(42)).toBe("42 s");
    expect(formatDuration(600)).toBe("10 min");
    expect(formatDuration(3600)).toBe("1 h");
    expect(formatDuration(5400)).toBe("1 h 30 min");
  });
  it("builds default output label", () => {
    expect(defaultOutputLabel([])).toBe("");
    expect(defaultOutputLabel(["D:\\raf\\"])).toBe("D:\\raf/FujiView varianti");
    expect(defaultOutputLabel(["D:\\raf\\a.RAF"])).toBe("D:\\raf/FujiView varianti");
  });
  it("computes progress percentage including skipped", () => {
    expect(progressPercent(undefined)).toBe(0);
    expect(progressPercent({ total: 0, completed: 0, skipped: 0, failed: 0 })).toBe(0);
    expect(progressPercent({ total: 10, completed: 2, skipped: 2, failed: 1 })).toBe(50);
    expect(progressPercent({ total: 4, completed: 9, skipped: 0, failed: 0 })).toBe(100);
  });
  it("toggles presets", () => {
    expect(togglePreset(["a", "b"], "a")).toEqual(["b"]);
    expect(togglePreset(["a"], "b")).toEqual(["a", "b"]);
  });
  it("describes camera state", () => {
    expect(cameraMessage({ ...base, state: "disconnected" })).toBe("Nessuna camera rilevata");
    expect(cameraMessage({ ...base, state: "busy" })).toContain("chiudi X RAW Studio");
    expect(cameraMessage({ ...base, state: "notReady", message: "Modalità errata" })).toContain("USB RAW CONV./BACKUP RESTORE");
    expect(cameraMessage({ ...base, state: "ready", model: "X-T5", firmware: "3.00", simulated: true })).toBe("X-T5 · firmware 3.00 · pronta per la conversione (simulata)");
    expect(cameraMessage({ ...base, state: "error", message: "boom" })).toBe("boom");
  });
});

const diag = (over: Partial<CameraDiagnostics> = {}): CameraDiagnostics => ({
  checks: [
    { id: "device", label: "Camera", status: "ok", detail: null, hint: null },
    { id: "usbMode", label: "USB", status: "warning", detail: null, hint: "avviso" }
  ],
  model: null, firmware: null, simulated: false, checkedAt: 0, ready: true, unresponsive: false, running: false, ...over
});

describe("camera diagnostics helpers", () => {
  it("labels states", () => {
    expect(stateLabel("ready")).toBe("pronta");
    expect(stateLabel("unresponsive")).toBe("in attesa");
    expect(stateLabel(undefined)).toBe("controllo…");
    expect(cameraMessage({ ...base, state: "unresponsive" })).toContain("spegni e riaccendi");
  });
  it("gates generation on status and checklist", () => {
    const ready = { ...base, state: "ready" as const };
    expect(canGenerate(ready, undefined)).toBe(true);
    expect(canGenerate(ready, diag())).toBe(true);
    expect(canGenerate(ready, diag({ ready: false }))).toBe(false);
    expect(canGenerate(ready, diag({ ready: false, running: true }))).toBe(true);
    expect(canGenerate({ ...base, state: "busy" }, diag())).toBe(false);
    expect(canGenerate(undefined, diag())).toBe(false);
  });
  it("finds the first problem, errors first", () => {
    expect(firstProblem(diag())?.id).toBe("usbMode");
    const withError = diag({ checks: [...diag().checks, { id: "driver", label: "d", status: "error", detail: "x", hint: null }] });
    expect(firstProblem(withError)?.id).toBe("driver");
    expect(firstProblem(undefined)).toBeUndefined();
  });
  it("builds headlines", () => {
    expect(diagnosticsHeadline(undefined)).toContain("Controllo");
    expect(diagnosticsHeadline(diag())).toContain("pronto");
    expect(diagnosticsHeadline(diag({ unresponsive: true, ready: false }))).toContain("riaccendi");
    expect(diagnosticsHeadline(diag({ ready: false }))).toBe("avviso");
  });
  it("backs off polling", () => {
    expect(nextPollDelay("ready", false)).toBe(5000);
    expect(nextPollDelay("unresponsive", false)).toBe(15000);
    expect(nextPollDelay("ready", true)).toBeNull();
  });
  it("formats and bounds the log", () => {
    const at = new Date(2024, 0, 1, 9, 5, 7).getTime();
    expect(formatClock(at)).toBe("09:05:07");
    expect(logToText([{ at, level: "warn", message: "ciao" }])).toBe("09:05:07 [warn] ciao");
    let lines = [] as ReturnType<typeof appendLog>;
    for (let i = 0; i < 5; i++) lines = appendLog(lines, { at, level: "info", message: String(i) }, 3);
    expect(lines.map((l) => l.message)).toEqual(["2", "3", "4"]);
  });
});
