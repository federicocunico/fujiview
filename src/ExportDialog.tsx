import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Download } from "lucide-react";
import { buildExportItems, countAtLeast, countByRating } from "./export";
import { loadSettings, updateSettings } from "./settings";
import type { ExportReport, LibraryIndex, Project } from "./types";

const RATING_LABELS = ["Tutti i vincitori", "★ 1 o più", "★ 2 o più", "★ 3 o più", "★ 4 o più", "★ 5"];

export function ExportDialog({ index, project, onClose }: { index: LibraryIndex; project: Project; onClose: () => void }) {
  const [destination, setDestination] = useState<string | null>(null);
  const [minRating, setMinRating] = useState(0);
  const [overwrite, setOverwrite] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [report, setReport] = useState<ExportReport>();
  const [error, setError] = useState<string>();

  const counts = useMemo(() => countByRating(index, project), [index, project]);
  const items = useMemo(() => buildExportItems(index, project, minRating), [index, project, minRating]);

  useEffect(() => {
    let alive = true;
    void loadSettings().then((settings) => {
      if (!alive) return;
      setDestination(settings.exportDir);
      setMinRating(settings.exportMinRating);
    });
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => { if (event.key === "Escape" && !exporting) onClose(); };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [exporting, onClose]);

  const chooseDestination = async () => {
    try {
      const picked = await open({ directory: true, multiple: false, title: "Cartella di esportazione", defaultPath: destination ?? undefined });
      if (typeof picked !== "string") return;
      setDestination(picked);
      void updateSettings({ exportDir: picked });
    } catch (reason) { setError(`Impossibile aprire il selettore cartella: ${reason}`); }
  };

  const chooseRating = (value: number) => {
    setMinRating(value);
    void updateSettings({ exportMinRating: value });
  };

  const start = async () => {
    if (!destination || !items.length) return;
    setExporting(true);
    setError(undefined);
    try {
      setReport(await invoke<ExportReport>("export_winners", { destination, items, overwrite }));
    } catch (reason) { setError(`Export non riuscito: ${reason}`); }
    finally { setExporting(false); }
  };

  return <div className="modal-overlay" role="dialog" aria-modal="true" aria-label="Esporta vincitori">
    <div className="modal-panel">
      <h2><Download size={18} /> Esporta vincitori</h2>

      {!report && <>
        <section className="vg-section">
          <h3>Cartella di destinazione</h3>
          <div className="vg-row">
            <small className="vg-path">{destination ?? "Nessuna cartella scelta"}</small>
            <button className="secondary" disabled={exporting} onClick={() => void chooseDestination()}>Scegli…</button>
          </div>
        </section>

        <section className="vg-section">
          <h3>Filtro per valutazione</h3>
          <div className="vg-presets">
            {RATING_LABELS.map((label, value) => <label key={value} className="vg-preset">
              <input type="radio" name="export-rating" checked={minRating === value} disabled={exporting} onChange={() => chooseRating(value)} />
              <span>{label}<small>{countAtLeast(counts, value)} scatti</small></span>
            </label>)}
          </div>
          <label className="vg-check"><input type="checkbox" checked={overwrite} disabled={exporting} onChange={(event) => setOverwrite(event.target.checked)} /> Sovrascrivi i file con lo stesso nome nella destinazione</label>
        </section>

        <section className="vg-summary">
          <div><b>{items.length}</b> scatti da esportare su {countAtLeast(counts, 0)} vincitori</div>
          {!overwrite && <small>I file già presenti nella destinazione vengono saltati.</small>}
        </section>

        {error && <div className="notice">{error}</div>}
        <footer className="vg-actions">
          <button className="secondary" disabled={exporting} onClick={onClose}>Chiudi</button>
          <button className="primary" disabled={exporting || !destination || !items.length} onClick={() => void start()}>
            {exporting ? "Esportazione…" : `Esporta ${items.length} file`}
          </button>
        </footer>
      </>}

      {report && <>
        <section className="vg-section">
          <div><b>{report.exported}</b> copiati · <b>{report.skipped}</b> saltati · <b>{report.errors.length}</b> errori</div>
          <small className="vg-path">{destination}</small>
          {report.errors.length > 0 && <ul className="vg-failures">{report.errors.map((message, i) => <li key={i}>{message}</li>)}</ul>}
        </section>
        <footer className="vg-actions"><button className="primary" onClick={onClose}>Chiudi</button></footer>
      </>}
    </div>
  </div>;
}
