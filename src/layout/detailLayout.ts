// v3.8.0：「詳細」（仕様の「配信モード」）の配置。ほかの画面の部品に依存しない純粋な関数だけを置く
// （scripts\ui-detail.test.mts から Node だけで、窓の幅 480〜2560px・高さ 480〜1440px を確かめるため）。
//
// 仕様（同時再生と配信モード 3.2・6.1）：
// - 詳細は「ミキサーをいつも出しておく状態」。曲一覧の側（.main）は今までどおり。ミキサーは .app の格子に1つ足す
// - 左右：サイドバーを除いた中央の幅に、曲一覧（最低 360px）とミキサー（35%、380〜520px）が並び、かつ窓が縦長（高さ＞幅）でないとき。
//   左に曲一覧、右にミキサー
// - 上下：それ以外。上に曲一覧、下にミキサー。曲一覧は隠さない。ミキサーの高さは、行が全部入る高さか、
//   曲一覧に2行を残す高さの小さいほう。ただし見出しと1行（音量つまみと「…」）はいつも入れる（ミキサーの操作を優先。入らない行はミキサーの中で送る）
// - 流れる歌詞の列：詳細で曲一覧が2行を切るときは自動で隠す（設定は書き換えない。窓を広げたら戻る）
// 幅・高さは全部 CSS（base.css・tokens.css）と同じ値。合っていることは scripts\ui-detail.test.mts で確かめる。
// ほかのファイルを読み込まない（Node のテストから読むため）。曲一覧の行の高さ（density.ts）と再生バーの高さ（playerLayout.ts）は、呼ぶ側が渡す。

/** 骨組み（base.css の .app と tokens.css）。600px 未満の窓では外の余白と間が 6px（base.css の 599px の段） */
export const APP = {
  pad: 10,
  gap: 10,
  padNarrow: 6,
  gapNarrow: 6,
  narrowBelow: 600,
  sidebarW: 236,
  sidebarCollapsedW: 64,
  /** この幅未満の窓では、サイドバーを自動でたたむ（useLayoutSize） */
  sidebarAutoBelow: 1100,
  lyricsRowH: 48,
} as const;

/** 詳細の左右・上下の決まり（仕様 3.2・6.1） */
export const DETAIL = {
  listMin: 360,
  mixerMin: 380,
  mixerMax: 520,
  mixerRatio: 0.35,
} as const;

/**
 * ミキサーの寸法（base.css の .mixer の節と同じ値）。行は左右のときも上下のときも同じ形（6.2）：
 *   上の段（28px）：アイコン・名前・印（全体に従わない・状態・音割れ）・dB の数字・オン/オフ・「…」
 *   下の段（24px）：メーター（左右2本と -60〜0 dB の目盛り）・音量つまみ・%
 */
export const MIXER = {
  border: 2,
  padY: 8,
  padX: 10,
  headH: 32,
  gap: 6,
  rowGap: 4,
  rowPadY: 6,
  rowPadX: 8,
  rowTop: 28,
  rowInnerGap: 4,
  rowBottom: 24,
  /** 出力先が1つも無いときの文の行 */
  emptyH: 40,
  /** 行の中身の最低幅（--mixer-strip-min-w）。ミキサーの最低幅 380 − 枠 2 − 余白 10×2 ≒ 358 より小さい */
  stripMin: 340,
  /** 上の段：オン/オフと「…」（28px ずつ・間 2）、段と列の間 8、要素の間 6、アイコン 20、dB の数字の欄 72、名前の最低幅 48（base.css） */
  buttons: 28 * 2 + 2,
  colGap: 8,
  topGap: 6,
  icon: 20,
  dbW: 72,
  nameMin: 48,
} as const;

/** 1行の高さ（68px） */
export const MIXER_ROW_H = MIXER.rowPadY * 2 + MIXER.rowTop + MIXER.rowInnerGap + MIXER.rowBottom;

/** ミキサーの中身が全部入る高さ（n は出力先の数 0〜3） */
export function mixerContentHeight(n: number): number {
  const rows = n <= 0 ? MIXER.emptyH : n * MIXER_ROW_H + (n - 1) * MIXER.rowGap;
  return MIXER.border + MIXER.padY * 2 + MIXER.headH + MIXER.gap + rows;
}

/** ミキサーの最低の高さ：見出しと1行（出力先が無ければ文の行）。音量つまみと「…」がいつも押せる高さ */
export function mixerMinHeight(n: number): number {
  return mixerContentHeight(Math.min(1, Math.max(0, n)));
}

/**
 * 曲一覧の側の、行より上にあるもの（見積もり）：枠 1×2 ＋ 上の帯 56 ＋ 一覧の見出し 56（余白 14＋10 と、ボタンの 32px）＋ 列の見出し 34。
 * 一覧の見出しは画面と幅で変わる（狭いと折り返す）ので、決まった値で見積もる。行の数はこれで数える
 */
export const LIST_CHROME = 2 + 56 + 56 + 34;

export interface DetailInput {
  winW: number;
  winH: number;
  /** サイドバーをたたんでいるか（設定）。窓が 1100px 未満なら、これにかかわらずたたむ */
  sidebarCollapsed: boolean;
  /** 出力先の数（0〜3） */
  slots: number;
  /** 流れる歌詞の列を出す設定か（歌詞の画面を開いていないときだけ出す。そこは呼ぶ側が決める） */
  lyricsRow: boolean;
  /** 曲一覧の側の幅のときの、曲一覧の1行の高さ（コンパクトなら 36、そうでなければ 60 か 48。density.ts の trackRowHeight） */
  rowHeight: (listW: number) => number;
  /** 再生バーの高さ（3列 80・1段 76。再生バーの幅 barWidth(winW) で playerLayout が決める） */
  playerH: number;
}

export interface DetailLayout {
  side: boolean;
  /** サイドバーを除いた中央の幅（詳細を抜けたあとの曲一覧の側の幅と同じ） */
  centerW: number;
  /** 左右のときのミキサーの幅（上下のときは centerW） */
  mixerW: number;
  /** 曲一覧の側の幅 */
  listW: number;
  /** 上下のときのミキサーの高さ（左右のときは listH と同じ） */
  mixerH: number;
  /** 曲一覧の側の高さ */
  listH: number;
  /** 曲一覧の行の高さと、見える行の数（見積もり） */
  rowH: number;
  listRows: number;
  /** 流れる歌詞の列を自動で隠すか */
  hideLyricsRow: boolean;
  /** ミキサーの中身が入りきらず、ミキサーの中で送るか */
  mixerScrolls: boolean;
  /** 再生バーの高さ */
  playerH: number;
}

/** 窓の幅のときの外の余白と間 */
export function appSpacing(winW: number): { pad: number; gap: number } {
  return winW < APP.narrowBelow ? { pad: APP.padNarrow, gap: APP.gapNarrow } : { pad: APP.pad, gap: APP.gap };
}

/** サイドバーを除いた中央の幅（.main の幅。詳細でないときの曲一覧の側の幅と同じ） */
export function centerWidth(winW: number, sidebarCollapsed: boolean): number {
  const { pad, gap } = appSpacing(winW);
  const side = sidebarCollapsed || winW < APP.sidebarAutoBelow ? APP.sidebarCollapsedW : APP.sidebarW;
  return winW - pad * 2 - side - gap;
}

/** 左右のときのミキサーの幅：中央の幅の 35%、380〜520px（整数に丸める） */
export function mixerWidth(centerW: number): number {
  return Math.round(Math.min(DETAIL.mixerMax, Math.max(DETAIL.mixerMin, centerW * DETAIL.mixerRatio)));
}

/** 再生バーの幅（左右いっぱい。窓 − 外の余白×2） */
export function barWidth(winW: number): number {
  return winW - appSpacing(winW).pad * 2;
}

const rowsIn = (listH: number, rowH: number) => Math.max(0, Math.floor((listH - LIST_CHROME) / rowH));

export function detailLayout(inp: DetailInput): DetailLayout {
  const { pad, gap } = appSpacing(inp.winW);
  const centerW = centerWidth(inp.winW, inp.sidebarCollapsed);
  const sideMixerW = mixerWidth(centerW);
  const sideListW = centerW - gap - sideMixerW;
  const side = sideListW >= DETAIL.listMin && !(inp.winH > inp.winW);
  const playerH = inp.playerH;
  const n = Math.max(0, Math.min(3, inp.slots));
  const listW = side ? sideListW : centerW;
  const rowH = inp.rowHeight(listW);
  const content = mixerContentHeight(n);
  const plan = (lyrics: boolean) => {
    // 再生バーより上の行の高さ（窓 − 外の余白 − 間 − 再生バー − 流れる歌詞の列）
    const area = inp.winH - pad * 2 - gap - playerH - (lyrics ? APP.lyricsRowH : 0);
    if (side) {
      return { listH: area, mixerH: area, rows: rowsIn(area, rowH), scrolls: content > area };
    }
    const keepList = LIST_CHROME + 2 * rowH;
    const mixerH = Math.min(content, Math.max(mixerMinHeight(n), area - gap - keepList));
    const listH = area - gap - mixerH;
    return { listH, mixerH, rows: rowsIn(listH, rowH), scrolls: content > mixerH };
  };
  let hide = false;
  let p = plan(inp.lyricsRow);
  if (inp.lyricsRow && p.rows < 2) {
    hide = true;
    p = plan(false);
  }
  return {
    side,
    centerW,
    mixerW: side ? sideMixerW : centerW,
    listW,
    mixerH: p.mixerH,
    listH: p.listH,
    rowH,
    listRows: p.rows,
    hideLyricsRow: hide,
    mixerScrolls: p.scrolls,
    playerH,
  };
}
