import { describe, expect, it } from "vitest";
import { createProject } from "./project";

describe("createProject", () => {
  it("limits the initial split view to four presets", () => {
    const project = createProject({ root: "D:\\photos", presets: ["a", "b", "c", "d", "e"], guidePreset: "e", shots: [], warnings: [] });
    expect(project.panelPresets).toEqual(["e", "a", "b", "c"]);
  });
});
