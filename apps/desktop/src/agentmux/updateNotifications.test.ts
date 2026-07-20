import { describe, expect, it, vi } from "vitest";
import { createTranslator } from "./i18n";
import {
  acknowledgeUpdateNotification,
  createUpdateNotificationSession,
  isNewerRelease,
  notifyUpdateAvailable,
  shouldUseInAppUpdateFallback,
} from "./updateNotifications";

class MemoryStorage {
  readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

describe("update availability notifications", () => {
  it("uses semantic version precedence before notifying", () => {
    expect(isNewerRelease("0.1.10", "0.1.11")).toBe(true);
    expect(isNewerRelease("1.0.0-beta.2", "1.0.0")).toBe(true);
    expect(isNewerRelease("1.0.0", "1.0.0-beta.3")).toBe(false);
    expect(isNewerRelease("1.2.0", "1.1.9")).toBe(false);
    expect(isNewerRelease("invalid", "1.2.0")).toBe(false);
  });

  it("shows a native toast only once per app run", async () => {
    const storage = new MemoryStorage();
    const session = createUpdateNotificationSession();
    const invoke = vi.fn().mockResolvedValue(undefined);
    const input = {
      currentVersion: "0.1.10",
      version: "v0.1.11",
      title: "AgentMux update available",
      body: "AgentMux 0.1.11 is ready.",
      actionLabel: "Open update",
    };

    await expect(
      notifyUpdateAvailable(input, { session, storage, tauri: { invoke } }),
    ).resolves.toBe("shown");
    await expect(
      notifyUpdateAvailable(input, { session, storage, tauri: { invoke } }),
    ).resolves.toBe("duplicate");

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("show_update_available_notification", {
      version: "0.1.11",
      title: input.title,
      body: input.body,
      action_label: input.actionLabel,
    });
    expect(storage.values.size).toBe(0);
  });

  it("retries a dismissed or unseen toast on the next app launch", async () => {
    const storage = new MemoryStorage();
    const invoke = vi.fn().mockResolvedValue(undefined);
    const input = {
      currentVersion: "0.1.10",
      version: "0.1.15",
      title: "Update",
      body: "Ready",
      actionLabel: "Open",
    };

    await expect(
      notifyUpdateAvailable(input, {
        session: createUpdateNotificationSession(),
        storage,
        tauri: { invoke },
      }),
    ).resolves.toBe("shown");
    await expect(
      notifyUpdateAvailable(input, {
        session: createUpdateNotificationSession(),
        storage,
        tauri: { invoke },
      }),
    ).resolves.toBe("shown");

    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("suppresses acknowledged versions across app launches", async () => {
    const storage = new MemoryStorage();
    const invoke = vi.fn().mockResolvedValue(undefined);
    const input = {
      currentVersion: "0.1.10",
      version: "v0.1.16",
      title: "Update",
      body: "Ready",
      actionLabel: "Open",
    };

    expect(acknowledgeUpdateNotification(input.version, storage)).toBe(true);
    await expect(
      notifyUpdateAvailable(input, {
        session: createUpdateNotificationSession(),
        storage,
        tauri: { invoke },
      }),
    ).resolves.toBe("duplicate");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("coalesces concurrent checks for the same version", async () => {
    const storage = new MemoryStorage();
    const session = createUpdateNotificationSession();
    let finish: (() => void) | undefined;
    const invoke = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const input = {
      currentVersion: "0.1.10",
      version: "0.1.12",
      title: "Update",
      body: "Ready",
      actionLabel: "Open",
    };

    const first = notifyUpdateAvailable(input, {
      session,
      storage,
      tauri: { invoke },
    });
    await expect(
      notifyUpdateAvailable(input, { session, storage, tauri: { invoke } }),
    ).resolves.toBe("duplicate");
    finish?.();
    await expect(first).resolves.toBe("shown");
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("uses the in-app fallback once, then retries native delivery next launch", async () => {
    const storage = new MemoryStorage();
    const invoke = vi
      .fn()
      .mockRejectedValueOnce(new Error("toast unavailable"))
      .mockResolvedValueOnce(undefined);
    const input = {
      currentVersion: "0.1.10",
      version: "0.1.13",
      title: "Update",
      body: "Ready",
      actionLabel: "Open",
    };
    const firstSession = createUpdateNotificationSession();

    await expect(
      notifyUpdateAvailable(input, {
        session: firstSession,
        storage,
        tauri: { invoke },
      }),
    ).resolves.toBe("failed");
    await expect(
      notifyUpdateAvailable(input, {
        session: firstSession,
        storage,
        tauri: { invoke },
      }),
    ).resolves.toBe("duplicate");
    await expect(
      notifyUpdateAvailable(input, {
        session: createUpdateNotificationSession(),
        storage,
        tauri: { invoke },
      }),
    ).resolves.toBe("shown");
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("uses the unsupported fallback only once per app run", async () => {
    const storage = new MemoryStorage();
    const session = createUpdateNotificationSession();
    const input = {
      currentVersion: "0.1.10",
      version: "0.1.17",
      title: "Update",
      body: "Ready",
      actionLabel: "Open",
    };

    await expect(
      notifyUpdateAvailable(input, { session, storage, tauri: null }),
    ).resolves.toBe("unsupported");
    await expect(
      notifyUpdateAvailable(input, { session, storage, tauri: null }),
    ).resolves.toBe("duplicate");
  });

  it("keeps in-memory deduplication when persistent storage is unavailable", async () => {
    const session = createUpdateNotificationSession();
    const storage = {
      getItem: () => {
        throw new Error("storage blocked");
      },
      setItem: () => {
        throw new Error("storage blocked");
      },
    };
    const invoke = vi.fn().mockResolvedValue(undefined);
    const input = {
      currentVersion: "0.1.10",
      version: "0.1.14",
      title: "Update",
      body: "Ready",
      actionLabel: "Open",
    };

    await expect(
      notifyUpdateAvailable(input, { session, storage, tauri: { invoke } }),
    ).resolves.toBe("shown");
    await expect(
      notifyUpdateAvailable(input, { session, storage, tauri: { invoke } }),
    ).resolves.toBe("duplicate");
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("routes unsupported and failed delivery to the in-app fallback", () => {
    expect(shouldUseInAppUpdateFallback("unsupported")).toBe(true);
    expect(shouldUseInAppUpdateFallback("failed")).toBe(true);
    expect(shouldUseInAppUpdateFallback("shown")).toBe(false);
    expect(shouldUseInAppUpdateFallback("duplicate")).toBe(false);
  });

  it("provides localized notification text in English and Korean", () => {
    const en = createTranslator("en");
    const ko = createTranslator("ko");
    expect(en("updates.notification.action")).toBe("Open update");
    expect(en("updates.notification.fallback.failed")).toContain(
      "remains available",
    );
    expect(ko("updates.notification.action")).toBe("업데이트 열기");
    expect(ko("updates.notification.body", { version: "0.1.11" })).toContain(
      "0.1.11",
    );
    expect(ko("updates.notification.fallback.unsupported")).toContain(
      "업데이트",
    );
  });
});
