// v3.8.0：再生バーの形（3列か1段か）と、どのボタンをどこに置くか。ほかに依存しない純粋な関数だけを置く
// （scripts\ui-layout.test.mts から Node だけで確かめるため）。
//
// 仕様（同時再生と配信モード 3.2・4.1・4.2）：
// - 決まった窓の幅では区切らず、「中身が入るかどうか」で切り替える。再生バーの幅（PlayerBar が測る）だけで決める
// - 3列：真ん中の列（再生バーの幅の 40%、240〜560px）と、左右の列の最低幅が入るなら3列。左右の列は同じ幅（1fr ずつ）なので、
//   再生ボタンは真ん中の列の中央＝再生バーの中央に固定され、右の列の中身で動かない
// - 入りきらないボタン：シャッフル・リピートは真ん中の列に入らなければ、右の列の「···」へ。歌詞のボタン（流れる歌詞の列・歌詞）も
//   右の列に入らなければ「···」へ。設定で隠したボタンは「···」にも出さない
// - 1段：シークバーは上の縁の細い線、左はジャケット・曲名・今の時間 / 長さ、中は前・再生/一時停止・次、右は「···」（全体の音量など）
// 出力ボタン（4.3。段2）は、右の列の全体の音量の左に固定幅で入る。その幅を outputW に渡せば、最低幅と並びに入る（今は 0）。

/** 寸法（px）。CSS（base.css の再生バーの節と tokens.css の --player-center-w）と同じ値。合っていることは scripts\ui-layout.test.mts で確かめる */
export const PLAYER = {
  /** 再生バーの枠（.panel の 1px を左右） */
  border: 2,
  /** 3列のときの左右の余白と列の間（v3.7.0 の .player と同じ） */
  pad3: 16,
  gap3: 16,
  /** 1段のときの左右の余白と列の間（3.7.0 の 719px 以下の値） */
  pad1: 10,
  gap1: 10,
  /** 真ん中の列：再生バーの中の幅の 40%、240〜560px（--player-center-w） */
  centerRatio: 0.4,
  centerMin: 240,
  centerMax: 560,
  /** ボタン（再生・一時停止 44、ほか 36、間 4。v3.7.0） */
  btn: 36,
  play: 44,
  btnGap: 4,
  /**
   * 左の列の最低幅：ジャケット 60 ＋ 間 12 ＋ 曲名が読める幅 120。
   * 120px は曲名の文字（17px。スキンで ±2px）で全角7字ほど。多くの曲名で、どの曲か分かる長さ（ジャケットと並べて見る）
   */
  art3: 60,
  artGap: 12,
  titleMin: 120,
  /** 全体の音量：ミュートのボタン 36 ＋ 間 6 ＋ 音量の帯 96（v3.7.0 の広い窓の幅） */
  volume: 36 + 6 + 96,
  /** 「···」（その他）のボタン */
  more: 36,
  /** 1段のときのジャケット（3.7.0 の 719px 以下と同じ 48px） */
  art1: 48,
  /** 高さ：3列は --player-h の 80px、1段は 76px（3.7.0 の 719px 以下と同じ値） */
  h3: 80,
  h1: 76,
} as const;

/** 再生バーのうち、置き場所が動くボタン（設定で隠せるもの。visibility.ts の PLAYER_BUTTONS と同じ名前） */
export type MovableButton = "shuffle" | "repeat" | "volume" | "lyricsRow" | "lyrics";

export interface PlayerLayout {
  mode: "three" | "one";
  /** 3列のときの真ん中の列の幅（1段のときは 0） */
  centerW: number;
  /** 3列のときの左右の列の幅（1段のときは 0） */
  sideW: number;
  /** 真ん中の列の両脇に置くか（3列だけ） */
  centerShuffle: boolean;
  centerRepeat: boolean;
  /** 右の列に並べるもの（左から。出力ボタン（段2）はこの前） */
  right: MovableButton[];
  /** 「···」の中に入れるもの（上から。空なら「···」を出さない） */
  more: MovableButton[];
}

/** 真ん中の列の幅（innerW は再生バーの中の幅＝枠と左右の余白を引いた幅。CSS の clamp(240px, 40%, 560px) と同じ） */
export function playerCenterWidth(innerW: number): number {
  return Math.min(PLAYER.centerMax, Math.max(PLAYER.centerMin, innerW * PLAYER.centerRatio));
}

export function playerLeftMin(): number {
  return PLAYER.art3 + PLAYER.artGap + PLAYER.titleMin;
}

/** 右の列の最低幅：出力ボタン（段2。今は 0）＋全体の音量＋「···」 */
export function playerRightMin(outputW = 0): number {
  return (outputW > 0 ? outputW + PLAYER.btnGap : 0) + PLAYER.volume + PLAYER.btnGap + PLAYER.more;
}

/** 真ん中の列に、両脇のシャッフル・リピートまで入る幅（36×4 ＋ 44 ＋ 間 4×4 ＝ 204px） */
export const CENTER_FULL_W = PLAYER.btn * 4 + PLAYER.play + PLAYER.btnGap * 4;

const widthOf = (b: MovableButton | "more"): number => (b === "volume" ? PLAYER.volume : b === "more" ? PLAYER.more : PLAYER.btn);

/** 右の列に並べたときの幅（間 4。出力ボタンは前に固定幅） */
function rowWidth(items: (MovableButton | "more")[], outputW: number): number {
  const parts = items.map(widthOf);
  if (outputW > 0) parts.unshift(outputW);
  return parts.reduce((a, b) => a + b, 0) + Math.max(0, parts.length - 1) * PLAYER.btnGap;
}

/**
 * 再生バーの形を決める。barW は再生バーの幅（枠を含む。.player の getBoundingClientRect の幅）。
 * shown は設定の「再生バーのボタン」で出しているか（隠したものは右の列にも「···」にも出さない）
 */
export function playerLayout(barW: number, shown: (b: MovableButton) => boolean, outputW = 0): PlayerLayout {
  const inner3 = barW - PLAYER.border - PLAYER.pad3 * 2;
  const centerW = playerCenterWidth(inner3);
  const sideW = (inner3 - centerW - PLAYER.gap3 * 2) / 2;
  const three = sideW >= Math.max(playerLeftMin(), playerRightMin(outputW));
  if (!three) {
    // 1段：全体の音量・シャッフル・リピート・歌詞のボタンは全部「···」へ（出しているものだけ）
    const more = (["volume", "shuffle", "repeat", "lyricsRow", "lyrics"] as const).filter(shown);
    return { mode: "one", centerW: 0, sideW: 0, centerShuffle: false, centerRepeat: false, right: [], more };
  }
  const centerFits = centerW >= CENTER_FULL_W;
  const more: MovableButton[] = [];
  const centerShuffle = shown("shuffle") && centerFits;
  const centerRepeat = shown("repeat") && centerFits;
  if (shown("shuffle") && !centerFits) more.push("shuffle");
  if (shown("repeat") && !centerFits) more.push("repeat");
  // 右の列：全体の音量はいつも右の列（最低幅に入れてある）。歌詞のボタン（流れる歌詞の列のオン・オフと歌詞）は、
  // 出しているものが全部右の列に入れば並べ、入らなければまとめて「···」へ移す。
  // 「歌詞だけを右に残す」途中の段は作らない：両方を出しているとき、流れる歌詞の列のボタン（36px）を「···」（36px）に
  // 置き換えるだけで幅が同じになり、その段が来る幅が無いため（検査1回目の指摘1）。
  // 「···」へ移したとき（音量＋「···」＝右の列の最低幅）は、3列にした条件で必ず入る
  const base: MovableButton[] = shown("volume") ? ["volume"] : [];
  const lyricsWanted = (["lyricsRow", "lyrics"] as const).filter(shown);
  const allFit = rowWidth([...base, ...lyricsWanted, ...(more.length > 0 ? (["more"] as const) : [])], outputW) <= sideW;
  const right: MovableButton[] = allFit ? [...base, ...lyricsWanted] : base;
  if (!allFit) more.push(...lyricsWanted);
  return { mode: "three", centerW, sideW, centerShuffle, centerRepeat, right, more };
}
