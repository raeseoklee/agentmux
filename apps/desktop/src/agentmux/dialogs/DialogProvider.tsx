import {
  createContext,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { DialogQueue, type DialogKind, type DialogQueueItem } from "./DialogQueue";
import {
  keyboardEventToStroke,
  normalizeShortcutBinding,
  type ShortcutBindingValue,
} from "../actions";
import { createTranslator, normalizeLanguage } from "../i18n";
import "./dialogs.css";

export type DialogTone = "default" | "danger" | "warning";

function dialogTranslator() {
  const language = normalizeLanguage(
    typeof document === "undefined" ? "en" : document.documentElement.lang,
  );
  return createTranslator(language);
}

export interface ConfirmDialogOptions {
  requestKey?: string;
  title: string;
  description?: string;
  detail?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: DialogTone;
  testId?: string;
}

export interface TextPromptOptions {
  requestKey?: string;
  title: string;
  label: string;
  description?: string;
  initialValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  multiline?: boolean;
  required?: boolean;
  validate?: (value: string) => string | null | undefined;
  testId?: string;
}

export type DialogFieldValue = string | boolean;

export interface DialogSelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface DialogFormField {
  id: string;
  label: string;
  description?: string;
  type?: "text" | "password" | "textarea" | "select" | "checkbox";
  initialValue?: DialogFieldValue;
  placeholder?: string;
  required?: boolean;
  options?: DialogSelectOption[];
  validate?: (value: DialogFieldValue, values: DialogFormValues) => string | null | undefined;
}

export type DialogFormValues = Record<string, DialogFieldValue>;

export interface FormDialogOptions {
  requestKey?: string;
  title: string;
  description?: string;
  fields: DialogFormField[];
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: DialogTone;
  testId?: string;
}

export interface NoticeDialogOptions {
  requestKey?: string;
  title: string;
  description?: string;
  detail?: string;
  acknowledgeLabel?: string;
  tone?: DialogTone;
  testId?: string;
}

export interface ShortcutCaptureOptions {
  requestKey?: string;
  title: string;
  description?: string;
  initialValue?: ShortcutBindingValue;
  firstStrokeLabel?: string;
  secondStrokeLabel?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  clearLabel?: string;
  testId?: string;
}

export interface ToastOptions {
  title: string;
  description?: string;
  tone?: DialogTone | "success";
  durationMs?: number;
  actionLabel?: string;
  onAction?: () => void;
  testId?: string;
}

interface ToastEntry extends Omit<ToastOptions, "tone"> {
  id: number;
  tone: NonNullable<ToastOptions["tone"]>;
}

interface DialogController {
  isDialogOpen: boolean;
  confirm(options: ConfirmDialogOptions): Promise<boolean>;
  prompt(options: TextPromptOptions): Promise<string | null>;
  form(options: FormDialogOptions): Promise<DialogFormValues | null>;
  notice(options: NoticeDialogOptions): Promise<void>;
  captureShortcut(options: ShortcutCaptureOptions): Promise<ShortcutBindingValue | undefined>;
  cancelRequest(requestKey: string): boolean;
  toast(options: ToastOptions): number;
  dismissToast(id: number): void;
}

const DialogContext = createContext<DialogController | null>(null);

export function useAppDialogs(): DialogController {
  const dialogs = useContext(DialogContext);
  if (dialogs === null) {
    throw new Error("useAppDialogs must be used inside DialogProvider");
  }
  return dialogs;
}

export interface DialogProviderProps {
  children: ReactNode;
}

function defaultFormValues(fields: DialogFormField[]): DialogFormValues {
  return Object.fromEntries(
    fields.map((field) => [
      field.id,
      field.initialValue ?? (field.type === "checkbox" ? false : ""),
    ]),
  );
}

function dialogId(item: DialogQueueItem<unknown>, suffix: string): string {
  return `agentmux-dialog-${item.id}-${suffix}`;
}

function isFocusable(element: HTMLElement): boolean {
  return !element.hasAttribute("disabled") && element.tabIndex >= 0;
}

function DialogFrame({
  item,
  title,
  description,
  tone = "default",
  onCancel,
  onSubmit,
  children,
  footer,
}: {
  item: DialogQueueItem<unknown>;
  title: string;
  description?: string;
  tone?: DialogTone;
  onCancel?: () => void;
  onSubmit?: () => void;
  children: ReactNode;
  footer: ReactNode;
}) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const titleId = dialogId(item, "title");
  const descriptionId = description ? dialogId(item, "description") : undefined;

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) {
      return;
    }
    const autofocus = dialog.querySelector<HTMLElement>("[data-dialog-autofocus='true']");
    const firstFocusable = dialog.querySelector<HTMLElement>(
      "button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])",
    );
    (autofocus ?? firstFocusable ?? dialog).focus();
  }, [item.id]);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && onCancel) {
      event.preventDefault();
      onCancel();
      return;
    }
    if (
      event.key === "Enter" &&
      !event.shiftKey &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.metaKey &&
      onSubmit &&
      !(event.target instanceof HTMLTextAreaElement) &&
      !(event.target instanceof HTMLButtonElement) &&
      !(event.target instanceof HTMLSelectElement)
    ) {
      event.preventDefault();
      onSubmit();
      return;
    }
    if (event.key === "Tab") {
      const dialog = dialogRef.current;
      if (dialog === null) {
        return;
      }
      const focusable = Array.from(
        dialog.querySelectorAll<HTMLElement>(
          "button, input, textarea, select, [tabindex]",
        ),
      ).filter(isFocusable);
      if (focusable.length === 0) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      const current = document.activeElement as HTMLElement | null;
      const currentIndex = current ? focusable.indexOf(current) : -1;
      const nextIndex = event.shiftKey
        ? currentIndex <= 0
          ? focusable.length - 1
          : currentIndex - 1
        : currentIndex === focusable.length - 1
          ? 0
          : currentIndex + 1;
      event.preventDefault();
      focusable[nextIndex]?.focus();
    }
  };

  return (
    <div
      className="agentmux-dialog-backdrop"
      data-testid="agentmux-dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onCancel?.();
        }
      }}
    >
      <div
        ref={dialogRef}
        className={`agentmux-dialog agentmux-dialog--${tone}`}
        data-agentmux-app-dialog="true"
        data-testid={item.options && typeof item.options === "object" && "testId" in item.options
          ? String(item.options.testId ?? "agentmux-dialog")
          : "agentmux-dialog"}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <div className="agentmux-dialog__header">
          <h2 id={titleId}>{title}</h2>
          {description ? <p id={descriptionId}>{description}</p> : null}
        </div>
        <div className="agentmux-dialog__body">{children}</div>
        <div className="agentmux-dialog__footer">{footer}</div>
      </div>
    </div>
  );
}

function DialogButton({
  children,
  onClick,
  variant = "secondary",
  autoFocus = false,
}: {
  children: ReactNode;
  onClick: () => void;
  variant?: "primary" | "secondary" | "danger";
  autoFocus?: boolean;
}) {
  return (
    <button
      type="button"
      className={`agentmux-dialog__button agentmux-dialog__button--${variant}`}
      data-dialog-autofocus={autoFocus ? "true" : undefined}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function ConfirmDialog({
  item,
  options,
  resolve,
}: {
  item: DialogQueueItem<unknown>;
  options: ConfirmDialogOptions;
  resolve: (value: boolean) => void;
}) {
  const t = dialogTranslator();
  return (
    <DialogFrame
      item={item}
      title={options.title}
      description={options.description}
      tone={options.tone}
      onCancel={() => resolve(false)}
      footer={
        <>
          <DialogButton onClick={() => resolve(false)} autoFocus>
            {options.cancelLabel ?? t("common.cancel")}
          </DialogButton>
          <DialogButton
            onClick={() => resolve(true)}
            variant={options.tone === "danger" ? "danger" : "primary"}
          >
            {options.confirmLabel ?? t("dialog.confirm")}
          </DialogButton>
        </>
      }
    >
      {options.detail ? <p className="agentmux-dialog__detail">{options.detail}</p> : null}
    </DialogFrame>
  );
}

function PromptDialog({
  item,
  options,
  resolve,
}: {
  item: DialogQueueItem<unknown>;
  options: TextPromptOptions;
  resolve: (value: string | null) => void;
}) {
  const t = dialogTranslator();
  const [value, setValue] = useState(options.initialValue ?? "");
  const [error, setError] = useState<string | null>(null);
  const inputId = dialogId(item, "input");

  const submit = () => {
    const trimmed = value.trim();
    const validationError =
      (options.required && !trimmed
        ? t("dialog.required", { label: options.label })
        : null) ??
      options.validate?.(value) ??
      null;
    if (validationError) {
      setError(validationError);
      return;
    }
    resolve(value);
  };

  const control = options.multiline ? (
    <textarea
      id={inputId}
      className="agentmux-dialog__control agentmux-dialog__textarea"
      data-dialog-autofocus="true"
      value={value}
      placeholder={options.placeholder}
      aria-invalid={error ? true : undefined}
      aria-describedby={error ? dialogId(item, "error") : undefined}
      onChange={(event) => {
        setValue(event.target.value);
        setError(null);
      }}
    />
  ) : (
    <input
      id={inputId}
      className="agentmux-dialog__control"
      data-dialog-autofocus="true"
      value={value}
      placeholder={options.placeholder}
      aria-invalid={error ? true : undefined}
      aria-describedby={error ? dialogId(item, "error") : undefined}
      onChange={(event) => {
        setValue(event.target.value);
        setError(null);
      }}
    />
  );

  return (
    <DialogFrame
      item={item}
      title={options.title}
      description={options.description}
      onCancel={() => resolve(null)}
      onSubmit={submit}
      footer={
        <>
          <DialogButton onClick={() => resolve(null)}>
            {options.cancelLabel ?? t("common.cancel")}
          </DialogButton>
          <DialogButton onClick={submit} variant="primary">
            {options.confirmLabel ?? t("common.save")}
          </DialogButton>
        </>
      }
    >
      <label className="agentmux-dialog__field" htmlFor={inputId}>
        <span>{options.label}</span>
        {control}
      </label>
      {error ? <p id={dialogId(item, "error")} className="agentmux-dialog__error">{error}</p> : null}
    </DialogFrame>
  );
}

function FormDialog({
  item,
  options,
  resolve,
}: {
  item: DialogQueueItem<unknown>;
  options: FormDialogOptions;
  resolve: (value: DialogFormValues | null) => void;
}) {
  const t = dialogTranslator();
  const [values, setValues] = useState<DialogFormValues>(() => defaultFormValues(options.fields));
  const [errors, setErrors] = useState<Record<string, string>>({});

  const submit = () => {
    const nextErrors: Record<string, string> = {};
    for (const field of options.fields) {
      const value = values[field.id];
      if (field.required && (value === "" || value === false)) {
        nextErrors[field.id] = t("dialog.required", { label: field.label });
        continue;
      }
      const validationError = field.validate?.(value, values);
      if (validationError) {
        nextErrors[field.id] = validationError;
      }
    }
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length === 0) {
      resolve(values);
    }
  };

  const updateValue = (field: DialogFormField, value: DialogFieldValue) => {
    setValues((current) => ({ ...current, [field.id]: value }));
    setErrors((current) => {
      if (!(field.id in current)) {
        return current;
      }
      const next = { ...current };
      delete next[field.id];
      return next;
    });
  };

  return (
    <DialogFrame
      item={item}
      title={options.title}
      description={options.description}
      tone={options.tone}
      onCancel={() => resolve(null)}
      onSubmit={submit}
      footer={
        <>
          <DialogButton onClick={() => resolve(null)}>
            {options.cancelLabel ?? t("common.cancel")}
          </DialogButton>
          <DialogButton
            onClick={submit}
            variant={options.tone === "danger" ? "danger" : "primary"}
          >
            {options.confirmLabel ?? t("common.save")}
          </DialogButton>
        </>
      }
    >
      <div className="agentmux-dialog__fields">
        {options.fields.map((field, index) => {
          const inputId = dialogId(item, `field-${field.id}`);
          const error = errors[field.id];
          const autofocus = index === 0 ? "true" : undefined;
          if (field.type === "checkbox") {
            return (
              <label className="agentmux-dialog__checkbox" key={field.id} htmlFor={inputId}>
                <input
                  id={inputId}
                  type="checkbox"
                  checked={values[field.id] === true}
                  data-dialog-autofocus={autofocus}
                  onChange={(event) => updateValue(field, event.target.checked)}
                />
                <span>{field.label}</span>
                {field.description ? <small>{field.description}</small> : null}
              </label>
            );
          }
          return (
            <label className="agentmux-dialog__field" key={field.id} htmlFor={inputId}>
              <span>{field.label}</span>
              {field.description ? <small>{field.description}</small> : null}
              {field.type === "textarea" ? (
                <textarea
                  id={inputId}
                  className="agentmux-dialog__control agentmux-dialog__textarea"
                  value={String(values[field.id] ?? "")}
                  placeholder={field.placeholder}
                  data-dialog-autofocus={autofocus}
                  aria-invalid={error ? true : undefined}
                  aria-describedby={error ? dialogId(item, `error-${field.id}`) : undefined}
                  onChange={(event) => updateValue(field, event.target.value)}
                />
              ) : field.type === "select" ? (
                <select
                  id={inputId}
                  className="agentmux-dialog__control"
                  value={String(values[field.id] ?? "")}
                  data-dialog-autofocus={autofocus}
                  aria-invalid={error ? true : undefined}
                  aria-describedby={error ? dialogId(item, `error-${field.id}`) : undefined}
                  onChange={(event) => updateValue(field, event.target.value)}
                >
                  {field.options?.map((option) => (
                    <option key={option.value} value={option.value} disabled={option.disabled}>
                      {option.label}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  id={inputId}
                  className="agentmux-dialog__control"
                  type={field.type === "password" ? "password" : "text"}
                  value={String(values[field.id] ?? "")}
                  placeholder={field.placeholder}
                  data-dialog-autofocus={autofocus}
                  aria-invalid={error ? true : undefined}
                  aria-describedby={error ? dialogId(item, `error-${field.id}`) : undefined}
                  onChange={(event) => updateValue(field, event.target.value)}
                />
              )}
              {error ? <small id={dialogId(item, `error-${field.id}`)} className="agentmux-dialog__error">{error}</small> : null}
            </label>
          );
        })}
      </div>
    </DialogFrame>
  );
}

function NoticeDialog({
  item,
  options,
  resolve,
}: {
  item: DialogQueueItem<unknown>;
  options: NoticeDialogOptions;
  resolve: () => void;
}) {
  const t = dialogTranslator();
  return (
    <DialogFrame
      item={item}
      title={options.title}
      description={options.description}
      tone={options.tone}
      onCancel={resolve}
      footer={
        <DialogButton onClick={resolve} autoFocus variant="primary">
          {options.acknowledgeLabel ?? t("dialog.confirm")}
        </DialogButton>
      }
    >
      {options.detail ? <p className="agentmux-dialog__detail">{options.detail}</p> : null}
    </DialogFrame>
  );
}

function formatCapturedStroke(stroke: string): string {
  return stroke
    .split("+")
    .map((part) => {
      if (part === "ctrl") return "Ctrl";
      if (part === "alt") return "Alt";
      if (part === "shift") return "Shift";
      if (part === "meta") return "Win";
      return part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1);
    })
    .join("+");
}

function ShortcutCaptureDialog({
  item,
  options,
  resolve,
}: {
  item: DialogQueueItem<unknown>;
  options: ShortcutCaptureOptions;
  resolve: (value: ShortcutBindingValue | undefined) => void;
}) {
  const t = dialogTranslator();
  const initial = normalizeShortcutBinding(options.initialValue ?? null)?.strokes ?? [];
  const [strokes, setStrokes] = useState<string[]>(() => [...initial]);

  const capture = (index: number, event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "Escape") {
      return;
    }
    if (
      event.key === "Tab" &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.metaKey
    ) {
      return;
    }
    const stroke = keyboardEventToStroke(event.nativeEvent);
    if (!stroke) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    setStrokes((current) => {
      if (index === 0) {
        return current.length > 1 ? [stroke, current[1]] : [stroke];
      }
      return [current[0], stroke];
    });
  };

  const submit = () => {
    if (strokes.length === 0) {
      resolve(null);
      return;
    }
    resolve(strokes.length > 1 ? [strokes[0], strokes[1]] : strokes[0]);
  };

  return (
    <DialogFrame
      item={item}
      title={options.title}
      description={options.description}
      onCancel={() => resolve(undefined)}
      footer={
        <>
          <DialogButton onClick={() => setStrokes([])}>
            {options.clearLabel ?? t("common.clear")}
          </DialogButton>
          <DialogButton onClick={() => resolve(undefined)}>
            {options.cancelLabel ?? t("common.cancel")}
          </DialogButton>
          <DialogButton onClick={submit} variant="primary">
            {options.confirmLabel ?? t("common.save")}
          </DialogButton>
        </>
      }
    >
      <div className="agentmux-shortcut-capture">
        <label>
          <span>{options.firstStrokeLabel ?? t("settings.keys")}</span>
          <button
            type="button"
            className="agentmux-shortcut-capture__key"
            data-dialog-autofocus="true"
            onKeyDown={(event) => capture(0, event)}
          >
            {strokes[0] ? formatCapturedStroke(strokes[0]) : t("dialog.pressKey")}
          </button>
        </label>
        {strokes[0] ? (
          <label>
            <span>{options.secondStrokeLabel ?? t("shortcuts.secondStroke")}</span>
            <button
              type="button"
              className="agentmux-shortcut-capture__key"
              onKeyDown={(event) => capture(1, event)}
            >
              {strokes[1]
                ? formatCapturedStroke(strokes[1])
                : t("dialog.pressSecondKey")}
            </button>
          </label>
        ) : null}
      </div>
    </DialogFrame>
  );
}

function ActiveDialog({
  item,
  resolve,
}: {
  item: DialogQueueItem<unknown>;
  resolve: (value: unknown) => void;
}) {
  switch (item.kind as DialogKind) {
    case "confirm":
      return <ConfirmDialog item={item} options={item.options as ConfirmDialogOptions} resolve={resolve} />;
    case "prompt":
      return <PromptDialog item={item} options={item.options as TextPromptOptions} resolve={resolve} />;
    case "form":
      return <FormDialog item={item} options={item.options as FormDialogOptions} resolve={resolve} />;
    case "notice":
      return <NoticeDialog item={item} options={item.options as NoticeDialogOptions} resolve={() => resolve(undefined)} />;
    case "shortcut":
      return <ShortcutCaptureDialog item={item} options={item.options as ShortcutCaptureOptions} resolve={resolve} />;
  }
}

function ToastHost({
  toasts,
  dismiss,
}: {
  toasts: ToastEntry[];
  dismiss: (id: number) => void;
}) {
  const t = dialogTranslator();
  return (
    <div className="agentmux-toast-host" aria-live="polite" aria-relevant="additions">
      {toasts.map((toast) => (
        <div
          className={`agentmux-toast agentmux-toast--${toast.tone}`}
          data-testid={toast.testId ?? "agentmux-toast"}
          key={toast.id}
          role="status"
        >
          <div className="agentmux-toast__content">
            <strong>{toast.title}</strong>
            {toast.description ? <span>{toast.description}</span> : null}
          </div>
          {toast.actionLabel && toast.onAction ? (
            <button
              className="agentmux-toast__action"
              type="button"
              onClick={() => {
                toast.onAction?.();
                dismiss(toast.id);
              }}
            >
              {toast.actionLabel}
            </button>
          ) : null}
          <button
            className="agentmux-toast__dismiss"
            type="button"
            aria-label={t("dialog.dismissToast", { title: toast.title })}
            onClick={() => dismiss(toast.id)}
          >
            x
          </button>
        </div>
      ))}
    </div>
  );
}

export function DialogProvider({ children }: DialogProviderProps) {
  const queueRef = useRef(new DialogQueue());
  const [active, setActive] = useState<DialogQueueItem<unknown> | null>(() => queueRef.current.active());
  const [toasts, setToasts] = useState<ToastEntry[]>([]);
  const toastIdRef = useRef(1);
  const toastTimersRef = useRef(new Map<number, ReturnType<typeof window.setTimeout>>());
  const restoreFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    return () => {
      queueRef.current.cancelAll();
      for (const timer of toastTimersRef.current.values()) {
        window.clearTimeout(timer);
      }
      toastTimersRef.current.clear();
    };
  }, []);

  useEffect(() => {
    if (active === null && restoreFocusRef.current !== null) {
      const previousFocus = restoreFocusRef.current;
      restoreFocusRef.current = null;
      queueMicrotask(() => previousFocus.focus?.());
    }
  }, [active]);

  const settle = useCallback((value: unknown) => {
    const next = queueRef.current.resolveActive(value);
    setActive(next);
  }, []);

  const enqueue = useCallback(<T,>(kind: DialogKind, options: unknown, cancelValue: T) => {
    if (
      queueRef.current.active() === null &&
      restoreFocusRef.current === null &&
      document.activeElement instanceof HTMLElement
    ) {
      restoreFocusRef.current = document.activeElement;
    }
    const requestKey =
      options && typeof options === "object" && "requestKey" in options
        ? String(options.requestKey ?? "") || undefined
        : undefined;
    const request = queueRef.current.enqueue(kind, options, cancelValue, requestKey);
    setActive(queueRef.current.active());
    return request;
  }, []);

  const dismissToast = useCallback((id: number) => {
    const timer = toastTimersRef.current.get(id);
    if (timer !== undefined) {
      window.clearTimeout(timer);
      toastTimersRef.current.delete(id);
    }
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const controller = useMemo<DialogController>(() => ({
    isDialogOpen: active !== null,
    confirm: (options) => enqueue("confirm", options, false),
    prompt: (options) => enqueue("prompt", options, null),
    form: (options) => enqueue("form", options, null),
    notice: (options) => enqueue("notice", options, undefined),
    captureShortcut: (options) => enqueue("shortcut", options, undefined),
    cancelRequest: (requestKey) => {
      const canceled = queueRef.current.cancel(requestKey);
      if (canceled) {
        setActive(queueRef.current.active());
      }
      return canceled;
    },
    toast: (options) => {
      const id = toastIdRef.current++;
      const toast: ToastEntry = {
        ...options,
        id,
        tone: options.tone ?? "default",
        title: options.title,
      };
      setToasts((current) => [...current, toast]);
      const durationMs = options.durationMs ?? 5000;
      if (durationMs > 0) {
        const timer = window.setTimeout(() => dismissToast(id), durationMs);
        toastTimersRef.current.set(id, timer);
      }
      return id;
    },
    dismissToast,
  }), [active, dismissToast, enqueue]);

  return (
    <DialogContext.Provider value={controller}>
      {children}
      {active ? <ActiveDialog item={active} resolve={settle} /> : null}
      <ToastHost toasts={toasts} dismiss={dismissToast} />
    </DialogContext.Provider>
  );
}

export const __dialogTesting = {
  defaultFormValues,
};
