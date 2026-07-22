import { describe, expect, it } from "vitest";
import { DialogQueue } from "./DialogQueue";
import { __dialogTesting, type DialogFormField } from "./DialogProvider";

describe("DialogQueue", () => {
  it("presents requests in FIFO order and resolves each only once", async () => {
    const queue = new DialogQueue();
    const first = queue.enqueue("confirm", { title: "First" }, false);
    const second = queue.enqueue("prompt", { title: "Second" }, null);

    expect(queue.active()?.kind).toBe("confirm");
    queue.resolveActive(true);
    expect(await first).toBe(true);
    expect(queue.active()?.kind).toBe("prompt");
    queue.resolveActive("AgentMux");
    await expect(second).resolves.toBe("AgentMux");
    expect(queue.active()).toBeNull();
  });

  it("uses each request's cancellation value", async () => {
    const queue = new DialogQueue();
    const confirm = queue.enqueue("confirm", {}, false);
    const prompt = queue.enqueue("prompt", {}, null);

    queue.cancelActive();
    queue.cancelActive();

    await expect(confirm).resolves.toBe(false);
    await expect(prompt).resolves.toBeNull();
  });

  it("cancels active and pending requests when the host unmounts", async () => {
    const queue = new DialogQueue();
    const active = queue.enqueue("form", {}, null);
    const pending = queue.enqueue("notice", {}, undefined);

    queue.cancelAll();

    await expect(active).resolves.toBeNull();
    await expect(pending).resolves.toBeUndefined();
    expect(queue.active()).toBeNull();
  });

  it("cancels a keyed active or pending request without disturbing FIFO order", async () => {
    const queue = new DialogQueue();
    const active = queue.enqueue("confirm", {}, false, "browser:first");
    const pending = queue.enqueue("prompt", {}, null, "browser:second");
    const third = queue.enqueue("notice", {}, undefined, "app:third");

    expect(queue.cancel("browser:second")).toBe(true);
    await expect(pending).resolves.toBeNull();
    expect(queue.active()?.requestKey).toBe("browser:first");

    expect(queue.cancel("browser:first")).toBe(true);
    await expect(active).resolves.toBe(false);
    expect(queue.active()?.requestKey).toBe("app:third");
    queue.resolveActive(undefined);
    await expect(third).resolves.toBeUndefined();
  });
});

describe("dialog form defaults", () => {
  it("initializes every supported field type predictably", () => {
    const fields: DialogFormField[] = [
      { id: "name", label: "Name" },
      { id: "notes", label: "Notes", type: "textarea", initialValue: "Hello" },
      { id: "enabled", label: "Enabled", type: "checkbox" },
      { id: "profile", label: "Profile", type: "select", initialValue: "wsl" },
    ];

    expect(__dialogTesting.defaultFormValues(fields)).toEqual({
      name: "",
      notes: "Hello",
      enabled: false,
      profile: "wsl",
    });
  });
});
