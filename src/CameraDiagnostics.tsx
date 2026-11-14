import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, Circle, ClipboardCopy, Info, Loader2, MinusCircle, RefreshCw, XCircle } from "lucide-react";
import type { CameraDiagnostics, CameraLogLine, CameraStatus, CheckStatus } from "./types";
import { appendLog, diagnosticsHeadline, formatClock, logToText, nextPollDelay, stateLabel } from "./variants";

function unlistenAll(promises: Promise<() => void>[]) {
  promises.forEach((promise) => promise.then((fn) => fn()).catch(() => undefined));
}

/** Passive status for the toolbar chip: cached value plus events, never touches the camera. */
export function useCameraChip(): CameraStatus | undefined {
  const [status, setStatus] = useState<CameraStatus>();
  useEffect(() => {
    invoke<CameraStatus>("camera_status_cached").then(setStatus).catch(() => undefined);
    const un = [listen<CameraStatus>("camera-status", (event) => setStatus(event.payload))];
    return () => unlistenAll(un);
  }, []);
  return status;
}

/** Small status dot meant to sit inside the toolbar Camera button. */
export function CameraChip() {
  const status = useCameraChip();
  const state = status?.state ?? "unknown";
  const label = status ? `Camera ${stateLabel(status.state)}` : "Camera: stato sconosciuto";
  return <span className={`camera-chip vg-badge ${state}`} title={label} aria-label={label} />;
}

export type CameraMonitor = {
  status?: CameraStatus;
  diagnostics?: CameraDiagnostics;
  checking: boolean;
  log: CameraLogLine[];
  recheck: () => Promise<CameraDiagnostics | undefined>;
  pollNow: () => Promise<void>;
};

/**
 * Live camera state for the generator modal. The full checklist runs on demand
 * (and once when the modal opens); while `poll` is true only the cheap status
 * call runs every few seconds, and never while a checklist is in flight.
 */
export function useCameraMonitor(poll: boolean): CameraMonitor {
  const [status, setStatus] = useState<CameraStatus>();
  const [diagnostics, setDiagnostics] = useState<CameraDiagnostics>();
  const [checking, setChecking] = useState(false);
  const [log, setLog] = useState<CameraLogLine[]>([]);
  const checkingRef = useRef(false);
  const stateRef = useRef<CameraStatus["state"] | undefined>(undefined);
  stateRef.current = status?.state;

  useEffect(() => {
    invoke<CameraLogLine[]>("camera_log").then(setLog).catch(() => undefined);
    invoke<CameraStatus>("camera_status_cached").then((cached) => setStatus((current) => current ?? cached)).catch(() => undefined);
    const un = [
      listen<CameraStatus>("camera-status", (event) => setStatus(event.payload)),
      listen<CameraDiagnostics>("camera-diagnostics", (event) => { if (checkingRef.current) setDiagnostics(event.payload); }),
      listen<CameraLogLine>("camera-log", (event) => setLog((lines) => appendLog(lines, event.payload)))
    ];
    return () => unlistenAll(un);
  }, []);

  const recheck = useCallback(async () => {
    if (checkingRef.current) return undefined;
    checkingRef.current = true;
    setChecking(true);
    try {
      const result = await invoke<CameraDiagnostics>("camera_diagnostics");
      setDiagnostics(result);
      return result;
    } catch (reason) {
      setLog((lines) => appendLog(lines, { at: Date.now(), level: "error", message: String(reason) }));
      return undefined;
    } finally {
      checkingRef.current = false;
      setChecking(false);
      try { setStatus(await invoke<CameraStatus>("camera_status_cached")); } catch { /* keep last status */ }
    }
  }, []);

  const pollNow = useCallback(async () => {
    if (checkingRef.current) return;
    try { setStatus(await invoke<CameraStatus>("camera_status")); } catch { /* ignore */ }
  }, []);

  // Cheap polling with backoff while the camera is unresponsive; paused when the window is hidden.
  useEffect(() => {
    if (!poll) return;
    let timer: number | undefined;
    let cancelled = false;
    const tick = async () => {
      const delay = nextPollDelay(stateRef.current, document.hidden);
      if (delay !== null) await pollNow();
      if (!cancelled) timer = window.setTimeout(tick, nextPollDelay(stateRef.current, document.hidden) ?? 5000);
    };
    timer = window.setTimeout(tick, 5000);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [poll, pollNow]);

  // The camera came back (or got fixed): refresh the checklist once.
  const previous = useRef<CameraStatus["state"] | undefined>(undefined);
  useEffect(() => {
    const state = status?.state;
    if (state === "ready" && previous.current && previous.current !== "ready" && diagnostics && !diagnostics.ready) void recheck();
    previous.current = state;
  }, [status?.state, diagnostics, recheck]);

  return { status, diagnostics, checking, log, recheck, pollNow };
}

function CheckIcon({ status }: { status: CheckStatus }) {
  switch (status) {
    case "ok": return <CheckCircle2 size={16} className="ck ok" />;
    case "warning": return <AlertTriangle size={16} className="ck warning" />;
    case "error": return <XCircle size={16} className="ck error" />;
    case "pending": return <Loader2 size={16} className="ck pending spin" />;
    case "skipped": return <MinusCircle size={16} className="ck skipped" />;
    case "info": return <Info size={16} className="ck info" />;
    default: return <Circle size={16} className="ck skipped" />;
  }
}

export function SetupGuide({ open }: { open?: boolean }) {
  return <details className="cd-guide" open={open}>
    <summary>Prima configurazione della X-T5</summary>
    <ol>
      <li>Collega la camera al PC con un cavo USB dati e accendila.</li>
      <li>Nel menu della camera imposta la modalità USB su <b>USB RAW CONV./BACKUP RESTORE</b>. Percorso indicativo: MENU &gt; Impostazioni connessione &gt; Impostazione USB (o «Modalità connessione»); il nome esatto cambia con il firmware.</li>
      <li>Chiudi FUJIFILM X RAW Studio e le altre app Fujifilm: non possono usare la camera insieme a FujiView.</li>
      <li>Disattiva lo spegnimento automatico e tieni la batteria carica (o alimenta la camera).</li>
      <li>Se la camera smette di rispondere, spegnila e riaccendila, poi premi Ricontrolla.</li>
    </ol>
  </details>;
}

export function CameraLogView({ log }: { log: CameraLogLine[] }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try { await navigator.clipboard.writeText(logToText(log)); setCopied(true); window.setTimeout(() => setCopied(false), 1500); } catch { setCopied(false); }
  };
  return <details className="cd-log">
    <summary>Log camera ({log.length})</summary>
    <div className="cd-log-actions"><button className="secondary" onClick={copy}><ClipboardCopy size={13} /> {copied ? "Copiato" : "Copia log"}</button></div>
    <pre>{log.length ? log.map((line, i) => <span key={i} className={`lg ${line.level}`}>{formatClock(line.at)} {line.message}{"\n"}</span>) : "Nessun evento."}</pre>
  </details>;
}

export function CameraPanel({ monitor, compact = false }: { monitor: CameraMonitor; compact?: boolean }) {
  const { status, diagnostics, checking, log, recheck } = monitor;
  const [showChecks, setShowChecks] = useState(!compact);
  const state = status?.state;
  const waiting = state === "unresponsive";
  const hasProblem = !!diagnostics && !diagnostics.ready && !diagnostics.running;
  const headline = checking ? "Controllo in corso… può richiedere qualche secondo" : waiting ? "In attesa della camera… se non risponde, spegni e riaccendi la camera." : state === "disconnected" && !diagnostics ? "Nessuna camera rilevata" : diagnosticsHeadline(diagnostics);
  const headTone = checking || waiting ? "pending" : diagnostics?.unresponsive ? "error" : hasProblem ? "warning" : state === "ready" ? "ok" : "";

  return <div className={`cd-panel ${compact ? "compact" : ""}`}>
    <div className="vg-camera">
      <span className={`vg-badge ${state ?? "unknown"}`} />
      <span><b>{status?.model ?? "Camera"}</b> · {stateLabel(state)}{status?.firmware ? ` · fw ${status.firmware}` : ""}{status?.simulated ? " · simulata" : ""}</span>
      <button className="secondary" disabled={checking} onClick={() => void recheck()}>
        {checking ? <Loader2 size={14} className="spin" /> : <RefreshCw size={14} />} Ricontrolla
      </button>
    </div>
    <div className={`cd-headline ${headTone}`} role="status">{headline}</div>
    {diagnostics && <>
      <button className="cd-toggle" onClick={() => setShowChecks((v) => !v)}>
        {showChecks ? <ChevronDown size={14} /> : <ChevronRight size={14} />} Controlli{diagnostics.checkedAt ? ` · ${formatClock(diagnostics.checkedAt)}` : ""}
      </button>
      {showChecks && <ul className="cd-checks">
        {diagnostics.checks.map((check) => <li key={check.id} className={check.status}>
          <CheckIcon status={check.status} />
          <span>
            <strong>{check.label}</strong>
            {check.detail && <small>{check.detail}</small>}
            {check.hint && check.status !== "ok" && check.status !== "pending" && check.status !== "skipped" && <em>{check.hint}</em>}
          </span>
        </li>)}
      </ul>}
    </>}
    <SetupGuide open={state === "disconnected" || state === "notReady" || hasProblem} />
    <CameraLogView log={log} />
  </div>;
}
