// 着せ替え（v3.2.0）：立ち絵を出すか・どの絵か・どの大きさか。
// 置き方は「右側に立ち絵用の幅を取る」だけ（曲一覧はその左で終わる）。歌詞画面と、幅が足りないときは出さない。
// v3.9.0：専用の幅は窓の幅で決める（立ち絵ごとの幅は使わない）。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

/** 立ち絵の幅の左右に取るすき間（合わせて） */
export const RAIL_PADDING = 12;
export const SCALE_MIN = 0.75;
/** v3.9.0：上限を 125% → 200% に（Rust の settings::store の丸めと同じ値） */
export const SCALE_MAX = 2;

// ---------------- v3.9.0：右の専用の幅は窓の幅で決める ----------------
// 2026-11-19 に決めた。値は暫定。下限と上限を決め、窓の幅に応じて変える。
// それまでは立ち絵ごとの幅（スキンの widthPx・追加した立ち絵の 180）を使っていたので、立ち絵を切り替えると
// 曲一覧・設定の幅が変わっていた（例：幅 220 のスキン）。今は立ち絵ごとの幅を使わず、同じ窓・同じ大きさならどの立ち絵でも同じ幅。
/** 専用の幅（絵の欄。すき間を除く）＝ 窓の幅 × この割合 × 大きさ */
export const RAIL_WINDOW_RATIO = 0.18;
/** 専用の幅の下限・上限（px。すき間を除く） */
export const RAIL_MIN_PX = 140;
export const RAIL_MAX_PX = 480;
/**
 * 隠す境目：曲一覧の側（中央の画面の幅から専用の幅を引いた残り）がこれより狭くなるなら隠す。
 * 3.8.1 の決まり（大きさ 100%・幅 180・窓 920）で残っていた幅：中央の幅 centerWidth(920, たたむ) = 920 − 余白 10×2 − サイドバー 64 − 間 10 = 826
 * から専用の幅 180 + 12 を引いた 634px（src\layout\detailLayout.ts の centerWidth と同じ値になることをテストで確かめる）。
 * スキンの minWindowWidthPx は使わない（置き方に立ち絵ごとの幅を使わなくなったので、その幅に合わせて決めた窓の下限も意味を持たない。
 * 使うと、立ち絵を切り替えたときに出る・隠れるの境目がずれ、立ち絵を切り替えると幅が変わる、という直した動きに戻る）
 * v3.9.0（詳細の表示でも出す）：詳細の表示のときは、詳細の配置が曲一覧に必ず残している幅（src\layout\detailLayout.ts の
 * DETAIL.listMin。360px）を境目にする。その値はここに写さず、呼ぶ側（AppShell・設定の案内）が `minListPx` として渡す
 */
export const MIN_LIST_SIDE_PX = 634;

export interface CharacterLike {
  playing: { path: string; still: string | null };
  stopped: { path: string; still: string | null };
  /** 立ち絵ごとの幅（スキンの widthPx・追加した立ち絵の 180）。v3.9.0 置き方には使わない */
  widthPx?: number;
  /** 高さの上限（立ち絵ごと）。× 大きさ が絵の高さ */
  maxHeightPx: number;
  /** スキンが決めた出す窓の幅の下限。v3.9.0 使わない（MIN_LIST_SIDE_PX の説明） */
  minWindowWidthPx?: number;
}

export interface CharacterInput {
  character: CharacterLike | null | undefined;
  /** 出すか（v3.9.0 からは常に true。「出さない」は Rust が character を null にして伝える） */
  visible: boolean;
  /** 使う人の設定：大きさ 0.75〜2（v3.9.0。3.8.1 までは 1.25） */
  scale: number;
  /** 今の画面（route の view） */
  view: string;
  /** 窓の幅（専用の幅を決める） */
  windowWidth: number;
  /**
   * 曲一覧の側の幅（専用の幅を取る前。中央の画面の幅、詳細の表示でミキサーを横に並べているときはその左の幅）。
   * 隠すかどうかは、ここから専用の幅を引いた残りで決める
   */
  paneWidth: number;
  /**
   * 曲一覧の側に残す幅（隠す境目）。詳細でないときは MIN_LIST_SIDE_PX（634）、詳細のときは DETAIL.listMin（360）。
   * 渡さなければ MIN_LIST_SIDE_PX
   */
  minListPx?: number;
  /** 再生中か（一時停止・停止・曲なしは停止中の絵） */
  playing: boolean;
  reducedMotion: boolean;
}

export interface CharacterLayout {
  state: "playing" | "stopped";
  src: string;
  /** 絵の欄の幅（すき間を除く。railWidth） */
  widthPx: number;
  maxHeightPx: number;
  /** 立ち絵用に取る幅（曲一覧などはこの分だけ右を空ける） */
  railPx: number;
}

export const clampScale = (s: number): number => (Number.isFinite(s) ? Math.min(SCALE_MAX, Math.max(SCALE_MIN, s)) : 1);

/**
 * 窓の幅だけで決めた専用の幅（すき間を除く）：clamp(140, 窓の幅 × 18% × 大きさ, 480) を整数に丸めたもの。立ち絵には関係しない。
 * 曲一覧の側に余裕があるときは、この幅で出す（いちばん大きく出す幅）
 */
export function idealRailWidth(windowWidth: number, scale: number): number {
  const w = Number.isFinite(windowWidth) ? windowWidth : 0;
  return Math.round(Math.min(RAIL_MAX_PX, Math.max(RAIL_MIN_PX, w * RAIL_WINDOW_RATIO * clampScale(scale))));
}

/**
 * 専用の幅（すき間を除く。v3.9.0：2026-11-19 に決めた）：
 *   clamp(140, min(窓の幅 × 18% × 大きさ, 曲一覧の側（引く前）− 残す幅 − すき間 12), 480)
 * 曲一覧の側が足りないときは、隠す前にまず幅を縮める。140px まで縮めても残す幅を割るときだけ隠す（null）。
 * 詳細でもふだんの表示でも同じ決め方（残す幅は詳細 360・ふだん 634。呼ぶ側が渡す）。
 * 前は窓の幅だけで決めた幅をそのまま取り、足りなければすぐ隠していたので、サイドバーが開く窓・詳細の左右で谷ができていた
 */
export function railWidthFor(windowWidth: number, scale: number, paneWidth: number, minListPx: number = MIN_LIST_SIDE_PX): number | null {
  const room = Math.floor(paneWidth - minListPx - RAIL_PADDING);
  if (!(room >= RAIL_MIN_PX)) return null;
  return Math.min(idealRailWidth(windowWidth, scale), room);
}

/** 立ち絵を出せるか：140px（＋すき間 12）まで縮めても、曲一覧の側に `minListPx`（既定は MIN_LIST_SIDE_PX。詳細のときは DETAIL.listMin）残るか */
export function characterFits(paneWidth: number, minListPx: number = MIN_LIST_SIDE_PX): boolean {
  return paneWidth - (RAIL_MIN_PX + RAIL_PADDING) >= minListPx;
}

/** 窓の幅を調べる範囲（案内の文で使う） */
const NOTE_MIN_WINDOW = 480;
const NOTE_MAX_WINDOW = 8000;

/** 今の窓での出し方：いちばん大きく出す（窓の幅だけで決めた幅）・縮めて出す・隠す */
export type CharacterFitState = "full" | "shrunk" | "hidden";

export function characterFitState(windowWidth: number, scale: number, paneWidth: number, minListPx: number = MIN_LIST_SIDE_PX): CharacterFitState {
  const r = railWidthFor(windowWidth, scale, paneWidth, minListPx);
  if (r === null) return "hidden";
  return r < idealRailWidth(windowWidth, scale) ? "shrunk" : "full";
}

export interface CharacterThresholds {
  /** この窓の幅より広ければ、ずっといちばん大きく出す（調べた範囲で届かなければ null） */
  fullFrom: number | null;
  /** この窓の幅より広ければ、ずっと出す（縮めることはある。調べた範囲で届かなければ null） */
  showFrom: number | null;
  /** fullFrom より狭い窓では、いつも縮めるか隠すか（いちばん大きく出る窓が混ざらない） */
  fullOnlyAbove: boolean;
  /** showFrom より狭い窓では、いつも隠すか（出る窓が混ざらない。混ざるのは詳細の左右と上下が窓の幅で入れ替わるところ） */
  hiddenBelow: boolean;
}

/**
 * この大きさの境目（案内の文）。`paneFor` は窓の幅 → 今の表示での曲一覧の側の幅（引く前）。
 * サイドバーが窓の幅 1100px で開く・詳細の左右と上下が入れ替わると曲一覧の側が狭くなるので、窓の幅に対してひと続きとは限らない。
 * それより広ければずっとそうなる、いちばん狭い窓の幅を返す
 */
export function characterThresholds(scale: number, paneFor: (windowWidth: number) => number, minListPx: number = MIN_LIST_SIDE_PX): CharacterThresholds {
  let fullFrom: number | null = NOTE_MIN_WINDOW;
  let showFrom: number | null = NOTE_MIN_WINDOW;
  let fullDone = false;
  let showDone = false;
  let fullOnlyAbove = true;
  let hiddenBelow = true;
  for (let w = NOTE_MAX_WINDOW; w >= NOTE_MIN_WINDOW; w--) {
    const st = characterFitState(w, scale, paneFor(w), minListPx);
    if (!fullDone && st !== "full") {
      fullFrom = w === NOTE_MAX_WINDOW ? null : w + 1;
      fullDone = true;
    } else if (fullDone && st === "full") fullOnlyAbove = false;
    if (!showDone && st === "hidden") {
      showFrom = w === NOTE_MAX_WINDOW ? null : w + 1;
      showDone = true;
    } else if (showDone && st !== "hidden") hiddenBelow = false;
  }
  return { fullFrom, showFrom, fullOnlyAbove, hiddenBelow };
}

export interface CharacterWindowNote extends CharacterThresholds {
  /** 今の窓での出し方（今の曲一覧の側の幅で決める。実際に出す決まり characterLayout と同じ） */
  state: CharacterFitState;
  /** 今の窓で出せるか（state が hidden でない） */
  fits: boolean;
  /** 大きさのつまみの下に出す文（隠すときは、そのことを文で書く。色だけで伝えない） */
  text: string;
}

/**
 * 大きさのつまみの下に出す案内（v3.9.0。「縮めてから隠す」決め方に合わせた）。
 * `hasCharacter` は出す立ち絵があるか（無ければ null）。`paneWidth` は今の曲一覧の側の幅（AppShell が決めたもの）、
 * `paneFor` は窓の幅 → 今の表示（詳細か・左右か上下か）での曲一覧の側の幅、`minListPx` は今の表示の残す幅（詳細なら DETAIL.listMin）。
 * `detail` は詳細の表示か（文の言い方だけに使う）。文は事実と食い違わない言い方だけを選ぶ（境目より狭い窓に別の出し方が混ざるときは「ことがあります」）
 */
export function characterWindowNote(
  hasCharacter: boolean,
  scale: number,
  windowWidth: number,
  paneWidth: number,
  paneFor: (windowWidth: number) => number,
  minListPx: number = MIN_LIST_SIDE_PX,
  detail = false,
): CharacterWindowNote | null {
  if (!hasCharacter) return null;
  const th = characterThresholds(scale, paneFor, minListPx);
  const w = Math.round(windowWidth);
  const state = characterFitState(windowWidth, scale, paneWidth, minListPx);
  const pct = Math.round(clampScale(scale) * 100);
  const where = detail ? "詳細の表示で、" : "";
  const parts: string[] = [];
  if (th.showFrom === null) {
    parts.push(`${where}この大きさ（${pct}%）では、立ち絵を出せる窓の幅がありません。`);
  } else {
    if (th.fullFrom !== null && th.fullFrom > th.showFrom) {
      parts.push(`${where}この大きさ（${pct}%）では、窓の幅がおよそ ${th.fullFrom}px 以上でいちばん大きく出ます。`);
      parts.push(th.fullOnlyAbove ? `それより狭いと縮めて出し、` : `それより狭いと縮めることがあり、`);
      parts.push(th.hiddenBelow ? `およそ ${th.showFrom}px より狭いと隠します。` : `およそ ${th.showFrom}px より狭いと隠すことがあります。`);
    } else {
      parts.push(`${where}この大きさ（${pct}%）では、窓の幅がおよそ ${th.showFrom}px 以上でいちばん大きく出ます。`);
      parts.push(th.hiddenBelow ? `それより狭いと隠します。` : `それより狭いと隠すことがあります。`);
    }
  }
  const now =
    state === "full"
      ? `今の窓（およそ ${w}px）では、いちばん大きく出ています。`
      : state === "shrunk"
        ? `今の窓（およそ ${w}px）では、曲一覧の側を ${minListPx}px 残すために縮めて出しています。`
        : `今の窓（およそ ${w}px）では、${detail ? "詳細の表示で" : ""}曲一覧の側に ${minListPx}px 残らないので、立ち絵を隠しています。窓を広げてください。`;
  parts.push(now);
  return { ...th, state, fits: state !== "hidden", text: parts.join("") };
}

/**
 * 曲一覧の実際の幅（v3.9.0）：.main の幅から、出している立ち絵の専用の幅（railPx。曲一覧などの margin-right と同じ値）を引いたもの。
 * 曲一覧の列（アルバム・ジャンル）・ジャケット・行の高さ・自動のコンパクトは、この幅で決める（density.ts の LIST_BELOW）。
 * 立ち絵を出していない・一時的に隠しているとき（layout が null）は .main の幅のまま。
 * 順番：出す・隠すは専用の幅を引く前の曲一覧の側の幅で決め（characterLayout）、そのあとで列の幅を引く。列の幅は出す判断に戻さないので、
 * 互いに引きずり合って切り替わり続けることはない
 */
export function listSideWidth(mainWidth: number, layout: Pick<CharacterLayout, "railPx"> | null): number {
  return Math.max(0, mainWidth - (layout ? layout.railPx : 0));
}

/** 出さないときは null */
export function characterLayout(i: CharacterInput): CharacterLayout | null {
  const c = i.character;
  if (!c || !i.visible) return null;
  if (i.view === "lyrics") return null;
  const scale = clampScale(i.scale);
  // 幅も出す・隠すも、引く前の曲一覧の側の幅だけで決める（列の幅＝引いた後の幅は入れない。listSideWidth の説明）
  const widthPx = railWidthFor(i.windowWidth, scale, i.paneWidth, i.minListPx ?? MIN_LIST_SIDE_PX);
  if (widthPx === null) return null;
  const state = i.playing ? "playing" : "stopped";
  const a = c[state];
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
// - 使う人が「出さない」を選んだとき・使う立ち絵やスキンを替えたとき・立ち絵の無いスキン：外す（読み込んだ画像を手放す）。
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
  return { layout, frames, widthPx: layout ? layout.widthPx : idealRailWidth(i.windowWidth, scale), maxHeightPx: Math.round(c.maxHeightPx * scale) };
}

/** 立ち絵の要素の key：画像の場所だけ（状態を入れない。上の「動く画像を頭に戻さない」） */
export function characterFrameKey(src: string): string {
  return src;
}

/**
 * 立ち絵の画像の大きさ（v3.9.0）：高さ（maxHeightPx × 大きさ。専用の欄の高さが足りなければその高さ）に合わせて出し、
 * 幅は縦横比のまま決まる。横長の画像で専用の幅からはみ出した左右は、欄（.character-rail の overflow: hidden）で切れる。
 * 欄は中央揃え・下揃えなので、切れるのは左右が同じだけ。3.8.1 までは幅にも収めていたので、横長の画像が小さく出ていた。
 * 縮まないよう flex: none（欄より広くても縮めない）
 */
export function characterImageStyle(maxHeightPx: number): { height: string; width: string; maxWidth: string; flex: string } {
  return { height: `min(${maxHeightPx}px, 100%)`, width: "auto", maxWidth: "none", flex: "none" };
}
