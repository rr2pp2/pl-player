// テーマの色の、文字と背景の濃さの差（WCAG 2.x のコントラスト比）を計算する（v3.1.2）。
// 走らせ方（プロジェクトのフォルダで。依存は要らない）：
//   node scripts/theme-contrast.mts            … Light と Dark の表を Markdown で出す（docs\テーマの色の根拠.md の表はこれで作った）
//   node --test scripts/theme-contrast.test.mts … 基準（本文 4.5:1・大きい文字と線 3:1）を満たすかを確かめる
//
// 重ね方は画面と同じ（src\theme\base.css）：背景画像 → 覆い rgba(--c-overlay-rgb, --bg-dim) → パネル rgba(--c-panel-rgb, --ui-alpha) → 文字。
// 半透明の色は、ブラウザと同じく sRGB の値のまま混ぜる（α合成）。背景画像は、いちばん明るい白（#ffffff）といちばん暗い黒（#000000）の両方で計算する。
// ぼかし（--bg-blur・backdrop-filter）は一様な色を変えないので、計算に入れない。
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export type RGB = [number, number, number];
type RGBA = { rgb: RGB; a: number };

/** テーマの CSS から `--c-*` の値を読む（`[data-theme="..."] { ... }` の中だけ） */
export function readTheme(path: string): Record<string, string> {
  const css = readFileSync(path, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const out: Record<string, string> = {};
  for (const m of css.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}

export function hex(s: string): RGB {
  const m = /^#([0-9a-f]{6})$/i.exec(s.trim());
  if (!m) throw new Error(`not #rrggbb: ${s}`);
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** `rgba(r, g, b, a)` か `#rrggbb` か `r, g, b`（--c-*-rgb の形） */
export function color(s: string): RGBA {
  const t = s.trim();
  if (t.startsWith("#")) return { rgb: hex(t), a: 1 };
  const m = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/.exec(t);
  if (m) return { rgb: [+m[1], +m[2], +m[3]], a: m[4] === undefined ? 1 : +m[4] };
  const p = t.split(",").map((x) => +x.trim());
  if (p.length === 3 && p.every((x) => Number.isFinite(x))) return { rgb: [p[0], p[1], p[2]], a: 1 };
  throw new Error(`unknown color: ${s}`);
}

/** 下の色に、色 `top` を不透明度 `a` で重ねた色（sRGB のまま混ぜる） */
export function over(bottom: RGB, top: RGB, a: number): RGB {
  return [0, 1, 2].map((i) => top[i] * a + bottom[i] * (1 - a)) as RGB;
}

/** WCAG 2.x の相対輝度 */
export function luminance(c: RGB): number {
  const lin = c.map((v) => {
    const s = v / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
}

export function ratio(a: RGB, b: RGB): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

export function toHex(c: RGB): string {
  return "#" + c.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
}

/** 背景の場面 */
export type Scene = { name: string; image: RGB | null; dim: number; ui: number };

/** 既定の設定（src\stores\settings.ts の背景の暗さ 0.45・UI の不透明度 0.7） */
export const DEFAULT_DIM = 0.45;
export const DEFAULT_UI = 0.7;

export function scenes(dim = DEFAULT_DIM, ui = DEFAULT_UI): Scene[] {
  return [
    // 背景画像なしのときは、覆いの暗さは 0（BackgroundLayer.tsx）で、下は --c-bg
    { name: "背景画像なし", image: null, dim: 0, ui },
    { name: "明るい背景画像（白）", image: [255, 255, 255], dim, ui },
    { name: "暗い背景画像（黒）", image: [0, 0, 0], dim, ui },
  ];
}

/** 判定する組み合わせ。min は基準（本文 4.5・大きい文字と線 3） */
export type Check = { name: string; fg: RGB; bg: RGB; min: number };

/**
 * 1つの場面で、画面の文字・線と、その下の色の組み合わせを作る。
 * どこで何に使っているかは base.css（主なもの）：
 * - 文字：--c-primary（曲名など）・--c-secondary（アーティスト・見出し・説明。12px も）・--c-accent（再生中の曲名・選んだ項目の印・歌詞の今の行）・
 *   --c-danger（削除のボタン・エラーの文）・--c-ok（できたの文）
 * - 下の色：パネル・選んだ行（--c-selected）・指を乗せた行（--c-hover）・入力欄（--c-input）・メニュー（パネルの色を 0.92〜0.98）
 * - 塗りの上の文字：--c-accent の上の --c-accent-contrast（主なボタン）・--c-primary の上の --c-bg（再生ボタンの絵）
 * - 線（3:1）：--c-border（区切り・シークバーの残り。Dark では飾りの線で、基準の外）・--c-accent（フォーカスの枠・シークバーの済んだところ）・
 *   --c-outline（v3.7.0：ボタン・入力欄・行のボタンの輪郭）
 * - アイコン（3:1）：--c-favorite（v3.7.0：お気に入りのオンの星。オンのボタンの面 --c-selected の上）
 */
export function checks(t: Record<string, string>, s: Scene): Check[] {
  const bgColor = color(t["--c-bg"]).rgb;
  const base = s.image ?? bgColor;
  const overlay = over(base, color(t["--c-overlay-rgb"]).rgb, s.dim);
  const panelRgb = color(t["--c-panel-rgb"]).rgb;
  const panel = over(overlay, panelRgb, s.ui);
  const menu = over(overlay, panelRgb, 0.92);
  const on = (bottom: RGB, v: string) => {
    const c = color(t[v]);
    return over(bottom, c.rgb, c.a);
  };
  const selected = on(panel, "--c-selected");
  const hover = on(panel, "--c-hover");
  const input = on(panel, "--c-input");
  const text = (v: string) => color(t[v]).rgb;
  const out: Check[] = [];
  const surfaces: [string, RGB][] = [
    ["パネル", panel],
    ["選んだ行", selected],
    ["指を乗せた行", hover],
    ["入力欄", input],
    ["メニュー（0.92）", menu],
  ];
  for (const [label, bottom] of surfaces) {
    out.push({ name: `--c-primary／${label}`, fg: text("--c-primary"), bg: bottom, min: 4.5 });
    out.push({ name: `--c-secondary／${label}`, fg: text("--c-secondary"), bg: bottom, min: 4.5 });
    out.push({ name: `--c-accent（文字）／${label}`, fg: text("--c-accent"), bg: bottom, min: 4.5 });
  }
  out.push({ name: "--c-danger／パネル", fg: text("--c-danger"), bg: panel, min: 4.5 });
  out.push({ name: "--c-danger／メニュー（0.92）", fg: text("--c-danger"), bg: menu, min: 4.5 });
  out.push({ name: "--c-ok／パネル", fg: text("--c-ok"), bg: panel, min: 4.5 });
  out.push({ name: "--c-accent-contrast／--c-accent", fg: text("--c-accent-contrast"), bg: text("--c-accent"), min: 4.5 });
  out.push({ name: "--c-bg／--c-primary（再生ボタン）", fg: bgColor, bg: text("--c-primary"), min: 4.5 });
  out.push({ name: "線 --c-border／パネル", fg: on(panel, "--c-border"), bg: panel, min: 3 });
  out.push({ name: "線 --c-border／入力欄の中", fg: on(panel, "--c-border"), bg: input, min: 3 });
  out.push({ name: "線 --c-accent（フォーカス・シークバー）／パネル", fg: text("--c-accent"), bg: panel, min: 3 });
  // v3.7.0：操作部の輪郭（ボタン・入力欄・行のボタン・設定のタブ）。輪郭は部品の面（--c-input）の上に描かれ、外はパネル
  out.push({ name: "線 --c-outline（操作部の輪郭）／パネル", fg: on(input, "--c-outline"), bg: panel, min: 3 });
  out.push({ name: "線 --c-outline（操作部の輪郭）／入力欄の中", fg: on(input, "--c-outline"), bg: input, min: 3 });
  // v3.7.0：お気に入りのオンの星（アイコン 3:1）。オンのボタンの面（--c-selected）の上。行が選ばれている・指を乗せているときは、その上に重なる
  const favOn = (row: RGB) => on(row, "--c-selected");
  out.push({ name: "--c-favorite（星）／オンのボタンの面", fg: text("--c-favorite"), bg: favOn(panel), min: 3 });
  out.push({ name: "--c-favorite（星）／選んだ行のオンのボタンの面", fg: text("--c-favorite"), bg: favOn(selected), min: 3 });
  out.push({ name: "--c-favorite（星）／指を乗せた行のオンのボタンの面", fg: text("--c-favorite"), bg: favOn(hover), min: 3 });
  // v3.8.0（同時再生）：メーターの帯（図 3:1）。新しい色の変数は足さず、緑 --c-ok・黄 --c-favorite・赤 --c-danger を使い回す。
  // 帯は溝（--c-border の 1px の枠）の中に描き、中はその下の面の色のまま：ミキサー（パネル。行には面の色を重ねない）と、出力のプルダウン（メニューの面）。
  // 状態の文字（切断中・開けません・重複・音割れ）は --c-danger の文字で、上の「--c-danger／パネル」「--c-danger／メニュー」で確かめている
  // （行に --c-hover を重ねると、Dark の明るい背景画像で 4.17 になり 4.5 に届かなかったので、行には面の色を付けない）
  for (const [label, bottom] of [
    ["パネル", panel],
    ["メニュー（0.92）", menu],
  ] as const) {
    out.push({ name: `メーター 緑の帯 --c-ok／${label}`, fg: text("--c-ok"), bg: bottom, min: 3 });
    out.push({ name: `メーター 黄の帯 --c-favorite／${label}`, fg: text("--c-favorite"), bg: bottom, min: 3 });
    out.push({ name: `メーター 赤の帯 --c-danger／${label}`, fg: text("--c-danger"), bg: bottom, min: 3 });
  }
  // 直前の最大の線（--c-primary の 2px）には、パネルの色（不透明。rgb(--c-panel-rgb)）の 1px の縁がある（base.css の .meter-hold）。
  // 溝の上では線と溝の中（パネル）、帯の上では縁と帯の色、線と縁、の組み合わせで見分ける
  const edge = panelRgb;
  out.push({ name: "メーター 最大の線 --c-primary／溝の中（パネル）", fg: text("--c-primary"), bg: panel, min: 3 });
  out.push({ name: "メーター 最大の線 --c-primary／線の縁（パネルの色）", fg: text("--c-primary"), bg: edge, min: 3 });
  out.push({ name: "メーター 最大の線の縁／緑の帯 --c-ok", fg: edge, bg: text("--c-ok"), min: 3 });
  out.push({ name: "メーター 最大の線の縁／黄の帯 --c-favorite", fg: edge, bg: text("--c-favorite"), min: 3 });
  out.push({ name: "メーター 最大の線の縁／赤の帯 --c-danger", fg: edge, bg: text("--c-danger"), min: 3 });
  // 出力ボタンの状態（切断中など・「!」）は --c-primary の太字（ボタンの面＝入力欄の上。上の「--c-primary／入力欄」）。
  // 危険の色はボタンの縁だけに使う（--c-danger の文字はボタンの面の上で、Dark の明るい背景画像だと 4.17 で 4.5 に届かなかった）
  out.push({ name: "線 --c-danger（出力ボタンの縁）／パネル", fg: text("--c-danger"), bg: panel, min: 3 });
  // 鳴らせない出力のアイコンの角の「!」（--c-primary の太字。地はパネルの色・不透明。base.css の .output-icon-mark）
  out.push({ name: "出力ボタンの「!」 --c-primary／印の地（パネルの色）", fg: text("--c-primary"), bg: panelRgb, min: 4.5 });
  // v3.8.0：遅れを測る小窓（.modal：パネルの色を 0.97。下は .modal-backdrop の黒 0.45 を重ねた画面）。
  // 文字は断り・結果（--c-primary）・説明（--c-secondary）・「測れませんでした」と押せない理由（--c-danger の太字。文でも伝える）。
  // 小窓の下がいちばん透ける形（覆いだけ）で計算する
  const modal = over(over(overlay, [0, 0, 0], 0.45), panelRgb, 0.97);
  out.push({ name: "--c-primary／小窓（0.97）", fg: text("--c-primary"), bg: modal, min: 4.5 });
  out.push({ name: "--c-secondary／小窓（0.97）", fg: text("--c-secondary"), bg: modal, min: 4.5 });
  out.push({ name: "--c-danger／小窓（0.97）", fg: text("--c-danger"), bg: modal, min: 4.5 });
  return out;
}

/** すべての場面のすべての組み合わせ */
export function evaluate(themePath: string, dim = DEFAULT_DIM, ui = DEFAULT_UI) {
  const t = readTheme(themePath);
  return scenes(dim, ui).flatMap((s) => checks(t, s).map((c) => ({ scene: s.name, ...c, ratio: ratio(c.fg, c.bg) })));
}

/** 背景の暗さごとに、全部の組み合わせが基準を満たす最小の「UI の不透明度」（0.30〜1.00、0.01 刻み）。満たせなければ null */
export function minUiOpacity(themePath: string, dim: number): number | null {
  for (let i = 30; i <= 100; i++) {
    const ui = i / 100;
    if (evaluate(themePath, dim, ui).every((r) => r.ratio >= r.min)) return ui;
  }
  return null;
}

// 直接走らせたときは Markdown の表を出す（Light、続けて Dark。v3.7.0 で Dark も出すようにした）
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  for (const theme of ["light", "dark"]) {
    const path = fileURLToPath(new URL(`../src/theme/${theme}.css`, import.meta.url));
    const rows = evaluate(path);
    console.log(`## ${theme}`);
    console.log("");
    console.log("| 場面 | 組み合わせ | 文字・線 | 下の色 | 比 | 基準 |");
    console.log("|---|---|---|---|---|---|");
    for (const r of rows) {
      console.log(`| ${r.scene} | ${r.name} | ${toHex(r.fg)} | ${toHex(r.bg)} | ${r.ratio.toFixed(2)} | ${r.min} |`);
    }
    console.log("");
    console.log("| 背景の暗さ | 基準を満たす UI の不透明度（最小） |");
    console.log("|---|---|");
    for (const dim of [0, 0.15, 0.3, 0.45, 0.6, 0.75, 0.95]) {
      const m = minUiOpacity(path, dim);
      console.log(`| ${Math.round(dim * 100)}% | ${m === null ? "満たせない" : Math.round(m * 100) + "%"} |`);
    }
    console.log("");
  }
}
