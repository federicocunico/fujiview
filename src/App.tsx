import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, ChevronRight, Download, FolderOpen, Maximize2, PanelLeftClose, PanelLeftOpen, Save, Star, Trophy, X } from "lucide-react";
import { createProject, mergeProject } from "./project";
import type { ExportItem, ExportReport, IndexedShot, LibraryIndex, PreviewResult, Project, ShotDecision, Variant } from "./types";

type Viewport = { scale: number; x: number; y: number };
const FIT: Viewport = { scale: 1, x: 0, y: 0 };

function ShotThumbnail({ shot, guidePreset, decision, active, onSelect }: {
  shot: IndexedShot;
  guidePreset: string;
  decision?: ShotDecision;
  active: boolean;
  onSelect: () => void;
}) {
  const element = useRef<HTMLButtonElement>(null);
  const [visible, setVisible] = useState(false);
  const [src, setSrc] = useState<string>();
  const guide = shot.variants.find((variant) => variant.preset === guidePreset);

  useEffect(() => {
    const target = element.current;
    if (!target || visible) return;
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) {
        setVisible(true);
        observer.disconnect();
      }
    }, { rootMargin: "240px 0px" });
    observer.observe(target);
    return () => observer.disconnect();
  }, [visible]);

  useEffect(() => {
    if (!visible || !guide) return;
    let current = true;
    // Main comparison previews and adjacent-shot prefetch get a head start;
    // thumbnails must never make opening or navigation feel slower.
    const timer = window.setTimeout(() => {
      invoke<PreviewResult>("get_preview", { path: guide.path, maxEdge: 320, priority: "thumbnail", generation: null })
        .then((result) => { if (current) setSrc(convertFileSrc(result.cachePath)); })
        .catch(() => undefined);
    }, 1200);
    return () => { current = false; window.clearTimeout(timer); };
  }, [visible, guide?.path]);

  useEffect(() => {
    if (active) element.current?.scrollIntoView({ block: "nearest" });
  }, [active]);

  return <button ref={element} className={`shot-thumbnail ${active ? "active" : ""}`} onClick={onSelect}>
    <span className="thumbnail-frame">{src && <img src={src} alt="" loading="lazy" decoding="async" />}</span>
    <span className="thumbnail-info">
      <strong>{shot.displayName}</strong>
      <small>
        {decision?.rating ? `${decision.rating}★` : "—"}
        {decision?.winnerPreset && <em>{decision.winnerPreset}</em>}
      </small>
    </span>
  </button>;
}

function Preview({ variant, viewport, onViewport, panel, winner, onWinner }: {
  variant?: Variant; viewport: Viewport; onViewport: (v: Viewport) => void;
  panel: number; winner: boolean; onWinner: () => void;
}) {
  const [src, setSrc] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const drag = useRef<{ x: number; y: number; ox: number; oy: number } | undefined>(undefined);
  const dragged = useRef(false);
  const edge = viewport.scale > 3.2 ? 8192 : viewport.scale > 1.6 ? 4096 : 2048;

  useEffect(() => {
    let active = true;
    setError(undefined);
    if (!variant) { setSrc(undefined); return; }
    setLoading(false);
    const loadingTimer = window.setTimeout(() => { if (active) setLoading(true); }, 100);
    invoke<PreviewResult>("get_preview", { path: variant.path, maxEdge: edge, priority: "interactive", generation: null })
      .then((result) => { if (active) setSrc(convertFileSrc(result.cachePath)); })
      .catch((reason) => { if (active) setError(String(reason)); })
      .finally(() => { window.clearTimeout(loadingTimer); if (active) setLoading(false); });
    return () => { active = false; window.clearTimeout(loadingTimer); };
  }, [variant?.path, edge]);

  return <div
    className={`preview ${variant ? "selectable" : ""} ${winner ? "winner" : ""}`}
    title={variant ? `Clicca per scegliere ${variant.preset} come vincitore` : undefined}
    onClick={() => {
      if (variant && !dragged.current) onWinner();
      dragged.current = false;
    }}
    onWheel={(event) => {
      event.preventDefault();
      const scale = Math.min(8, Math.max(1, viewport.scale * (event.deltaY < 0 ? 1.15 : 0.87)));
      onViewport(scale === 1 ? FIT : { ...viewport, scale });
    }}
    onPointerDown={(event) => {
      dragged.current = false;
      if (viewport.scale <= 1) return;
      event.currentTarget.setPointerCapture(event.pointerId);
      drag.current = { x: event.clientX, y: event.clientY, ox: viewport.x, oy: viewport.y };
    }}
    onPointerMove={(event) => {
      if (!drag.current) return;
      const deltaX = event.clientX - drag.current.x;
      const deltaY = event.clientY - drag.current.y;
      if (Math.abs(deltaX) > 3 || Math.abs(deltaY) > 3) dragged.current = true;
      onViewport({ ...viewport, x: drag.current.ox + deltaX, y: drag.current.oy + deltaY });
    }}
    onPointerUp={() => { drag.current = undefined; }}
    onDoubleClick={() => onViewport(viewport.scale === 1 ? { scale: 2, x: 0, y: 0 } : FIT)}
  >
    <div className="panel-number">{panel}</div>
    <button className="winner-button" onClick={(event) => { event.stopPropagation(); onWinner(); }} title="Scegli questa variante">
      <Trophy size={15} fill={winner ? "currentColor" : "none"} />
    </button>
    {winner && <div className="winner-badge"><Trophy size={13} fill="currentColor" /> Vincitore</div>}
    {src && <img draggable={false} decoding="async" src={src} alt={variant?.fileName ?? ""} style={{ transform: `translate3d(${viewport.x}px, ${viewport.y}px, 0) scale(${viewport.scale})` }} />}
    {!variant && <div className="empty-panel"><X size={24}/><span>Variante non disponibile</span></div>}
    {loading && <div className="loading"><i /></div>}
    {error && <div className="image-error">{error}</div>}
    <div className="preset-label">{variant?.preset ?? "—"}<small>{variant ? `${(variant.bytes / 1048576).toFixed(1)} MB` : ""}</small></div>
  </div>;
}

export default function App() {
  const [index, setIndex] = useState<LibraryIndex>();
  const [project, setProject] = useState<Project>();
  const [projectPath, setProjectPath] = useState<string>();
  const [shotIndex, setShotIndex] = useState(0);
  const [viewport, setViewport] = useState<Viewport>(FIT);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [busy, setBusy] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const prefetchGeneration = useRef(0);

  const shot = index?.shots[shotIndex];
  const decision = shot && project?.decisions[shot.key];
  const panelPresets = project?.panelPresets ?? [];

  const updateDecision = useCallback((patch: Partial<{ rating: number; winnerPreset: string | null; reviewed: boolean }>) => {
    if (!shot || !project) return;
    setProject({ ...project, decisions: { ...project.decisions, [shot.key]: { ...project.decisions[shot.key], ...patch } }, lastShotKey: shot.key, updatedAt: new Date().toISOString() });
  }, [project, shot]);

  const go = useCallback((delta: number) => {
    if (!index?.shots.length) return;
    setShotIndex((current) => Math.min(index.shots.length - 1, Math.max(0, current + delta)));
    setViewport(FIT);
  }, [index]);

  const selectShot = useCallback((position: number) => {
    setShotIndex(position);
    setViewport(FIT);
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if ((event.target as HTMLElement)?.matches("input,select,textarea")) return;
      if (event.key === "Enter" && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
        event.preventDefault();
        go(1);
        return;
      }
      if (event.key === "ArrowLeft") go(-1);
      if (event.key === "ArrowRight") go(1);
      if (event.altKey && /^[0-5]$/.test(event.key)) { event.preventDefault(); updateDecision({ rating: Number(event.key), reviewed: true }); }
      else if (/^[1-4]$/.test(event.key)) {
        const preset = panelPresets[Number(event.key) - 1];
        if (preset && shot?.variants.some((variant) => variant.preset === preset)) updateDecision({ winnerPreset: preset, reviewed: true });
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [go, panelPresets, shot, updateDecision]);

  useEffect(() => {
    if (!project || !projectPath) return;
    const timer = window.setTimeout(() => invoke("save_project", { path: projectPath, project }).catch((error) => setNotice(`Salvataggio non riuscito: ${error}`)), 450);
    return () => window.clearTimeout(timer);
  }, [project, projectPath]);

  useEffect(() => {
    if (!index || !project?.lastShotKey) return;
    const found = index.shots.findIndex((candidate) => candidate.key === project.lastShotKey);
    if (found >= 0) setShotIndex(found);
  }, [index?.root]);

  useEffect(() => {
    if (!index || !project) return;
    const adjacent = [1, -1, 2, 3, -2, 4]
      .map((offset) => index.shots[shotIndex + offset])
      .filter(Boolean);
    const timer = window.setTimeout(() => {
      const generation = ++prefetchGeneration.current;
      for (const candidate of adjacent) {
        for (const preset of project.panelPresets) {
          const variant = candidate.variants.find((item) => item.preset === preset);
          if (variant) void invoke("get_preview", { path: variant.path, maxEdge: 2048, priority: "prefetch", generation }).catch(() => undefined);
        }
      }
    }, 60);
    return () => window.clearTimeout(timer);
  }, [index, project?.panelPresets, shotIndex]);

  const scan = async (root: string, existing?: Project, path?: string) => {
    setBusy("Indicizzazione in corso…"); setNotice(undefined);
    try {
      const result = await invoke<LibraryIndex>("scan_library", { root });
      const next = existing ? mergeProject(existing, result) : createProject(result);
      setIndex(result); setProject(next); setProjectPath(path); setShotIndex(0); setViewport(FIT);
      if (result.warnings.length) setNotice(`${result.warnings.length} file ignorati durante la scansione.`);
    } catch (error) { setNotice(String(error)); }
    finally { setBusy(undefined); }
  };

  const openFolder = async () => {
    const root = await open({ directory: true, multiple: false, title: "Scegli la cartella contenente i preset" });
    if (root) await scan(root);
  };

  const openProject = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Progetto FujiView", extensions: ["fujiview"] }] });
    if (!path) return;
    try {
      const loaded = await invoke<Project>("load_project", { path });
      await scan(loaded.sourceRoot, loaded, path);
    } catch (error) { setNotice(`Impossibile aprire il progetto: ${error}`); }
  };

  const saveProject = async () => {
    if (!project) return;
    const path = projectPath ?? await save({ defaultPath: `${project.name}.fujiview`, filters: [{ name: "Progetto FujiView", extensions: ["fujiview"] }] });
    if (!path) return;
    try { await invoke("save_project", { path, project }); setProjectPath(path); setNotice("Progetto salvato."); }
    catch (error) { setNotice(`Salvataggio non riuscito: ${error}`); }
  };

  const exportWinners = async () => {
    if (!index || !project) return;
    const destination = await open({ directory: true, multiple: false, title: "Cartella di esportazione" });
    if (!destination) return;
    const items: ExportItem[] = [];
    for (const candidate of index.shots) {
      const selected = project.decisions[candidate.key];
      if (!selected?.winnerPreset) continue;
      const variant = candidate.variants.find((item) => item.preset === selected.winnerPreset);
      if (variant) items.push({ sourcePath: variant.path, outputName: variant.fileName, rating: selected.rating });
    }
    if (!items.length) { setNotice("Nessuno scatto ha ancora un vincitore."); return; }
    const overwrite = window.confirm("Sostituire eventuali file con lo stesso nome nella destinazione?\nScegli Annulla per saltarli.");
    setBusy(`Esportazione di ${items.length} vincitori…`);
    try {
      const report = await invoke<ExportReport>("export_winners", { destination, items, overwrite });
      setNotice(`Export completato: ${report.exported} copiati, ${report.skipped} saltati${report.errors.length ? `, ${report.errors.length} errori` : ""}.`);
    } catch (error) { setNotice(`Export non riuscito: ${error}`); }
    finally { setBusy(undefined); }
  };

  const variants = useMemo(() => panelPresets.map((preset) => shot?.variants.find((variant) => variant.preset === preset)), [panelPresets, shot]);
  const decided = project ? Object.values(project.decisions).filter((item) => item.winnerPreset).length : 0;

  if (!index || !project) return <main className="welcome">
    <div className="brand"><span>F</span><div><h1>FujiView</h1><p>Confronta. Scegli. Esporta.</p></div></div>
    <section>
      <button className="primary large" onClick={openFolder}><FolderOpen /> Apri cartella preset</button>
      <button className="secondary large" onClick={openProject}>Apri progetto</button>
      <p className="hint">Ogni sottocartella viene trattata come un preset.<br/>JPEG e TIFF con lo stesso nome vengono affiancati.</p>
      {notice && <div className="notice">{notice}</div>}
    </section>
    {busy && <div className="busy-overlay"><i />{busy}</div>}
  </main>;

  return <main className="workspace">
    <header>
      <div className="title"><strong>{project.name}</strong><small>{shot?.displayName ?? "Nessuna foto"}</small></div>
      <div className="ratings" aria-label="Valutazione">
        {[1,2,3,4,5].map((rating) => <button key={rating} onClick={() => updateDecision({ rating: decision?.rating === rating ? 0 : rating, reviewed: true })} title={`${rating} stelle (Alt+${rating})`}><Star size={18} fill={(decision?.rating ?? 0) >= rating ? "currentColor" : "none"}/></button>)}
      </div>
      <div className="toolbar">
        <button onClick={() => setSidebarOpen((open) => !open)} title={sidebarOpen ? "Nascondi elenco scatti" : "Mostra elenco scatti"}>
          {sidebarOpen ? <PanelLeftClose /> : <PanelLeftOpen />}
        </button>
        <button onClick={openFolder} title="Apri cartella"><FolderOpen /></button>
        <button onClick={saveProject} title="Salva progetto"><Save /></button>
        <button onClick={() => setViewport(FIT)} title="Adatta"><Maximize2 /></button>
        <button className="export" onClick={exportWinners}><Download /> Esporta</button>
      </div>
    </header>
    <div className={`viewer-body ${sidebarOpen ? "" : "sidebar-hidden"}`}>
      {sidebarOpen && <aside className="shot-sidebar">
        <div className="sidebar-heading">
          <strong>Scatti</strong>
          <small>Guida: {index.guidePreset} · {index.shots.length}</small>
        </div>
        <nav className="thumbnail-list" aria-label="Elenco scatti">
          {index.shots.map((candidate, position) => <ShotThumbnail
            key={candidate.key}
            shot={candidate}
            guidePreset={index.guidePreset}
            decision={project.decisions[candidate.key]}
            active={position === shotIndex}
            onSelect={() => selectShot(position)}
          />)}
        </nav>
      </aside>}
      <div className={`comparison panels-${Math.max(1, panelPresets.length)}`}>
        {panelPresets.map((preset, panel) => <div className="panel-wrap" key={`${panel}-${preset}`}>
          <select value={preset} onChange={(event) => setProject({ ...project, panelPresets: project.panelPresets.map((value, i) => i === panel ? event.target.value : value) })}>
            {project.presets.map((option) => <option key={option} value={option}>{option}</option>)}
          </select>
          <Preview variant={variants[panel]} viewport={viewport} onViewport={setViewport} panel={panel + 1} winner={decision?.winnerPreset === preset} onWinner={() => updateDecision({ winnerPreset: preset, reviewed: true })}/>
        </div>)}
      </div>
    </div>
    <footer>
      <button aria-label="Scatto precedente" title="Scatto precedente (freccia sinistra)" disabled={shotIndex === 0} onClick={() => go(-1)}><ChevronLeft /></button>
      <div className="progress"><span style={{ width: `${index.shots.length ? ((shotIndex + 1) / index.shots.length) * 100 : 0}%` }}/></div>
      <strong>{index.shots.length ? shotIndex + 1 : 0} <em>/</em> {index.shots.length}</strong>
      <small>{decided} decisi</small>
      <button aria-label="Scatto successivo" title="Scatto successivo (Invio o freccia destra)" disabled={shotIndex >= index.shots.length - 1} onClick={() => go(1)}><ChevronRight /></button>
    </footer>
    {notice && <button className="toast" onClick={() => setNotice(undefined)}>{notice}<X size={15}/></button>}
    {busy && <div className="busy-overlay"><i />{busy}</div>}
  </main>;
}
