import { afterEach, describe, expect, it, vi } from "vitest";
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
    expect(normalizeThemePref("ember")).toBe("system"); // widened in a later task
    expect(normalizeThemePref("dark")).toBe("dark");
  });
});
