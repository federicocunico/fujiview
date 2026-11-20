import type { ExportItem, LibraryIndex, Project } from "./types";

/** Winning variants to export, keeping only shots rated at least `minRating` (0 = every winner). */
export function buildExportItems(index: LibraryIndex, project: Project, minRating: number): ExportItem[] {
  const items: ExportItem[] = [];
  for (const shot of index.shots) {
    const decision = project.decisions[shot.key];
    if (!decision?.winnerPreset || decision.rating < minRating) continue;
    const variant = shot.variants.find((item) => item.preset === decision.winnerPreset);
    if (variant) items.push({ sourcePath: variant.path, outputName: variant.fileName, rating: decision.rating });
  }
  return items;
}

/** Number of exportable winners per exact rating (index 0..5). */
export function countByRating(index: LibraryIndex, project: Project): number[] {
  const counts = [0, 0, 0, 0, 0, 0];
  for (const item of buildExportItems(index, project, 0)) counts[Math.min(5, Math.max(0, item.rating))] += 1;
  return counts;
}

/** Winners with rating >= minRating, from the per-rating counts. */
export function countAtLeast(counts: number[], minRating: number): number {
  return counts.slice(Math.max(0, minRating)).reduce((sum, value) => sum + value, 0);
}
