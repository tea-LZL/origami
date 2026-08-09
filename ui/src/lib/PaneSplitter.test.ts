import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import PaneSplitter from "./PaneSplitter.svelte";

describe("PaneSplitter", () => {
  it("supports keyboard resizing within its bounds", async () => {
    const onResize = vi.fn();
    const onResizeEnd = vi.fn();
    const props = {
      value: 260,
      min: 200,
      max: 300,
      label: "Sidebar width",
      onResize,
      onResizeEnd,
    };
    const view = render(PaneSplitter, props);

    const splitter = screen.getByRole("separator", { name: "Sidebar width" });
    await fireEvent.keyDown(splitter, { key: "ArrowRight", shiftKey: true });
    await view.rerender({ ...props, value: 300 });
    await fireEvent.keyDown(splitter, { key: "End" });
    await fireEvent.keyDown(splitter, { key: "ArrowLeft" });

    expect(onResize.mock.calls.map(([width]) => width)).toEqual([300, 300, 284]);
    expect(onResizeEnd).toHaveBeenCalledTimes(3);
  });
});
