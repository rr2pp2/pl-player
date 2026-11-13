// 着せ替え（v3.2.0）：スキンが変えてよい CSS 変数と、その値の確かめ・当て方。
// Rust（src-tauri\src\skins\tokens.rs）が取り込みのときに確かめて形をそろえた値を、当てる前にもう一度ここで確かめる（二重の守り）。
// 任意の CSS の文字列は受け取らない。レイアウトの寸法（--row-h など）と背景の設定（--bg-dim など）は変えさせない。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

/** テーマごとに変えてよい色（src\theme\dark.css・light.css と同じ名前） */
export const COLOR_VARS = [
  "--c-bg",
  "--c-panel-rgb",
  "--c-overlay-rgb",
  "--c-primary",
  "--c-secondary",
  "--c-accent",
  "--c-accent-contrast",
  "--c-hover",
  "--c-selected",
  "--c-border",
  "--c-input",
  "--c-danger",
  "--c-ok",
] as const;

/** 文字の大きさの既定値（src\theme\tokens.css）。スキンは ±2px まで */
export const FONT_SIZE_DEFAULTS: Record<string, number> = {
  "--fs-title": 17,
  "--fs-title-list": 16,
  "--fs-artist": 13,
  "--fs-body": 14,
  "--fs-settings": 14,
  "--fs-small": 12,
};
export const FONT_SIZE_RANGE = 2;
export const RADIUS_VARS = ["--radius", "--radius-card", "--radius-small"];
export const DURATION_VARS = ["--dur-hover", "--dur-panel", "--dur-page", "--dur-bg"];

/** スキンから変えさせない（当てるときにも外す） */
export const LOCKED_VARS = [
  "--sidebar-w",
  "--sidebar-w-collapsed",
  "--row-h",
  "--row-art",
  "--player-h",
  "--topbar-h",
  "--lyrics-row-h",
  "--lyrics-row-anchor",
  "--bg-dim",
  "--bg-blur",
  "--ui-alpha",
];

/** 既定の文字の並び（tokens.css の --font-ui と同じ）。スキンのフォント名はこの前に足す */
export const DEFAULT_FONT_STACK = '"Yu Gothic UI", "Segoe UI", "Meiryo UI", "Meiryo", system-ui, sans-serif';

const EASE: Record<string, string> = { standard: "cubic-bezier(0.2, 0, 0, 1)", linear: "linear", "ease-out": "ease-out" };

/** このファイルが触る変数の全部（当てる前に全部外すため） */
export const SKIN_VARS: string[] = [
  ...COLOR_VARS,
  ...Object.keys(FONT_SIZE_DEFAULTS),
  ...RADIUS_VARS,
  ...DURATION_VARS,
  "--ease",
  "--font-ui",
  "--shadow-panel",
  "--shadow-pop",
];

const byte = (s: string): number | null => {
  const t = s.trim();
  if (!/^\d{1,3}$/.test(t)) return null;
  const n = Number(t);
  return n <= 255 ? n : null;
};

const alpha = (s: string): string | null => {
  const t = s.trim();
  if (!/^(?:[01]|[01]?\.\d{1,3})$/.test(t)) return null;
  const n = Number(t.startsWith(".") ? "0" + t : t);
  if (!(n >= 0 && n <= 1)) return null;
  return String(Math.round(n * 1000) / 1000);
};

const triple = (s: string): [number, number, number] | null => {
  const p = s.split(",");
  if (p.length !== 3) return null;
  const v = p.map(byte);
  return v.every((x) => x !== null) ? (v as [number, number, number]) : null;
};

const hex2 = (n: number) => n.toString(16).padStart(2, "0");

/** 色の値（`#rgb`・`#rrggbb`・`rgb(r, g, b)`・`rgba(r, g, b, a)`）。形をそろえて返す。ほかは null */
export function validColor(v: unknown): string | null {
  if (typeof v !== "string") return null;
  const t = v.trim();
  let m = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.exec(t);
  if (m) {
    const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
    return "#" + h.toLowerCase();
  }
  m = /^rgba\(([^()]*)\)$/.exec(t);
  if (m) {
    const p = m[1].split(",");
    if (p.length !== 4) return null;
    const rgb = triple(p.slice(0, 3).join(","));
    const a = alpha(p[3]);
    return rgb && a !== null ? `rgba(${rgb[0]}, ${rgb[1]}, ${rgb[2]}, ${a})` : null;
  }
  m = /^rgb\(([^()]*)\)$/.exec(t);
  if (m) {
    const rgb = triple(m[1]);
    return rgb ? "#" + rgb.map(hex2).join("") : null;
  }
  return null;
}

/** `--c-panel-rgb`・`--c-overlay-rgb` の形（`r, g, b`） */
export function validRgbTriple(v: unknown): string | null {
  if (typeof v !== "string") return null;
  const t = triple(v);
  return t ? t.join(", ") : null;
}

/** テーマの色の1つ（名前が許したものでなければ null） */
export function validColorToken(name: string, v: unknown): string | null {
  if (!(COLOR_VARS as readonly string[]).includes(name)) return null;
  return name.endsWith("-rgb") ? validRgbTriple(v) : validColor(v);
}

function numberWithUnit(v: unknown, unit: string, min: number, max: number): string | null {
  let n: number;
  if (typeof v === "number") n = v;
  else if (typeof v === "string") {
    const m = new RegExp(`^(\\d{1,4})${unit}$`).exec(v.trim());
    if (!m) return null;
    n = Number(m[1]);
  } else return null;
  if (!Number.isInteger(n) || n < min || n > max) return null;
  return `${n}${unit}`;
}

/** 共通の tokens（文字の大きさ・角丸・動き）の1つ。許していない名前・範囲の外は null */
export function validCommonToken(name: string, v: unknown): string | null {
  if (LOCKED_VARS.includes(name)) return null;
  if (name in FONT_SIZE_DEFAULTS) {
    const d = FONT_SIZE_DEFAULTS[name];
    return numberWithUnit(v, "px", d - FONT_SIZE_RANGE, d + FONT_SIZE_RANGE);
  }
  if (RADIUS_VARS.includes(name)) return numberWithUnit(v, "px", 0, 24);
  if (DURATION_VARS.includes(name)) return numberWithUnit(v, "ms", 100, 250);
  if (name === "--ease") return typeof v === "string" && v in EASE ? EASE[v] : null;
  return null;
}

/** フォントの名前（PC にあるものの名前だけ）：文字・数字・空白・. _ -、1〜64 文字、前後に空白なし */
export function validFontName(s: unknown): boolean {
  return typeof s === "string" && s.length >= 1 && [...s].length <= 64 && s.trim() === s && /^[\p{L}\p{N} ._-]+$/u.test(s);
}

/** 影の段階 → 値（テーマごと。standard は今の値そのもの） */
export function shadowValues(theme: string, preset: unknown): { panel: string; pop: string } | null {
  const light = theme === "light";
  const [a, b] = light ? [0.12, 0.18] : [0.28, 0.35];
  if (preset === "none") return { panel: "none", pop: "none" };
  if (preset === "soft") return { panel: `0 2px 10px rgba(0, 0, 0, ${a / 2})`, pop: `0 4px 16px rgba(0, 0, 0, ${b / 2})` };
  if (preset === "standard") return { panel: `0 4px 18px rgba(0, 0, 0, ${a})`, pop: `0 8px 28px rgba(0, 0, 0, ${b})` };
  return null;
}

/** スキンの見た目の部分（ipc の SkinAppearance と同じ形。ここでは型だけ使う） */
export interface AppearanceLike {
  dark: { tokens: Record<string, string>; shadow: string | null } | null;
  light: { tokens: Record<string, string>; shadow: string | null } | null;
  fontFamily: string[];
  tokens: Record<string, string>;
}

/**
 * 今のテーマに当てる変数を作る。確かめに通らなかった値は入れず、名前を rejected に返す
 * （呼び出し側は、1つでもあれば既定の見た目に戻して知らせる）
 */
export function skinCssVars(a: AppearanceLike | null | undefined, theme: string): { vars: Record<string, string>; rejected: string[] } {
  const vars: Record<string, string> = {};
  const rejected: string[] = [];
  if (!a) return { vars, rejected };
  const t = theme === "light" ? a.light : a.dark;
  for (const [k, v] of Object.entries(t?.tokens ?? {})) {
    const ok = validColorToken(k, v);
    if (ok === null) rejected.push(k);
    else vars[k] = ok;
  }
  if (t?.shadow) {
    const s = shadowValues(theme, t.shadow);
    if (!s) rejected.push("shadow");
    else {
      vars["--shadow-panel"] = s.panel;
      vars["--shadow-pop"] = s.pop;
    }
  }
  for (const [k, v] of Object.entries(a.tokens ?? {})) {
    const ok = validCommonToken(k, v);
    if (ok === null) rejected.push(k);
    else vars[k] = ok;
  }
  const fonts = a.fontFamily ?? [];
  if (fonts.length > 8 || !fonts.every(validFontName)) rejected.push("fontFamily");
  else if (fonts.length > 0) vars["--font-ui"] = [...fonts.map((f) => `"${f}"`), DEFAULT_FONT_STACK].join(", ");
  return { vars, rejected };
}

/** 変数を当てる。先にこのファイルが触る変数を全部外す（既定のスキンでは何も残らない＝今の見た目そのまま） */
export function applyCssVars(style: { setProperty(n: string, v: string): void; removeProperty(n: string): unknown }, vars: Record<string, string>) {
  for (const n of SKIN_VARS) style.removeProperty(n);
  for (const [n, v] of Object.entries(vars)) {
    if (SKIN_VARS.includes(n)) style.setProperty(n, v);
  }
}
