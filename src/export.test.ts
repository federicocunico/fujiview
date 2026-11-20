import { describe, expect, it } from "vitest";
import { buildExportItems, countAtLeast, countByRating } from "./export";
import type { LibraryIndex, Project } from "./types";

const variant = (preset: string, key: string) => ({ preset, path: `D:\\${preset}\\${key}.jpg`, fileName: `${key}.jpg`, extension: "jpg", bytes: 1 });
const index: LibraryIndex = {
  root: "D:\\", presets: ["a", "b"], guidePreset: "a", warnings: [],
  shots: ["s1", "s2", "s3", "s4"].map((key) => ({ key, displayName: key, variants: [variant("a", key), variant("b", key)] })),
};
const decision = (key: string, rating: number, winnerPreset: string | null) => ({ key, rating, winnerPreset, reviewed: true });
const project = {
  decisions: {
    s1: decision("s1", 5, "a"),
    s2: decision("s2", 3, "b"),
    s3: decision("s3", 0, "a"),
    s4: decision("s4", 4, null),
  },
} as unknown as Project;

describe("buildExportItems", () => {
  it("exports every winner when no rating filter is set, skipping shots without winner", () => {
    expect(buildExportItems(index, project, 0).map((item) => item.outputName)).toEqual(["s1.jpg", "s2.jpg", "s3.jpg"]);
  });

  it("keeps only winners rated at least the minimum, with the winner's path", () => {
    expect(buildExportItems(index, project, 3)).toEqual([
      { sourcePath: "D:\\a\\s1.jpg", outputName: "s1.jpg", rating: 5 },
      { sourcePath: "D:\\b\\s2.jpg", outputName: "s2.jpg", rating: 3 },
    ]);
    expect(buildExportItems(index, project, 5)).toHaveLength(1);
  });

  it("ignores a winner whose variant is missing", () => {
    const broken = { decisions: { s1: decision("s1", 5, "zzz") } } as unknown as Project;
    expect(buildExportItems(index, broken, 0)).toEqual([]);
  });
});

describe("countByRating", () => {
  it("counts winners per exact rating and cumulatively", () => {
    const counts = countByRating(index, project);
    expect(counts).toEqual([1, 0, 0, 1, 0, 1]);
    expect(countAtLeast(counts, 0)).toBe(3);
    expect(countAtLeast(counts, 4)).toBe(1);
  });
});
