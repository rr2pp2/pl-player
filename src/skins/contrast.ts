// 着せ替え（v3.2.0）：スキンの色の読みやすさ（WCAG 2.x のコントラスト比）。取り込みの確認画面で警告に使う。
// 計算は scripts\theme-contrast.mts（開発用。配るアプリには入らない）と同じ。あちらは CSS のファイルを読むが、
// こちらは変数の値（既定のテーマの値にスキンの値を重ねたもの）を受け取る。重ね方は画面と同じ（src\theme\base.css）：
// 背景画像 → 覆い rgba(--c-overlay-rgb, 背景の暗さ) → パネル rgba(--c-panel-rgb, UI の不透明度) → 文字。
// 警告は目安で、保証ではない（背景画像と設定の組み合わせは無数にある）。警告が出ても、使う人が選べば取り込める。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

export type RGB = [number, number, number];
type RGBA = { rgb: RGB; a: number };

export function hex(s: string): RGB {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s.trim());
  if (!m) throw new Error(`not a hex color: ${s}`);
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  const n = parseInt(h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** `rgba(r, g, b, a)`・`rgb(r, g, b)`・`#rrggbb`・`#rgb`・`r, g, b`（--c-*-rgb の形） */
export function color(s: string): RGBA {
  const t = s.trim();
  if (t.startsWith("#")) return { rgb: hex(t), a: 1 };
  const m = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/.exec(t);
  if (m) return { rgb: [+m[1], +m[2], +m[3]], a: m[4] === undefined ? 1 : +m[4] };
  const p = t.split(",").map((x) => +x.trim());
  if (p.length === 3 && p.every((x) => Number.isFinite(x))) return { rgb: [p[0], p[1], p[2]], a: 1 };
  throw new Error(`unknown color: ${s}`);
}

export function over(bottom: RGB, top: RGB, a: number): RGB {
  return [0, 1, 2].map((i) => top[i] * a + bottom[i] * (1 - a)) as RGB;
}

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

export type Scene = { name: string; image: RGB | null; dim: number; ui: number };

/** 既定の設定（背景の暗さ 45%・UI の不透明度 70%） */
export const DEFAULT_DIM = 0.45;
export const DEFAULT_UI = 0.7;

export function scenes(dim = DEFAULT_DIM, ui = DEFAULT_UI): Scene[] {
  return [
    { name: "背景画像なし", image: null, dim: 0, ui },
    { name: "明るい背景画像（白）", image: [255, 255, 255], dim, ui },
    { name: "暗い背景画像（黒）", image: [0, 0, 0], dim, ui },
  ];
}

export type Check = { name: string; fg: RGB; bg: RGB; min: number };

/** 1つの場面の組み合わせ（scripts\theme-contrast.mts の checks と同じ 44 通り。v3.7.0 で輪郭2・お気に入りの星3、v3.8.0 段2-2 でメーターの帯6・最大の線5・出力ボタンの縁1と「!」1、段3 で遅れを測る小窓の文字3を足した） */
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
  // v3.8.0 段2-2：メーターの帯（図 3:1）。緑 --c-ok・黄 --c-favorite・赤 --c-danger を、ミキサー（パネル）と出力のプルダウン（メニュー）の面で
  for (const [label, bottom] of [
    ["パネル", panel],
    ["メニュー（0.92）", menu],
  ] as const) {
    out.push({ name: `メーター 緑の帯 --c-ok／${label}`, fg: text("--c-ok"), bg: bottom, min: 3 });
    out.push({ name: `メーター 黄の帯 --c-favorite／${label}`, fg: text("--c-favorite"), bg: bottom, min: 3 });
    out.push({ name: `メーター 赤の帯 --c-danger／${label}`, fg: text("--c-danger"), bg: bottom, min: 3 });
  }
  // 直前の最大の線（--c-primary）とその縁（パネルの色・不透明）。出力ボタンの危険の色の縁
  const edge = panelRgb;
  out.push({ name: "メーター 最大の線 --c-primary／溝の中（パネル）", fg: text("--c-primary"), bg: panel, min: 3 });
  out.push({ name: "メーター 最大の線 --c-primary／線の縁（パネルの色）", fg: text("--c-primary"), bg: edge, min: 3 });
  out.push({ name: "メーター 最大の線の縁／緑の帯 --c-ok", fg: edge, bg: text("--c-ok"), min: 3 });
  out.push({ name: "メーター 最大の線の縁／黄の帯 --c-favorite", fg: edge, bg: text("--c-favorite"), min: 3 });
  out.push({ name: "メーター 最大の線の縁／赤の帯 --c-danger", fg: edge, bg: text("--c-danger"), min: 3 });
  out.push({ name: "線 --c-danger（出力ボタンの縁）／パネル", fg: text("--c-danger"), bg: panel, min: 3 });
  out.push({ name: "出力ボタンの「!」 --c-primary／印の地（パネルの色）", fg: text("--c-primary"), bg: panelRgb, min: 4.5 });
  // v3.8.0 段3：遅れを測る小窓（.modal：パネルの色を 0.97。下は .modal-backdrop の黒 0.45）
  const modal = over(over(overlay, [0, 0, 0], 0.45), panelRgb, 0.97);
  out.push({ name: "--c-primary／小窓（0.97）", fg: text("--c-primary"), bg: modal, min: 4.5 });
  out.push({ name: "--c-secondary／小窓（0.97）", fg: text("--c-secondary"), bg: modal, min: 4.5 });
  out.push({ name: "--c-danger／小窓（0.97）", fg: text("--c-danger"), bg: modal, min: 4.5 });
  return out;
}

export type Result = Check & { scene: string; ratio: number };

/** 既定のテーマの値（base）にスキンの値（skin）を重ねて、全部の場面を計算する */
export function evaluate(base: Record<string, string>, skin: Record<string, string>, dim = DEFAULT_DIM, ui = DEFAULT_UI): Result[] {
  const t = { ...base, ...skin };
  return scenes(dim, ui).flatMap((s) => checks(t, s).map((c) => ({ scene: s.name, ...c, ratio: ratio(c.fg, c.bg) })));
}

/**
 * 基準に届かない組み合わせのうち、スキンで新しく届かなくなったもの・既定より悪くなったものだけ。
 * 既定のテーマで元から届かない組み合わせ（Dark の線など）を、スキンのせいにしない。スキンが色を変えていなければ空
 */
export function contrastWarnings(base: Record<string, string>, skin: Record<string, string>): Result[] {
  if (Object.keys(skin).length === 0) return [];
  const before = evaluate(base, {});
  return evaluate(base, skin).filter((r, i) => !(r.ratio >= r.min) && (before[i].ratio >= before[i].min || r.ratio < before[i].ratio - 0.005));
}
