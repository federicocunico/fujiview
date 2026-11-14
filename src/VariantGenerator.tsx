import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Camera } from "lucide-react";
import { CameraPanel, useCameraMonitor } from "./CameraDiagnostics";
import type { CameraPreset, VariantPlan, VariantProgress, VariantReport, VariantRequest } from "./types";
import { canGenerate, defaultOutputLabel, formatBytes, formatDuration, progressPercent, togglePreset } from "./variants";

type View = "setup" | "running" | "finished";

export function VariantGenerator({ onClose, onOpenLibrary }: { onClose: () => void; onOpenLibrary: (outputDir: string) => void }) {
  const [presets, setPresets] = useState<CameraPreset[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [sources, setSources] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState<string | null>(null);
  const [overwrite, setOverwrite] = useState(false);
  const [plan, setPlan] = useState<VariantPlan>();
  const [planError, setPlanError] = useState<string>();
  const [view, setView] = useState<View>("setup");
  const [progress, setProgress] = useState<VariantProgress>();
  const [report, setReport] = useState<VariantReport>();
  const [error, setError] = useState<string>();
  const monitor = useCameraMonitor(view === "setup" || (view === "finished" && report?.state === "paused"));
  const { status, diagnostics, recheck } = monitor;

  const loadPresets = useCallback(async () => {
    try {
      const list = await invoke<CameraPreset[]>("camera_presets");
      setPresets(list);
      setSelected(list.map((preset) => preset.id));
    } catch (reason) { setError(String(reason)); }
  }, []);

  // Full checklist (also used by the "Ricontrolla" button); presets are read once the camera is ready.
  const check = useCallback(async () => {
    setError(undefined);
    const result = await recheck();
    if (result?.ready) await loadPresets();
  }, [recheck, loadPresets]);

  useEffect(() => {
    void check();
    const unlisten = [
      listen<VariantProgress>("variant-progress", (event) => setProgress(event.payload)),
      listen<VariantReport>("variant-finished", (event) => { setReport(event.payload); setView("finished"); })
    ];
    return () => { unlisten.forEach((promise) => promise.then((fn) => fn()).catch(() => undefined)); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The camera became ready after the modal opened (cable plugged in, mode fixed): fetch presets.
  useEffect(() => {
    if (status?.state === "ready" && !presets.length && view === "setup") void loadPresets();
  }, [status?.state, presets.length, view, loadPresets]);

  const request = useMemo<VariantRequest>(() => ({ sources, outputDir, presetIds: selected, overwrite }), [sources, outputDir, selected, overwrite]);

  useEffect(() => {
    if (view !== "setup") return;
    if (!sources.length || !selected.length) { setPlan(undefined); setPlanError(undefined); return; }
    let current = true;
    const timer = window.setTimeout(() => {
      invoke<VariantPlan>("plan_variants", { request })
        .then((result) => { if (current) { setPlan(result); setPlanError(undefined); } })
        .catch((reason) => { if (current) { setPlan(undefined); setPlanError(String(reason)); } });
    }, 300);
    return () => { current = false; window.clearTimeout(timer); };
  }, [request, view, sources.length, selected.length]);

  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Scegli la cartella dei file RAF" });
    if (typeof dir === "string") setSources([dir]);
  };
  const chooseFiles = async () => {
    const files = await open({ multiple: true, title: "Scegli i file RAF", filters: [{ name: "Fujifilm RAF", extensions: ["raf", "RAF"] }] });
    if (Array.isArray(files) && files.length) setSources(files);
    else if (typeof files === "string") setSources([files]);
  };
  const chooseOutput = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Cartella di output" });
    if (typeof dir === "string") setOutputDir(dir);
  };

  const running = view === "running" && progress?.state !== "paused";
  const ready = canGenerate(status, diagnostics);
  const pending = plan?.pendingConversions ?? 0;
  const canStart = ready && !!plan && plan.enoughSpace && pending > 0;

  const start = async () => {
    setError(undefined); setReport(undefined); setProgress(undefined);
    try { await invoke("start_variants", { request }); setView("running"); }
    catch (reason) { setError(String(reason)); }
  };
  const cancel = () => invoke("cancel_variants").catch((reason) => setError(String(reason)));
  const resume = async () => {
    setError(undefined);
    try { await invoke("resume_variants"); setReport(undefined); setView("running"); }
    catch (reason) { setError(String(reason)); }
  };

  const outputLabel = outputDir ?? defaultOutputLabel(sources);

  return <div className="modal-overlay" role="dialog" aria-modal="true" aria-label="Genera varianti dalla camera">
    <div className="modal-panel variant-generator">
      <h2><Camera size={20} /> Genera varianti dalla camera</h2>
      {error && <div className="notice">{error}</div>}

      {view === "setup" && <>
        <section className="vg-section">
          <h3>Camera</h3>
          <CameraPanel monitor={{ ...monitor, recheck: async () => { await check(); return undefined; } }} />
        </section>

        <section className="vg-section">
          <h3>Preset</h3>
          <div className="vg-presets">
            {presets.map((preset) => <label key={preset.id} className="vg-preset">
              <input type="checkbox" checked={selected.includes(preset.id)} onChange={() => setSelected(togglePreset(selected, preset.id))} />
              <span><strong>{preset.folderName}</strong><small>{preset.summary}</small>
                {preset.unappliedProperties.length > 0 && <em className="vg-warn">Non applicato: {preset.unappliedProperties.join(", ")}</em>}
              </span>
            </label>)}
          </div>
        </section>

        <section className="vg-section">
          <h3>Sorgente</h3>
          <div className="vg-row">
            <button className="secondary" onClick={chooseFolder}>Scegli cartella RAF</button>
            <button className="secondary" onClick={chooseFiles}>Scegli file RAF</button>
          </div>
          {sources.length > 0 && <small className="vg-path">{sources.length === 1 ? sources[0] : `${sources.length} file RAF selezionati`}</small>}
          <h3>Output</h3>
          <div className="vg-row">
            <small className="vg-path">{outputLabel || "—"}</small>
            <button className="secondary" onClick={chooseOutput}>Cambia…</button>
            {outputDir && <button className="secondary" onClick={() => setOutputDir(null)}>Predefinita</button>}
          </div>
          <label className="vg-check"><input type="checkbox" checked={overwrite} onChange={(event) => setOverwrite(event.target.checked)} /> Sovrascrivi varianti esistenti</label>
        </section>

        {planError && <div className="notice">{planError}</div>}
        {plan && <section className="vg-summary">
          <div><b>{plan.rafCount}</b> RAF · <b>{plan.totalConversions}</b> conversioni · <b>{plan.alreadyDone}</b> già fatte (saltate) · <b>{plan.pendingConversions}</b> da fare</div>
          <div>Spazio stimato {formatBytes(plan.estimatedBytes)} su {formatBytes(plan.freeBytes)} liberi · Tempo stimato {formatDuration(plan.estimatedSeconds)}</div>
          {plan.warnings.map((warning) => <div key={warning} className="vg-warn">{warning}</div>)}
          {!plan.enoughSpace && <div className="notice">Spazio su disco insufficiente per le varianti richieste.</div>}
        </section>}

        <footer className="vg-actions">
          <button className="secondary" onClick={onClose}>Chiudi</button>
          {!ready && <small className="vg-warn vg-why">Genera è disattivato finché la camera non è pronta.</small>}
          <button className="primary" disabled={!canStart} onClick={start}>Genera {pending} varianti</button>
        </footer>
      </>}

      {view === "running" && <>
        <section className="vg-section">
          <div>{progress?.state === "paused" ? "In pausa" : progress?.state === "cancelling" ? "Annullamento…" : "Conversione in corso…"}</div>
          <small className="vg-path">{progress?.currentRaf ?? "—"}{progress?.currentPreset ? ` · ${progress.currentPreset}` : ""}</small>
          <div className="vg-bar"><span style={{ width: `${progressPercent(progress)}%` }} /></div>
          <div>{progress ? progress.completed + progress.skipped : 0} / {progress?.total ?? 0} · Errori: {progress?.failed ?? 0} · Tempo residuo {formatDuration(progress?.etaSeconds)}</div>
          {progress?.message && <div className="vg-warn">{progress.message}</div>}
          {progress?.state === "paused"
            ? <div className="vg-paused">La generazione è in pausa. Controlla che la camera sia accesa e collegata (se non risponde, spegnila e riaccendila), poi riprendi.</div>
            : <small className="vg-path">Non spegnere né scollegare la camera: il display mostra la conversione in corso. I file già convertiti vengono conservati.</small>}
        </section>
        {progress?.state === "paused" && <section className="vg-section"><CameraPanel compact monitor={{ ...monitor, recheck: async () => { await check(); return undefined; } }} /></section>}
        <footer className="vg-actions">
          <button className="secondary" disabled={progress?.state === "cancelling"} onClick={cancel}>{progress?.state === "cancelling" ? "Annullamento…" : "Annulla"}</button>
          <button className="secondary" disabled={running} onClick={onClose}>Chiudi</button>
        </footer>
      </>}

      {view === "finished" && report && <>
        <section className="vg-section">
          <div><b>{report.converted}</b> convertite · <b>{report.skipped}</b> saltate · <b>{report.failures.length}</b> fallite</div>
          {report.pausedReason && <div className="vg-paused">{report.pausedReason}</div>}
          {report.state === "paused" && <>
            <div className="vg-warn">Per riprendere: riaccendi la camera o ricollega il cavo USB, attendi che risulti «pronta», poi premi Riprendi. Le conversioni già fatte non vengono rifatte.</div>
            <CameraPanel compact monitor={{ ...monitor, recheck: async () => { await check(); return undefined; } }} />
          </>}
          {report.failures.length > 0 && <ul className="vg-failures">
            {report.failures.map((failure, i) => <li key={i}><strong>{failure.raf}</strong>{failure.preset ? ` · ${failure.preset}` : ""}<small>{failure.reason}</small></li>)}
          </ul>}
        </section>
        <footer className="vg-actions">
          {report.state === "paused" && <button className="primary" disabled={status?.state !== "ready"} onClick={resume} title={status?.state === "ready" ? undefined : "Attendi che la camera sia pronta"}>Riprendi</button>}
          <button className="primary" onClick={() => onOpenLibrary(report.outputDir)}>Apri per il confronto</button>
          <button className="secondary" onClick={onClose}>Chiudi</button>
        </footer>
      </>}
    </div>
  </div>;
}
