import { describe, expect, it } from "vitest";
import { createProject, MAX_PANELS, mergeProject, panelGrid, showAllPresets, togglePresetVisibility } from "./project";

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

  it("restores the saved panel order, winners and last shot", () => {
    const saved = { ...createProject(index(["a", "b", "c"], "a")), panelPresets: ["c", "a", "b"], lastShotKey: "dscf2",
      decisions: { dscf2: { key: "dscf2", rating: 4, winnerPreset: "c", reviewed: true } } };
    const merged = mergeProject(saved, index(["a", "b", "c"], "a"));
    expect(merged.panelPresets).toEqual(["c", "a", "b"]);
    expect(merged.lastShotKey).toBe("dscf2");
    expect(merged.decisions.dscf2).toEqual({ key: "dscf2", rating: 4, winnerPreset: "c", reviewed: true });
  });
});

describe("preset visibility", () => {
  const base = () => createProject(index(["a", "b", "c", "d"], "a"));

  it("hides and shows a preset, appending it back at the end", () => {
    const hidden = togglePresetVisibility(base(), "b");
    expect(hidden.panelPresets).toEqual(["a", "c", "d"]);
    expect(hidden.hiddenPresets).toEqual(["b"]);
    const shown = togglePresetVisibility(hidden, "b");
    expect(shown.panelPresets).toEqual(["a", "c", "d", "b"]);
    expect(shown.hiddenPresets).toEqual([]);
  });

  it("keeps at least one preset visible", () => {
    let project = base();
    for (const preset of ["a", "b", "c", "d"]) project = togglePresetVisibility(project, preset);
    expect(project.panelPresets).toEqual(["d"]);
  });

  it("keeps hidden presets hidden when the folder is reopened, new ones appear", () => {
    const saved = togglePresetVisibility(togglePresetVisibility(base(), "a"), "c");
    const merged = mergeProject(saved, index(["a", "b", "c", "d", "e"], "a"));
    expect(merged.panelPresets).toEqual(["b", "d", "e"]);
    expect(merged.hiddenPresets).toEqual(["a", "c"]);
  });

  it("opens projects saved before hiding existed", () => {
    const { hiddenPresets: _, ...old } = base();
    expect(mergeProject(old, index(["a", "b", "c", "d"], "a")).panelPresets).toEqual(["a", "b", "c", "d"]);
  });

  it("shows all again", () => {
    const project = showAllPresets(togglePresetVisibility(base(), "b"));
    expect(project.panelPresets).toEqual(["a", "c", "d", "b"]);
    expect(project.hiddenPresets).toEqual([]);
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
