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

// ---------------- v3.3.0：動く画像を頭に戻さない ----------------
//
// 3.2.0 は、再生中・停止中で img の src を差し替えていた。差し替えると、動く画像（GIF・APNG・動く WebP）は最初のコマに戻る。
// 3.3.0 は、2枚とも読み込んだまま重ねて置き、見せる方だけを切り替える（src を変えない・要素を外さない）。
// - 一時停止の間も、見えていない方の絵の中の時間は止まるか進むか（WebView2 に任せる）。どちらでも頭には戻らない
// - 「動きを減らす」：今までどおり、今の状態の止まった絵（最初のコマ）だけを読む（動かないので、差し替えても困らない）
// - 歌詞画面・狭い幅で隠すとき：読み込んだまま見えなくする（幅は取らない）。戻ったら続きから
// - 使う人が「立ち絵を出す」を切ったとき・スキンを替えたとき・立ち絵の無いスキン：外す（読み込んだ画像を手放す）。
//   次に出すときは頭から（隠している間メモリを持ち続けないため）
// - 再生中と停止中が同じ画像なら、1枚だけ
// - 画面の要素の key（CharacterFrame.key）は画像の場所だけで決める。状態（再生中・停止中）を入れない。
//   同じ画像の1枚のとき、状態を入れると再生・一時停止のたびに key が変わり、要素が作り直されて頭に戻るため
//   （2枚のときは画像の場所が違うので、場所だけで重ならない）

export interface CharacterFrame {
  /** 画面の要素の key。画像の場所だけ（再生・一時停止で変わらない） */
  key: string;
  state: "playing" | "stopped";
  src: string;
  /** 見せている絵か（false なら読み込んだまま見えなくする） */
  shown: boolean;
}

export interface CharacterStage {
  /** 出しているときの置き方。一時的に隠しているときは null（幅も取らない） */
  layout: CharacterLayout | null;
  frames: CharacterFrame[];
  widthPx: number;
  maxHeightPx: number;
}

/** 立ち絵の画像を読み込んでおくか（外すときは null） */
export function characterStage(i: CharacterInput): CharacterStage | null {
  const c = i.character;
  if (!c || !i.visible) return null;
  const layout = characterLayout(i);
  const scale = clampScale(i.scale);
  const state = i.playing ? "playing" : "stopped";
  const on = layout !== null;
  const frame = (s: "playing" | "stopped", src: string, shown: boolean): CharacterFrame => ({ key: characterFrameKey(src), state: s, src, shown });
  let frames: CharacterFrame[];
  if (i.reducedMotion) {
    const a = c[state];
    frames = [frame(state, a.still ?? a.path, on)];
  } else if (c.playing.path === c.stopped.path) {
    frames = [frame(state, c.playing.path, on)];
  } else {
    frames = (["stopped", "playing"] as const).map((s) => frame(s, c[s].path, on && s === state));
  }
  return { layout, frames, widthPx: Math.round(c.widthPx * scale), maxHeightPx: Math.round(c.maxHeightPx * scale) };
}

/** 立ち絵の要素の key：画像の場所だけ（状態を入れない。上の「動く画像を頭に戻さない」） */
export function characterFrameKey(src: string): string {
  return src;
}
