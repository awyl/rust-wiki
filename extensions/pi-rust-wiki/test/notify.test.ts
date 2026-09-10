import { describe, expect, it, vi } from "vitest";
import { notify } from "../llm-wiki-skills/lib/notify.js";

describe("notify", () => {
  it("forwards to ctx.ui.notify with the level", () => {
    const spy = vi.fn();
    notify({ ui: { notify: spy } }, "hello", "warning");
    expect(spy).toHaveBeenCalledWith("hello", "warning");
  });

  it("defaults to info", () => {
    const spy = vi.fn();
    notify({ ui: { notify: spy } }, "hi");
    expect(spy).toHaveBeenCalledWith("hi", "info");
  });

  it("swallows a stale ctx whose ui accessor throws", () => {
    // Exactly what pi raises after ctx.reload() / newSession / fork.
    const stale = {
      get ui(): { notify: () => void } {
        throw new Error("This extension ctx is stale after session replacement or reload.");
      },
    };
    expect(() => notify(stale, "boom")).not.toThrow();
  });

  it("swallows a throwing notify, a missing ui, and a missing ctx", () => {
    const throwing = {
      ui: {
        notify: () => {
          throw new Error("nope");
        },
      },
    };
    expect(() => notify(throwing, "x")).not.toThrow();
    expect(() => notify({}, "x")).not.toThrow();
    expect(() => notify(undefined, "x")).not.toThrow();
  });
});
