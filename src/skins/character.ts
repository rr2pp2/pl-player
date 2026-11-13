// 着せ替え（v3.2.0）：立ち絵を出すか・どの絵か・どの大きさか。
// 置き方は「右側に立ち絵用の幅を取る」だけ（曲一覧はその左で終わる）。歌詞画面と、幅が足りないときは出さない。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

/** どのスキンでも、これより狭いウィンドウでは出さない（最小の幅 480px で無理に出さない） */
export const MIN_WINDOW_FOR_CHARACTER = 760;
/** 立ち絵の幅の左右に取るすき間（合わせて） */
export const RAIL_PADDING = 12;
export const SCALE_MIN = 0.75;
export const SCALE_MAX = 1.25;

export interface CharacterLike {
  playing: { path: string; still: string | null };
  stopped: { path: string; still: string | null };
  widthPx: number;
  maxHeightPx: number;
  minWindowWidthPx: number;
}

export interface CharacterInput {
  character: CharacterLike | null | undefined;
  /** 使う人の設定：立ち絵を出す */
  visible: boolean;
  /** 使う人の設定：大きさ 0.75〜1.25 */
  scale: number;
  /** 今の画面（route の view） */
  view: string;
  windowWidth: number;
  /** 再生中か（一時停止・停止・曲なしは停止中の絵） */
  playing: boolean;
  reducedMotion: boolean;
}

export interface CharacterLayout {
  state: "playing" | "stopped";
  src: string;
  widthPx: number;
  maxHeightPx: number;
  /** 立ち絵用に取る幅（曲一覧などはこの分だけ右を空ける） */
  railPx: number;
}

export const clampScale = (s: number): number => (Number.isFinite(s) ? Math.min(SCALE_MAX, Math.max(SCALE_MIN, s)) : 1);

/** 出さないときは null */
export function characterLayout(i: CharacterInput): CharacterLayout | null {
  const c = i.character;
  if (!c || !i.visible) return null;
  if (i.view === "lyrics") return null;
  if (i.windowWidth < Math.max(MIN_WINDOW_FOR_CHARACTER, c.minWindowWidthPx)) return null;
  const scale = clampScale(i.scale);
  const state = i.playing ? "playing" : "stopped";
  const a = c[state];
  const widthPx = Math.round(c.widthPx * scale);
  return {
    state,
    src: i.reducedMotion && a.still ? a.still : a.path,
    widthPx,
    maxHeightPx: Math.round(c.maxHeightPx * scale),
    railPx: widthPx + RAIL_PADDING,
  };
}
