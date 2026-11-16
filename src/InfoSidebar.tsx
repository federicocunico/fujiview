import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Copy } from "lucide-react";
import { MAX_PANELS, showAllPresets, togglePresetVisibility } from "./project";
import type { ImageMetadata, IndexedShot, Project } from "./types";

/** Metadata per file path; files do not change while the app is open. */
const cache = new Map<string, Promise<ImageMetadata>>();
function loadMetadata(path: string): Promise<ImageMetadata> {
  let entry = cache.get(path);
  if (!entry) {
    entry = invoke<ImageMetadata>("image_metadata", { path });
    entry.catch(() => cache.delete(path));
    cache.set(path, entry);
  }
  return entry;
}

/** Metadata of every variant of the shot, keyed by preset. */
function useShotMetadata(shot: IndexedShot | undefined) {
  const [byPreset, setByPreset] = useState<Record<string, ImageMetadata>>({});
  const generation = useRef(0);
  useEffect(() => {
    const current = ++generation.current;
    setByPreset({});
    if (!shot) return;
    for (const variant of shot.variants) {
      loadMetadata(variant.path)
        .then((metadata) => { if (generation.current === current) setByPreset((prev) => ({ ...prev, [variant.preset]: metadata })); })
        .catch(() => undefined);
    }
  }, [shot]);
  return byPreset;
}

type Props = { project: Project; shot: IndexedShot | undefined; onProject: (project: Project) => void };

export function InfoSidebar({ project, shot, onProject }: Props) {
  const metadata = useShotMetadata(shot);
  const panelPresets = project.panelPresets;
  // Shooting data is the same in every variant: read it from the guide, else any variant.
  const primaryPreset = shot?.variants.some((variant) => variant.preset === project.guidePreset)
    ? project.guidePreset
    : shot?.variants[0]?.preset;
  const primary = primaryPreset ? metadata[primaryPreset] : undefined;
  const [copied, setCopied] = useState(false);

  const copyCoordinates = async (text: string) => {
    try { await navigator.clipboard.writeText(text); setCopied(true); window.setTimeout(() => setCopied(false), 1500); } catch { /* clipboard unavailable */ }
  };

  return <aside className="info-sidebar" aria-label="Preset e informazioni scatto">
    <details className="info-section" open>
      <summary><strong>Preset</strong><small>{panelPresets.length} di {project.presets.length} visibili</small></summary>
      <div className="preset-list">
        {project.presets.map((preset) => {
          const position = panelPresets.indexOf(preset);
          const shown = position >= 0;
          const locked = shown ? panelPresets.length <= 1 : panelPresets.length >= MAX_PANELS;
          const missing = !!shot && !shot.variants.some((variant) => variant.preset === preset);
          const recipe = metadata[preset]?.recipe;
          const tooltip = locked
            ? (shown ? "Deve restare almeno un preset visibile" : `Al massimo ${MAX_PANELS} preset visibili`)
            : recipe?.rows.map((row) => `${row.label}: ${row.value}`).join("\n");
          return <label key={preset} className={`preset-toggle ${shown ? "shown" : ""}`} title={tooltip}>
            <input type="checkbox" checked={shown} disabled={locked} onChange={() => onProject(togglePresetVisibility(project, preset))} />
            <span className="preset-name">
              <span>{preset}{preset === project.guidePreset && <small> · guida</small>}</span>
              {missing ? <small className="preset-missing">manca in questo scatto</small> : recipe?.summary && <small>{recipe.summary}</small>}
            </span>
            {shown && <kbd>{position + 1}</kbd>}
          </label>;
        })}
      </div>
      {panelPresets.length < Math.min(project.presets.length, MAX_PANELS) && <button className="secondary preset-all" onClick={() => onProject(showAllPresets(project))}>Mostra tutti</button>}
    </details>

    {!shot && <p className="info-empty">Nessuno scatto selezionato.</p>}
    {shot && !primary && <p className="info-empty">Lettura dei metadati…</p>}
    {primary?.sections.map((section) => <details key={section.title} className="info-section" open>
      <summary><strong>{section.title}</strong></summary>
      <dl className="info-rows">
        {section.rows.map((row) => <div key={row.label}><dt>{row.label}</dt><dd title={row.value}>{row.value}</dd></div>)}
      </dl>
      {section.title === "Luogo" && primary.gps && <button className="secondary info-copy" onClick={() => copyCoordinates(`${primary.gps!.latitude.toFixed(6)}, ${primary.gps!.longitude.toFixed(6)}`)}>
        <Copy size={13} /> {copied ? "Copiate" : "Copia coordinate"}
      </button>}
    </details>)}
  </aside>;
}
