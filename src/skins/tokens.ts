// 着せ替え（v3.2.0）：スキンが変えてよい CSS 変数と、その値の確かめ・当て方。
// Rust（src-tauri\src\skins\tokens.rs）が取り込みのときに確かめて形をそろえた値を、当てる前にもう一度ここで確かめる（二重の守り）。
// 任意の CSS の文字列は受け取らない。レイアウトの寸法（--row-h など）と背景の設定（--bg-dim など）は変えさせない。
// v3.10.0：手で選ぶ色（設定の skin.colors）の確かめと重ね方も、このファイルの終わりに置く。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

/** テーマごとに変えてよい色（src\theme\dark.css・light.css と同じ名前）。v3.7.0 で --c-outline・--c-favorite を足した（15 個。Rust の COLOR_VARS と同じ） */
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
  "--c-outline",
  "--c-input",
  "--c-danger",
  "--c-ok",
  "--c-favorite",
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

/** スキンから変えさせない（当てるときにも外す）。Rust（src-tauri\src\skins\tokens.rs）の LOCKED_VARS と同じ並び。
 *  v3.8.0：再生バーの真ん中の列の幅 --player-center-w を足した（3列と1段の切り替えの計算 src\layout\playerLayout.ts が前提にする）。
 *  v3.8.0 段2-2：出力ボタン・出力のプルダウン・ミキサー・メーターの寸法を足した（再生バーの計算と、詳細の配置の計算 src\layout\detailLayout.ts が前提にする） */
export const LOCKED_VARS = [
  "--sidebar-w",
  "--sidebar-w-collapsed",
  "--row-h",
  "--row-art",
  "--player-h",
  "--player-center-w",
  "--player-output-w",
  "--output-popup-w",
  "--mixer-w",
  "--mixer-strip-min-w",
  "--meter-h",
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

// ---------------- 手で選ぶ色（v3.10.0） ----------------
// 「設定 → 着せ替え」の「色」の欄。正は Rust（src-tauri\src\skins\colors.rs）。Rust が形をそろえて設定の skin.colors に置いた値を、
// 当てる前にもう一度ここで確かめる（二重の守り）。名前は上の COLOR_VARS（新しい表は作らない）。
// 重なり順は テーマの既定 < 当てているスキンのそのテーマの色 < 手の色。手の色は Dark・Light 共通の1組。
// 読みやすさの警告は contrast.ts の contrastWarnings に、基準＝既定＋スキンの色、重ねる色＝manualColorVars の値 を渡す（スキンの取り込みと同じ計算）

export type ColorName = (typeof COLOR_VARS)[number];

/** 選び方：色だけ（不透明）・色＋濃さ（半透明の5色）・色だけ（濃さは「UI の不透明度」が決める *-rgb の2色） */
export type ColorKind = "solid" | "alpha" | "rgb";

/** 色＋濃さで選ぶ5色（テーマの既定が半透明の色） */
export const ALPHA_VARS: readonly ColorName[] = ["--c-hover", "--c-selected", "--c-border", "--c-outline", "--c-input"];

/** 画面に出す色の名前（並びは COLOR_VARS のまま） */
export const COLOR_LABELS: Record<ColorName, string> = {
  "--c-bg": "背景",
  "--c-panel-rgb": "板の色（パネル）",
  "--c-overlay-rgb": "重ねる面の色",
  "--c-primary": "文字",
  "--c-secondary": "薄い文字",
  "--c-accent": "アクセント",
  "--c-accent-contrast": "アクセントの上の文字",
  "--c-hover": "指を乗せたとき",
  "--c-selected": "選んだ行・オンのボタン",
  "--c-border": "区切り線",
  "--c-outline": "操作部の輪郭",
  "--c-input": "入力欄",
  "--c-danger": "注意（赤）",
  "--c-ok": "うまくいった（緑）",
  "--c-favorite": "お気に入りの星",
};

export function colorKind(name: string): ColorKind {
  if (name.endsWith("-rgb")) return "rgb";
  return (ALPHA_VARS as readonly string[]).includes(name) ? "alpha" : "solid";
}

/** 「色」の欄の行（15 個を COLOR_VARS の並びで） */
export const COLOR_ROWS: { name: ColorName; label: string; kind: ColorKind }[] = COLOR_VARS.map((name) => ({ name, label: COLOR_LABELS[name], kind: colorKind(name) }));

/** 手で選んだ色のうち、決まりに合うものだけを形をそろえて返す。合わない名前は rejected に返す（オブジェクトでなければ何も当てない） */
export function manualColorVars(manual: unknown): { vars: Record<string, string>; rejected: string[] } {
  const vars: Record<string, string> = {};
  const rejected: string[] = [];
  if (!manual || typeof manual !== "object" || Array.isArray(manual)) return { vars, rejected };
  for (const [k, v] of Object.entries(manual as Record<string, unknown>)) {
    const ok = validColorToken(k, v);
    if (ok === null) rejected.push(k);
    else vars[k] = ok;
  }
  return { vars, rejected };
}

/**
 * 今のテーマに当てる変数：スキンの値（tokens.ts の skinCssVars）の上に、手の色を重ねる。
 * rejected はスキンの値で確かめに通らなかった名前（呼び出し側はスキンを既定に戻す）、manualRejected は手の色で通らなかった名前（その色だけ当てない）
 */
export function layeredCssVars(
  a: AppearanceLike | null | undefined,
  theme: string,
  manual: unknown,
): { vars: Record<string, string>; rejected: string[]; manualRejected: string[] } {
  const s = skinCssVars(a, theme);
  const m = manualColorVars(manual);
  return { vars: { ...s.vars, ...m.vars }, rejected: s.rejected, manualRejected: m.rejected };
}

/** 当てているスキンの、そのテーマの色だけ（確かめに通るもの） */
export function skinThemeColors(a: AppearanceLike | null | undefined, theme: string): Record<string, string> {
  const t = a ? (theme === "light" ? a.light : a.dark) : null;
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(t?.tokens ?? {})) {
    const ok = validColorToken(k, v);
    if (ok !== null) out[k] = ok;
  }
  return out;
}

export type ColorSource = "theme" | "skin" | "manual";

/** いま当たっている色と、どこから来たか（テーマの既定 < スキン < 手の色） */
export function currentColors(
  defaults: Record<string, string>,
  skin: Record<string, string>,
  manual: Record<string, string>,
): Record<string, { value: string; source: ColorSource }> {
  const m = manualColorVars(manual).vars;
  const out: Record<string, { value: string; source: ColorSource }> = {};
  for (const n of COLOR_VARS) {
    if (n in m) out[n] = { value: m[n], source: "manual" };
    else if (n in skin) out[n] = { value: skin[n], source: "skin" };
    else out[n] = { value: (defaults[n] ?? "").trim(), source: "theme" };
  }
  return out;
}

const hexOf = (rgb: number[]) => "#" + rgb.map(hex2).join("");

/** `#rrggbbaa`・`#rgba` → `rgba(r, g, b, a)`（a は小数3桁）。濃さが 1 なら `#rrggbb`。ほかの形は null */
function hexWithAlpha(v: string): string | null {
  const m = /^#([0-9a-fA-F]{4}|[0-9a-fA-F]{8})$/.exec(v.trim());
  if (!m) return null;
  const h = (m[1].length === 4 ? [...m[1]].map((c) => c + c).join("") : m[1]).toLowerCase();
  const [r, g, b, a255] = [0, 2, 4, 6].map((i) => parseInt(h.slice(i, i + 2), 16));
  const a = Math.round((a255 / 255) * 1000) / 1000;
  return a === 1 ? "#" + h.slice(0, 6) : `rgba(${r}, ${g}, ${b}, ${a})`;
}

/**
 * 画面で今当たっている色（テーマの既定）を読む（v3.10.0 直し3回目）。ビルドのときに CSS を小さくする道具が
 * `rgba(255, 255, 255, 0.07)` を `#ffffff12` のような8桁の16進に書き換えるので、受け付ける形（validColorToken）に加えて
 * `#rrggbbaa`・`#rgba` を読み、今までの形（`rgba(r, g, b, a)`。a は小数3桁。濃さ 1 なら `#rrggbb`）にそろえる。
 * 読むだけで、手の色・スキンの値として受け付ける形（validColorToken と Rust の確かめ）は広げない。読めなければ null
 */
export function readCurrentColor(name: string, v: unknown): string | null {
  if (typeof v !== "string") return null;
  const ok = validColorToken(name, v);
  if (ok !== null) return ok;
  if (!(COLOR_VARS as readonly string[]).includes(name) || name.endsWith("-rgb")) return null;
  return hexWithAlpha(v);
}

/** テーマの既定の色の全部を読む（readCurrentColor）。読めない名前は unreadable に返す（画面に出して知らせる） */
export function readThemeColors(raw: Record<string, string>): { colors: Record<string, string>; unreadable: string[] } {
  const colors: Record<string, string> = {};
  const unreadable: string[] = [];
  for (const n of COLOR_VARS) {
    const v = readCurrentColor(n, raw[n]);
    if (v === null) unreadable.push(n);
    else colors[n] = v;
  }
  return { colors, unreadable };
}

/** 色の値を、色の選び窓（#rrggbb）と濃さ（0〜1）に分ける。読めない値は null */
export function splitColor(value: string): { hex: string; alpha: number } | null {
  const rgb = validRgbTriple(value);
  if (rgb !== null) return { hex: hexOf(rgb.split(",").map((x) => Number(x.trim()))), alpha: 1 };
  // 今当たっている値はビルドした CSS から来るので、#rrggbbaa・#rgba も読む（readCurrentColor）
  const c = validColor(value) ?? hexWithAlpha(value);
  if (c === null) return null;
  if (c.startsWith("#")) return { hex: c, alpha: 1 };
  const m = /^rgba\((\d+), (\d+), (\d+), ([\d.]+)\)$/.exec(c);
  return m ? { hex: hexOf([+m[1], +m[2], +m[3]]), alpha: Number(m[4]) } : null;
}

/** 選んだ色と濃さから、保存する値を作る（Rust の確かめに通る形）。濃さは小数3桁まで（色の確かめと同じ。色だけ選んだときに今の濃さ 0.045 などを変えない。つまみは 0.1% きざみ）。色だけの行では濃さを使わない */
export function pickedColor(kind: ColorKind, hex: string, alpha = 1): string | null {
  const c = validColor(hex);
  if (c === null || !c.startsWith("#")) return null;
  const n = parseInt(c.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  if (kind === "solid") return c;
  if (kind === "rgb") return `${r}, ${g}, ${b}`;
  const a = Math.round(Math.min(1, Math.max(0, Number.isFinite(alpha) ? alpha : 1)) * 1000) / 1000;
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

/** 警告の組み合わせの名前に出る変数の名前（--c-primary など）を、画面の色の名前に置き換える */
export function warningLabel(name: string): string {
  return name.replace(/--c-[a-z-]+[a-z]/g, (v) => (v in COLOR_LABELS ? `「${COLOR_LABELS[v as ColorName]}」` : v));
}
