import { describe, expect, it } from "vitest";
import { createProject, MAX_PANELS, mergeProject, panelGrid } from "./project";

const index = (presets: string[], guidePreset: string) => ({ root: "D:\\photos", presets, guidePreset, shots: [], warnings: [] });

describe("createProject", () => {
  it("shows one panel per preset, guide first", () => {
    const project = createProject(index(["a", "b", "c", "d", "e", "f", "g"], "e"));
    expect(project.panelPresets).toEqual(["e", "a", "b", "c", "d", "f", "g"]);
  });

  it("caps the panels at the number keys", () => {
    const presets = Array.from({ length: 12 }, (_, i) => `p${i}`);
    expect(createProject(index(presets, "p0")).panelPresets).toHaveLength(MAX_PANELS);
  });
});

describe("mergeProject", () => {
  it("adds presets that appeared after the project was saved (old 4-panel projects)", () => {
    const saved = createProject(index(["a", "b", "c", "d"], "a"));
    const merged = mergeProject(saved, index(["a", "b", "c", "d", "e", "f"], "a"));
    expect(merged.panelPresets).toEqual(["a", "b", "c", "d", "e", "f"]);
  });
});

describe("panelGrid", () => {
  it("adapts the layout to the number of panels", () => {
    expect([1, 2, 3, 4, 5, 6, 7, 9].map((n) => panelGrid(n))).toEqual([
      { columns: 1, rows: 1 }, { columns: 2, rows: 1 }, { columns: 3, rows: 1 }, { columns: 2, rows: 2 },
      { columns: 3, rows: 2 }, { columns: 3, rows: 2 }, { columns: 3, rows: 3 }, { columns: 3, rows: 3 }
    ]);
    expect(panelGrid(0)).toEqual({ columns: 1, rows: 1 });
  });
});
