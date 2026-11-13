// v3.3.0：吹き出し（Tip.tsx）の包み方を決める。依存の無い関数だけを置く（scripts\ui-visibility.test.mts から Node だけで確かめるため）。
//
// 包み方は、吹き出しの文の有無で変えない。変えると React が包んだボタンを作り直し、キーボードで操作中のフォーカスが外れる
// （サイドバーは、たたむと文が入り、広げると文が無くなる。狭いウィンドウでは自動でたたまれる）。
// 文が無いときは、読み上げの結びつけ・見えない文・吹き出しを出さないだけ。

export interface TipParts {
  /** 包んだ要素に足す属性（文が無いときは何も足さない＝もとの属性のまま） */
  childProps: { "aria-describedby"?: string };
  /** 読み上げ用の見えない文（文が無いときは null で、置かない） */
  srText: string | null;
  /** 吹き出しを出してよいか */
  canShow: boolean;
}

export function tipParts(text: string | null | undefined, descId: string): TipParts {
  const on = typeof text === "string" && text.length > 0;
  return {
    childProps: on ? { "aria-describedby": descId } : {},
    srText: on ? text : null,
    canShow: on,
  };
}
