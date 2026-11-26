// v3.8.0：再生バーの形（3列か1段か）と、どのボタンをどこに置くか。ほかに依存しない純粋な関数だけを置く
// （scripts\ui-layout.test.mts から Node だけで確かめるため）。
//
// 仕様（同時再生と配信モード 3.2・4.1・4.2）：
// - 決まった窓の幅では区切らず、「中身が入るかどうか」で切り替える。再生バーの幅（PlayerBar が測る）だけで決める
// - 3列：真ん中の列（再生バーの幅の 40%、240〜560px）と、左右の列の最低幅が入るなら3列。左右の列は同じ幅（1fr ずつ）なので、
//   再生ボタンは真ん中の列の中央＝再生バーの中央に固定され、右の列の中身で動かない
// - 入りきらないボタン：シャッフル・リピートは真ん中の列に入らなければ、右の列の「···」へ。歌詞のボタン（流れる歌詞の列・歌詞）も
//   右の列に入らなければ「···」へ。v3.8.0：それでも入らなければ全体の音量も「···」へ（1段にする前に3列を保つ）。
//   設定で隠したボタンは「···」にも出さない
// - 1段：シークバーは上の縁の細い線、左はジャケット・曲名・今の時間 / 長さ、中は前・再生/一時停止・次、右は「···」（全体の音量など）
// 出力ボタン（4.3）は、右の列の全体の音量の左に固定幅で入る（3列 PLAYER.output3・1段 PLAYER.output1）。
// PlayerBar は出していれば outputW に PLAYER.output3 を渡す（3列の最低幅に入る）。1段のときの幅は PLAYER.output1。

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
  /**
   * v3.8.0：出力ボタン（仕様 4.3。--player-output-w）。中身で幅が変わらない固定の幅。高さは 40px（ほかのボタンの 36px より大きく、押しやすく）。
   * 3列：112px ＝ 枠 1×2 ＋ 余白 8×2 ＋ アイコン 20 ＋ 間 6 ＋ 名前 68（14px の字で全角4字ほど。長い名前は「…」で切り、全部は吹き出しと読み上げに出す）。
   *   右の列に入らないときは全体の音量を「···」へ移して3列を保つ。3列は出力ボタンが無いときと同じ再生バーの幅 728px から、
   *   全体の音量が右の列に並ぶのは 1068px から（scripts\ui-detail.test.mts）
   * 1段：80px。中身は 枠 1×2 ＋ 余白 6×2 ＋ アイコン 20 を3つ ＋ 間 2×2 ＝ 78（オンの出力先が3つでも入る幅。1つのときも同じ幅で、アイコンは真ん中に）。
   *   480px の窓で、曲名と時間の文字に 120px 以上が残る幅（scripts\ui-layout.test.mts の「480×480」）
   */
  output3: 112,
  output1: 80,
  outputH: 40,
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
  /** 右の列に並べるもの（左から。出力ボタンはこの前） */
  right: MovableButton[];
  /** 「···」の中に入れるもの（上から。空なら「···」を出さない） */
  more: MovableButton[];
  /** 出力ボタンの幅（出さないときは 0。3列は渡された outputW、1段は PLAYER.output1） */
  output: number;
}

/** 真ん中の列の幅（innerW は再生バーの中の幅＝枠と左右の余白を引いた幅。CSS の clamp(240px, 40%, 560px) と同じ） */
export function playerCenterWidth(innerW: number): number {
  return Math.min(PLAYER.centerMax, Math.max(PLAYER.centerMin, innerW * PLAYER.centerRatio));
}

export function playerLeftMin(): number {
  return PLAYER.art3 + PLAYER.artGap + PLAYER.titleMin;
}

/** 右の列に全体の音量まで置くのに要る幅：出力ボタン（出さないときは 0）＋全体の音量＋「···」 */
export function playerRightMin(outputW = 0): number {
  return (outputW > 0 ? outputW + PLAYER.btnGap : 0) + PLAYER.volume + PLAYER.btnGap + PLAYER.more;
}

/**
 * v3.8.0：3列にするための右の列の最低幅。全体の音量も「···」へ移せるので、出力ボタン＋「···」。
 * 出力ボタン 112px でも 152px で、左の列の最低幅（192px）より小さい。だから3列が始まる幅は、出力ボタンがあっても、無いときと同じ（再生バーの幅 728px）
 */
export function playerRightMinCompact(outputW = 0): number {
  return (outputW > 0 ? outputW + PLAYER.btnGap : 0) + PLAYER.more;
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
  const three = sideW >= Math.max(playerLeftMin(), playerRightMinCompact(outputW));
  if (!three) {
    // 1段：全体の音量・シャッフル・リピート・歌詞のボタンは全部「···」へ（出しているものだけ）
    const more = (["volume", "shuffle", "repeat", "lyricsRow", "lyrics"] as const).filter(shown);
    return { mode: "one", centerW: 0, sideW: 0, centerShuffle: false, centerRepeat: false, right: [], more, output: outputW > 0 ? PLAYER.output1 : 0 };
  }
  const centerFits = centerW >= CENTER_FULL_W;
  const more: MovableButton[] = [];
  const centerShuffle = shown("shuffle") && centerFits;
  const centerRepeat = shown("repeat") && centerFits;
  if (shown("shuffle") && !centerFits) more.push("shuffle");
  if (shown("repeat") && !centerFits) more.push("repeat");
  // 右の列（出力ボタンの右）：入れる順は 全体の音量 → 歌詞のボタン（流れる歌詞の列のオン・オフと歌詞）。
  // 1. 出しているものが全部入れば並べる
  // 2. 入らなければ、歌詞のボタンをまとめて「···」へ（全体の音量＋「···」は右に残す）
  //    「歌詞だけを右に残す」途中の段は作らない：両方を出しているとき、流れる歌詞の列のボタン（36px）を「···」（36px）に
  //    置き換えるだけで幅が同じになり、その段が来る幅が無いため
  // 3. それでも入らなければ、全体の音量も「···」へ（v3.8.0。1段にする前に3列を保つ。仕様 4.1 の「···」へ直接移す流儀）。
  //    「···」の中では全体の音量を先頭に置く（1段のときと同じ並び）。歌詞のボタンだけが右に残る段は作らない（音量を先に右へ置く）
  //    出力ボタン＋「···」は、3列にした条件（playerRightMinCompact）で必ず入る
  const base: MovableButton[] = shown("volume") ? ["volume"] : [];
  const lyricsWanted = (["lyricsRow", "lyrics"] as const).filter(shown);
  const fits = (items: MovableButton[], withMore: boolean) =>
    rowWidth([...items, ...(withMore ? (["more"] as const) : [])], outputW) <= sideW;
  let right: MovableButton[];
  if (fits([...base, ...lyricsWanted], more.length > 0)) {
    right = [...base, ...lyricsWanted];
  } else if (fits(base, true)) {
    right = base;
    more.push(...lyricsWanted);
  } else {
    right = [];
    more.unshift(...base);
    more.push(...lyricsWanted);
  }
  return { mode: "three", centerW, sideW, centerShuffle, centerRepeat, right, more, output: outputW };
}
