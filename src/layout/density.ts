// v3.4.0：表示の詰め方（曲一覧のコンパクト表示・サイドバーのアイコンの大きさ）。
// ほかに依存しない純粋な関数だけを置く（scripts\ui-density.test.mts から Node だけで確かめるため）。
//
// 値の名前と範囲は Rust の settings\store.rs（COMPACT_LIST_MODES・SIDEBAR_ICON_PX_*）と同じ。
// Rust が保存・読み込みのときに整えるが、画面でも同じ決まりで読む（古い設定・壊れた値でも今までの見た目になるように）。
//
// v3.8.0：曲一覧の列・ジャケット・行の高さ・自動のコンパクトは、ウィンドウの幅ではなく「曲一覧の側の幅」で決める
// （AppShell が測る .main の幅。配信モードで曲一覧とミキサーを左右に並べたとき、曲一覧だけが狭くなるため）。
// コンパクト表示は「使わない／いつも」＋「窓が狭いときは自動でコンパクトにする」にした（3.7.0 の "narrow" は読み替える）。

export interface DensityUiLike {
  compactList?: unknown;
  compactAuto?: unknown;
  sidebarIconPx?: unknown;
}

/** 曲一覧のコンパクト表示の選び方（並びは設定画面に出す順） */
export const COMPACT_LIST_MODES = [
  { id: "off", label: "使わない" },
  { id: "always", label: "いつも" },
] as const;

export type CompactListMode = (typeof COMPACT_LIST_MODES)[number]["id"];

/** 3.4.0〜3.7.0 の「ウィンドウが狭いときだけ」。読むときだけ受ける（Rust の COMPACT_LIST_LEGACY_NARROW と同じ） */
export const COMPACT_LIST_LEGACY_NARROW = "narrow";

/**
 * 曲一覧の側の幅（.main の幅。px）の境目。この値「未満」のとき、それぞれの段に入る。
 *
 * 3.7.0 まではウィンドウの幅で、ジャンル 900・アルバム 820・ジャケット 720（と自動のコンパクト）・行の高さ 600 だった。
 * ウィンドウの幅が 1100px 未満ではサイドバーが自動でたたまれる（64px）ので、曲一覧の側の幅は
 *   ウィンドウの幅 − 外の余白 10×2 − サイドバー 64 − 間 10 = ウィンドウの幅 − 94
 * になる（600px 以上。base.css の .app）。境目はこの式で置き換えた：900 → 806・820 → 726・720 → 626。
 * 1100px 以上でサイドバーを広げていても（曲一覧の側は 834px 以上）、どの段にも入らないので 3.7.0 と同じ。
 *
 * 行の高さ（600）だけは、ウィンドウの幅 600px 未満で外の余白と間が 6px に縮む（曲一覧の側が 11px 広がる。base.css の 599px の段）ため、
 * 同じ見た目にする値が1つに決まらない（599px で 517px、600px で 506px）。600px 未満の窓で今と同じになる 518 を採った。
 * そのため、窓の幅 600〜611px だけは行の高さが 3.7.0 の 60px でなく 48px になる（scripts\ui-density.test.mts で範囲を確かめる）
 */
export const LIST_BELOW = { genre: 806, album: 726, art: 626, row: 518 } as const;

/** 自動のコンパクトの境目（曲一覧の側の幅）。3.7.0 の「狭いときだけ」（ウィンドウ 720px 未満）と同じ、ジャケットが縮む段 */
export const COMPACT_AUTO_BELOW = LIST_BELOW.art;

/** コンパクト表示の行の高さ（px）。行の中で一番高い部品は、お気に入り・ごみ箱のボタン（コンパクト表示では 28px。v3.7.0 から通常の行では 32px の面）。上下に 4px ずつ残す */
export const COMPACT_ROW_H = 36;

/** 曲一覧の大きさ（曲一覧の側の幅で決める）。artSize は CSS の --row-art、rowH は --row-h（TrackTable が曲一覧の要素に入れて渡す） */
export interface ListLayout {
  /** 曲一覧の側の幅（px） */
  width: number;
  hideGenre: boolean;
  hideAlbum: boolean;
  /** ジャケット：44px、狭いと 36px（v3.7.0 の値） */
  artSize: number;
  /** 通常の行の高さ：60px、狭いと 48px（仮想スクロールの1行の高さにもなる） */
  rowH: number;
  /** 行の左右の余白を詰める（16 → 10px・列の間 12 → 10px。行の高さが 48px になる段と同じ） */
  tight: boolean;
  /** 上の帯のファイル・フォルダのボタンを、アイコンだけにする（ジャケットが縮む段と同じ） */
  topbarNarrow: boolean;
}

export function listLayoutFor(w: number): ListLayout {
  return {
    width: w,
    hideGenre: w < LIST_BELOW.genre,
    hideAlbum: w < LIST_BELOW.album,
    artSize: w < LIST_BELOW.art ? 36 : 44,
    rowH: w < LIST_BELOW.row ? 48 : 60,
    tight: w < LIST_BELOW.row,
    topbarNarrow: w < LIST_BELOW.art,
  };
}

/** 設定の値を読む。知らない値・値が無い（3.3.0 までの設定）・3.7.0 までの "narrow" は「使わない」 */
export function compactListMode(ui: DensityUiLike | null | undefined): CompactListMode {
  const v = ui?.compactList;
  return COMPACT_LIST_MODES.some((m) => m.id === v) ? (v as CompactListMode) : "off";
}

/** 「窓が狭いときは自動でコンパクトにする」。値が無ければオフ。3.7.0 までの "narrow" はオン（Rust の sanitize と同じ読み替え） */
export function compactAutoOn(ui: DensityUiLike | null | undefined): boolean {
  return ui?.compactAuto === true || ui?.compactList === COMPACT_LIST_LEGACY_NARROW;
}

/** 今、曲一覧をコンパクトに出すか（listWidth は曲一覧の側の幅。useListLayout の width） */
export function compactListOn(ui: DensityUiLike | null | undefined, listWidth: number): boolean {
  if (compactListMode(ui) === "always") return true;
  return compactAutoOn(ui) && listWidth < COMPACT_AUTO_BELOW;
}

/** 自動でコンパクトになっているところか（設定は「使わない」のまま、幅で切り替わっている） */
export function compactIsAuto(ui: DensityUiLike | null | undefined, listWidth: number): boolean {
  return compactListMode(ui) !== "always" && compactListOn(ui, listWidth);
}

/** 曲一覧の行の高さ：コンパクトなら COMPACT_ROW_H、そうでなければ通常の高さ（listLayoutFor の rowH：60（v3.7.0。3.6.0 までは 64）、狭いと 48） */
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

// ---------- 上の帯のモード切替（v3.8.0） ----------
// 仕様（同時再生と配信モード 5章）：
// - 押したほうがそのまま設定になる（設定画面のコンパクト表示を選ぶのと同じ値）
// - 自動でコンパクトになっているときは、上の帯もコンパクトを出す（窓を広げ直したら通常へ戻る。設定は書き換えない）
// - 自動でコンパクトになっているときに「通常」を押したら、その場で通常に戻し、「窓が狭いときは自動でコンパクトにする」をオフにする
// - 自動でコンパクトになっているときに「コンパクト」を押したら、設定は「いつも」になる
// 「配信」（段2）と「ミニ」（あとで）は、VIEW_MODES に足して、pressViewMode・currentViewMode に分岐を足す作り
// （配信モード中の通常／コンパクトの扱い・入る前の表示の保存は段2）。Node のテストから読むので、このファイルに置く（ほかのファイルを読み込まない）。

/** 並びは上の帯に出す順（label は文字・吹き出し・読み上げの名前のどれにも使う） */
export const VIEW_MODES = [
  { id: "normal", label: "通常" },
  { id: "compact", label: "コンパクト" },
] as const;

export type ViewMode = (typeof VIEW_MODES)[number]["id"];

/** 上の帯で選ばれて見えるモード（自動でコンパクトになっているときもコンパクト） */
export function currentViewMode(ui: DensityUiLike | null | undefined, listWidth: number): ViewMode {
  return compactListOn(ui, listWidth) ? "compact" : "normal";
}

/** 押したときの設定の値（ui の compactList・compactAuto だけ。ほかの値は呼ぶ側がそのまま残す） */
export function pressViewMode(
  ui: DensityUiLike | null | undefined,
  listWidth: number,
  mode: ViewMode,
): { compactList: "off" | "always"; compactAuto: boolean } {
  const auto = compactAutoOn(ui);
  if (mode === "compact") return { compactList: "always", compactAuto: auto };
  // 通常：「使わない」にする。それでも自動でコンパクトのままになる幅なら（自動でコンパクトになっているとき）、自動もオフにする
  const next = { compactList: "off" as const, compactAuto: auto };
  if (compactListOn(next, listWidth)) next.compactAuto = false;
  return next;
}

/**
 * 上の帯のモード切替に文字を出すか（出せなければアイコンだけ。マウスを乗せる・キーボードで選ぶと文字の吹き出し、読み上げには名前）。
 * 上の帯の幅（.main の幅と同じ）に、ほかの部品と、文字つきの切替を並べても、検索欄に TOPBAR.searchKeep（200px）が残るときだけ文字を出す。
 * ファイル・フォルダのボタンがアイコンだけになる幅（topbarNarrow）では、切替もアイコンだけ（先に切替の文字を外す。幅を狭めていく途中で、
 * 一度消えた文字がまた出る、を起こさないため）。
 *
 * 幅は base.css の値から出す（スキンは文字を ±2px まで大きくできるので、文字は 16px で数える。日本語は1字が文字の大きさと同じ幅）：
 * - 上の帯：一覧の側の枠 1×2・左右の余白 14×2・検索欄との間 10
 * - 右端の並び（.topbar-actions。間 6）：ファイル・フォルダ（.btn）は余白 12×2・枠 1×2・アイコン 16・間 6・文字4字 → 112 ずつ
 * - 切替（.seg.view-mode）：枠 1×2、1つのボタンは余白 10×2・アイコン 16・間 6・文字（通常 2字・コンパクト 5字）。アイコンだけなら 36 ずつ
 */
export const TOPBAR = {
  frame: 2 + 28,
  gap: 10,
  actionsGap: 6,
  fileFolderLabeled: 112 + 6 + 112,
  switchLabeled: 2 + (20 + 16 + 6 + 2 * 16) + (20 + 16 + 6 + 5 * 16),
  switchIcons: 2 + 2 * 36,
  searchKeep: 200,
} as const;

export function viewModeLabelsFit(topbarWidth: number, topbarNarrow: boolean): boolean {
  if (topbarNarrow) return false;
  const inner = topbarWidth - TOPBAR.frame;
  const actions = TOPBAR.fileFolderLabeled + TOPBAR.actionsGap + TOPBAR.switchLabeled;
  return inner - TOPBAR.gap - actions >= TOPBAR.searchKeep;
}
