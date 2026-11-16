// v3.8.0：曲一覧の見出しの並べ替えの印（矢印）を、どの見出しに出すか。
// ほかに依存しない純粋な関数だけを置く（scripts\ui-density.test.mts から Node だけで確かめるため）。
//
// アーティストで並べ替えたまま、アーティストの見出しが無くなることがある（コンパクト表示から通常の表示に戻ったとき・
// コンパクト表示でアーティストの列を切ったとき）。3.7.0 までは、並びはアーティスト順のままなのに、どこにも矢印が出なかった。
// そのときは並びを変えず（勝手に並べ替えを戻さない）、タイトルの見出し（通常の表示では「タイトル / アーティスト」）に
// 矢印を出し、吹き出しと読み上げで「アーティストで並べ替え中」と伝える。押したときの動きは今までどおり（タイトルの並べ替え）。
// 年間まとめ（fixedOrder。並びの決まっている一覧）は、今までどおりどこにも矢印を出さない。

export interface SortStateLike {
  by: string | null;
  desc: boolean;
}

/** 見出しが無いときに代わりに矢印を出す見出し（並べ替えの鍵 → 代わりの見出しの鍵）。今はアーティスト → タイトルだけ */
export const SORT_STAND_IN: Readonly<Record<string, string>> = { artist: "title" };

/** 代わりに矢印を出すことがある見出しの鍵（TrackTable は、この見出しだけをいつも吹き出しで包む） */
export const SORT_STAND_IN_HEADS: readonly string[] = [...new Set(Object.values(SORT_STAND_IN))];

export interface SortHead {
  /** 矢印を出す見出しの鍵（押したときの並べ替えの鍵）。どこにも出さないときは null */
  at: string | null;
  /** 逆順か（↓）。at が null のときは false */
  desc: boolean;
  /** 代わりの見出しに出しているときの、吹き出しと読み上げの文。そうでなければ null */
  hint: string | null;
}

/** 代わりの見出しに出すときの文 */
export function sortStandInHint(desc: boolean): string {
  return `アーティストで並べ替え中${desc ? "（逆順）" : ""}。押すとタイトルで並べ替えます。`;
}

/**
 * 並べ替えの状態と、今の見出しの形から、矢印を出す見出しを決める。
 * artistColumn：アーティストの見出しがあるか（コンパクト表示で、アーティストの列を出しているとき。TrackTable の cols.artist）
 */
export function sortHead(
  s: SortStateLike | null | undefined,
  o: { fixedOrder?: boolean; artistColumn: boolean },
): SortHead {
  if (o.fixedOrder || !s || typeof s.by !== "string" || s.by === "") return { at: null, desc: false, hint: null };
  const desc = s.desc === true;
  if (s.by === "artist" && !o.artistColumn) return { at: SORT_STAND_IN.artist, desc, hint: sortStandInHint(desc) };
  return { at: s.by, desc, hint: null };
}
