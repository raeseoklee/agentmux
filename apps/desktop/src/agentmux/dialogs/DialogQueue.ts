export type DialogKind = "confirm" | "prompt" | "form" | "notice" | "shortcut";

export interface DialogQueueItem<T> {
  id: number;
  kind: DialogKind;
  options: unknown;
  requestKey?: string;
  cancelValue: T;
  resolve: (value: T) => void;
}

/**
 * Small, framework-independent queue used by DialogProvider. Keeping the
 * resolver lifecycle here makes it straightforward to test cancellation and
 * prevents a dialog request from being orphaned when a host unmounts.
 */
export class DialogQueue {
  private nextId = 1;
  private activeItem: DialogQueueItem<unknown> | null = null;
  private pendingItems: DialogQueueItem<unknown>[] = [];

  enqueue<T>(
    kind: DialogKind,
    options: unknown,
    cancelValue: T,
    requestKey?: string,
  ): Promise<T> {
    return new Promise<T>((resolve) => {
      const item: DialogQueueItem<T> = {
        id: this.nextId++,
        kind,
        options,
        requestKey,
        cancelValue,
        resolve,
      };
      if (this.activeItem === null) {
        this.activeItem = item as DialogQueueItem<unknown>;
        return;
      }
      this.pendingItems.push(item as DialogQueueItem<unknown>);
    });
  }

  active(): DialogQueueItem<unknown> | null {
    return this.activeItem;
  }

  resolveActive<T>(value: T): DialogQueueItem<unknown> | null {
    const item = this.activeItem;
    if (item === null) {
      return null;
    }
    this.activeItem = this.pendingItems.shift() ?? null;
    (item.resolve as (resolved: T) => void)(value);
    return this.activeItem;
  }

  cancelActive(): DialogQueueItem<unknown> | null {
    const item = this.activeItem;
    if (item === null) {
      return null;
    }
    return this.resolveActive(item.cancelValue);
  }

  cancel(requestKey: string): boolean {
    if (this.activeItem?.requestKey === requestKey) {
      this.resolveActive(this.activeItem.cancelValue);
      return true;
    }
    const pendingIndex = this.pendingItems.findIndex(
      (item) => item.requestKey === requestKey,
    );
    if (pendingIndex < 0) {
      return false;
    }
    const [item] = this.pendingItems.splice(pendingIndex, 1);
    item?.resolve(item.cancelValue);
    return true;
  }

  cancelAll(): void {
    const active = this.activeItem;
    const pending = this.pendingItems;
    this.activeItem = null;
    this.pendingItems = [];
    if (active !== null) {
      active.resolve(active.cancelValue);
    }
    for (const item of pending) {
      item.resolve(item.cancelValue);
    }
  }
}
