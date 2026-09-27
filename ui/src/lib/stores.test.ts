import { afterEach, describe, expect, it, vi } from "vitest";
// @ts-ignore -- vitest runs on node; ui tsconfig omits node types
import { readFileSync } from "node:fs";
import {
  applyThemePreference,
  normalizeThemePref,
  resolveTheme,
} from "./stores.svelte";

describe("theme resolution", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("resolveTheme_maps_system", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  it("resolveTheme_passthrough", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("system_resolution_roundtrip", () => {
    const listeners: ((event: { matches: boolean }) => void)[] = [];
    vi.stubGlobal(
      "matchMedia",
      (query: string) => ({
        matches: true,
        media: query,
        addEventListener: (_: string, cb: (event: { matches: boolean }) => void) => {
          listeners.push(cb);
        },
        removeEventListener: () => {},
      }),
    );

    applyThemePreference("system");
    expect(document.documentElement.dataset.theme).toBe("dark");

    // OS flips to light while the app preference stays "system".
    for (const cb of listeners) cb({ matches: false });
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("loadPreferences_unknown_theme_safe", () => {
    expect(normalizeThemePref("chartreuse")).toBe("system");
    expect(normalizeThemePref(undefined)).toBe("system");
    expect(normalizeThemePref("ember")).toBe("ember");
    expect(normalizeThemePref("dark")).toBe("dark");
  });

  it("resolveTheme_handles_ember", () => {
    expect(resolveTheme("ember", true)).toBe("ember");
    expect(resolveTheme("ember", false)).toBe("ember");
  });

  it("ember_theme_accepted", () => {
    applyThemePreference("ember");
    expect(document.documentElement.dataset.theme).toBe("ember");
    applyThemePreference("system");
  });

  it("ember_surfaces_are_warm", () => {
    const css = readFileSync("src/app.css", "utf8") as unknown as string;
    const block = css.match(/:root\[data-theme="ember"\]\s*\{([^}]*)\}/)?.[1] ?? "";
    const bg = block.match(/--bg:\s*(#\w{6})/)?.[1];
    expect(bg, "ember --bg must exist").toBeTruthy();

    const [r, g, b] = [1, 3, 5].map((i) => parseInt(bg!.slice(i, i + 2), 16));
    const max = Math.max(r, g, b) / 255;
    const min = Math.min(r, g, b) / 255;
    const light = (max + min) / 2;
    const delta = max - min;
    let hue = 0;
    if (delta !== 0) {
      const [rr, gg, bb] = [r / 255, g / 255, b / 255];
      if (max === rr / 1) hue = ((gg - bb) / delta) % 6;
      else if (max === gg) hue = (bb - rr) / delta + 2;
      else hue = (rr - gg) / delta + 4;
      hue = Math.round(hue * 60);
      if (hue < 0) hue += 360;
    }
    expect(hue, `ember --bg hue ${hue} must be warm (15-45)`).toBeGreaterThanOrEqual(15);
    expect(hue).toBeLessThanOrEqual(45);
    expect(light).toBeLessThan(0.3);
  });
});
