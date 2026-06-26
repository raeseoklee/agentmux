// Theme tokens and accent palette for the agentmux Terminal design.
// Ported from the "agentmux Terminal.dc.html" Claude Design prototype.
import type { CSSProperties } from "react";

export type ThemeName = "dark" | "light";

export interface ThemeTokens {
  bg: string;
  canvas: string;
  surface: string;
  s2: string;
  s3: string;
  border: string;
  borderStrong: string;
  borderSubtle: string;
  fg1: string;
  fg2: string;
  fg3: string;
  fg4: string;
  term: string;
  desk: string;
  green: string;
  red: string;
  warn: string;
  info: string;
  cyan: string;
  shadow: string;
}

export const THEMES: Record<ThemeName, ThemeTokens> = {
  dark: {
    bg: "#0A0A0B",
    canvas: "#0D0D0F",
    surface: "#161618",
    s2: "#1F1F23",
    s3: "#27272A",
    border: "#262629",
    borderStrong: "#3A3A40",
    borderSubtle: "#1C1C1F",
    fg1: "#F4F4F5",
    fg2: "#C9C9CF",
    fg3: "#9A9AA2",
    fg4: "#6B6B73",
    term: "#0B0B0D",
    desk: "#000000",
    green: "#4ADE80",
    red: "#F87171",
    warn: "#FBBF24",
    info: "#7AA2F7",
    cyan: "#34D3D3",
    shadow: "0 24px 70px rgba(0,0,0,0.6)"
  },
  light: {
    bg: "#F4F5F7",
    canvas: "#FFFFFF",
    surface: "#FFFFFF",
    s2: "#F4F4F5",
    s3: "#ECECEE",
    border: "#E4E4E7",
    borderStrong: "#D4D4D8",
    borderSubtle: "#EFEFF1",
    fg1: "#0A0A0B",
    fg2: "#3F3F46",
    fg3: "#71717A",
    fg4: "#A1A1AA",
    term: "#FCFCFD",
    desk: "#D9D7D1",
    green: "#16A34A",
    red: "#DC2626",
    warn: "#D97706",
    info: "#2563EB",
    cyan: "#0891B2",
    shadow: "0 24px 70px rgba(10,10,11,0.22)"
  }
};

export interface Accent {
  key: string;
  label: string;
  hex: string;
  hover: string;
  soft: string;
}

export const ACCENTS: Accent[] = [
  { key: "blue", label: "Azure", hex: "#3B82F6", hover: "#2563EB", soft: "rgba(59,130,246,0.16)" },
  { key: "orange", label: "Coral", hex: "#F0561D", hover: "#D9491A", soft: "rgba(240,86,29,0.16)" },
  { key: "green", label: "Emerald", hex: "#10B981", hover: "#059669", soft: "rgba(16,185,129,0.16)" },
  { key: "violet", label: "Iris", hex: "#8B5CF6", hover: "#7C3AED", soft: "rgba(139,92,246,0.16)" }
];

// Build the CSS custom-property map applied to the prototype root element so
// every nested inline style can reference var(--accent), var(--fg1), etc.
export function buildRootVars(theme: ThemeTokens, accent: Accent, fontSize: number): CSSProperties {
  return {
    "--bg": theme.bg,
    "--canvas": theme.canvas,
    "--surface": theme.surface,
    "--s2": theme.s2,
    "--s3": theme.s3,
    "--border": theme.border,
    "--border-strong": theme.borderStrong,
    "--border-subtle": theme.borderSubtle,
    "--fg1": theme.fg1,
    "--fg2": theme.fg2,
    "--fg3": theme.fg3,
    "--fg4": theme.fg4,
    "--info": theme.info,
    "--term": theme.term,
    "--accent": accent.hex,
    "--accent-hover": accent.hover,
    "--accent-soft": accent.soft,
    "--term-fs": `${fontSize}px`
  } as CSSProperties;
}
