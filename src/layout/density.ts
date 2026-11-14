// v3.4.0：表示の詰め方（曲一覧のコンパクト表示・サイドバーのアイコンの大きさ）。
// ほかに依存しない純粋な関数だけを置く（scripts\ui-density.test.mts から Node だけで確かめるため）。
//
// 値の名前と範囲は Rust の settings\store.rs（COMPACT_LIST_MODES・SIDEBAR_ICON_PX_*）と同じ。
// Rust が保存・読み込みのときに整えるが、画面でも同じ決まりで読む（古い設定・壊れた値でも今までの見た目になるように）。

export interface DensityUiLike {
  compactList?: unknown;
  sidebarIconPx?: unknown;
}

/** 曲一覧のコンパクト表示の選び方（並びは設定画面に出す順） */
export const COMPACT_LIST_MODES = [
  { id: "off", label: "使わない" },
  { id: "always", label: "いつも" },
  { id: "narrow", label: "ウィンドウが狭いときだけ" },
] as const;

export type CompactListMode = (typeof COMPACT_LIST_MODES)[number]["id"];

/**
 * 「ウィンドウが狭いときだけ」の境目：ウィンドウの幅がこれ未満のときコンパクトにする。
 * useLayoutSize の「3. ジャケットを縮める」（720px 未満で 44 → 36px。3.6.0 までは 48 → 36px）と同じ境目。
 * 今までの作りが「ジャケットが大きすぎる」と決めている幅なので、そこから先はジャケットを縮める代わりに消す
 * （新しい境目を足さない。サイドバー 1100・ジャンル 900・アルバム 820 の列が先に消えたあと）
 */
export const COMPACT_NARROW_BELOW = 720;

/** コンパクト表示の行の高さ（px）。行の中で一番高い部品は、お気に入り・ごみ箱のボタン（コンパクト表示では 28px。v3.7.0 から通常の行では 32px の面）。上下に 4px ずつ残す */
export const COMPACT_ROW_H = 36;

/** 設定の値を読む。知らない値・値が無い（3.3.0 までの設定）は「使わない」 */
export function compactListMode(ui: DensityUiLike | null | undefined): CompactListMode {
  const v = ui?.compactList;
  return COMPACT_LIST_MODES.some((m) => m.id === v) ? (v as CompactListMode) : "off";
}

/** 今、曲一覧をコンパクトに出すか（width はウィンドウの幅。useLayoutSize の width） */
export function compactListOn(ui: DensityUiLike | null | undefined, width: number): boolean {
  const m = compactListMode(ui);
  if (m === "always") return true;
  if (m === "narrow") return width < COMPACT_NARROW_BELOW;
  return false;
}

/** 曲一覧の行の高さ：コンパクトなら COMPACT_ROW_H、そうでなければ通常の高さ（useLayoutSize の rowH：60（v3.7.0。3.6.0 までは 64）、600px 未満は 48） */
export function trackRowHeight(compact: boolean, normalRowH: number): number {
  return compact ? COMPACT_ROW_H : normalRowH;
}

/** サイドバーのアイコンの大きさ（px）。既定は 3.3.0 までの 20px */
export const SIDEBAR_ICON_PX = { min: 14, max: 32, default: 20 } as const;

/** 設定の値を読む。数でなければ既定、範囲の外は端へ、小数は四捨五入（Rust の clamp_icon_px と同じ） */
export function sidebarIconPx(ui: DensityUiLike | null | undefined): number {
  const v = ui?.sidebarIconPx;
  if (typeof v !== "number" || !Number.isFinite(v)) return SIDEBAR_ICON_PX.default;
  return Math.min(SIDEBAR_ICON_PX.max, Math.max(SIDEBAR_ICON_PX.min, Math.round(v)));
}
