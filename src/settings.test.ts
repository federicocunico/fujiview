import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, normalizeSettings } from "./settings";

describe("normalizeSettings", () => {
  it("opens both sidebars by default", () => {
    expect(normalizeSettings(undefined)).toEqual(DEFAULT_SETTINGS);
    expect(DEFAULT_SETTINGS.shotSidebarOpen).toBe(true);
    expect(DEFAULT_SETTINGS.infoSidebarOpen).toBe(true);
  });

  it("keeps saved values and drops malformed ones", () => {
    expect(normalizeSettings({ shotSidebarOpen: false, infoSidebarOpen: "no", lastRoot: "D:\Foto", lastProjectPath: "", generatorPresetIds: ["C1", 3, "asshot"] })).toEqual({
      shotSidebarOpen: false,
      infoSidebarOpen: true,
      lastRoot: "D:\Foto",
      lastProjectPath: null,
      generatorPresetIds: ["C1", "asshot"],
    });
  });
});
