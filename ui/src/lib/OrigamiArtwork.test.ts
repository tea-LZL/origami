import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import OrigamiArtwork from "./OrigamiArtwork.svelte";

describe("OrigamiArtwork", () => {
  it("uses the light launch mark with semantic alternative text", () => {
    render(OrigamiArtwork, {
      props: { variant: "launch", theme: "light", alt: "Origami is loading", loading: "eager" },
    });

    const image = screen.getByAltText("Origami is loading");
    expect(image.getAttribute("src")).toContain("Origami-bg");
    expect(image).toHaveAttribute("loading", "eager");
  });

  it("uses the dark counterpart for dark theme artwork", () => {
    render(OrigamiArtwork, {
      props: { variant: "empty", theme: "dark", alt: "" },
    });

    const image = document.querySelector<HTMLImageElement>(".origami-artwork")!;
    expect(image.getAttribute("src")).toContain("Origami-outline-dark");
    expect(image).toHaveAttribute("alt", "");
    expect(image).toHaveClass("dark");
  });
});
