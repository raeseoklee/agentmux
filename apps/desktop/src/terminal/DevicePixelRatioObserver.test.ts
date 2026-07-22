import { describe, expect, it, vi } from "vitest";
import {
  observeDevicePixelRatio,
  type DevicePixelRatioSource,
} from "./DevicePixelRatioObserver";

describe("observeDevicePixelRatio", () => {
  it("reports a scale change and rearms for the new resolution", () => {
    let ratio = 1;
    const listeners: Array<() => void> = [];
    const queries: string[] = [];
    const source: DevicePixelRatioSource = {
      read: () => ratio,
      match: (query) => {
        queries.push(query);
        return {
          addEventListener: (_type, listener) => listeners.push(listener),
          removeEventListener: () => {},
        };
      },
    };
    const onChange = vi.fn();
    const dispose = observeDevicePixelRatio(onChange, source);

    ratio = 1.5;
    listeners[0]?.();

    expect(onChange).toHaveBeenCalledWith(1.5);
    expect(queries).toEqual([
      "(resolution: 1dppx)",
      "(resolution: 1.5dppx)",
    ]);
    dispose();
  });

  it("ignores media changes when the effective ratio is unchanged", () => {
    const listeners: Array<() => void> = [];
    const onChange = vi.fn();
    const dispose = observeDevicePixelRatio(onChange, {
      read: () => 2,
      match: () => ({
        addEventListener: (_type, listener) => listeners.push(listener),
        removeEventListener: () => {},
      }),
    });

    listeners[0]?.();

    expect(onChange).not.toHaveBeenCalled();
    dispose();
  });
});
