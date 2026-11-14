import { useEffect, useState } from "react";

/** 幅による4段階の縮み方（原文の順番）。CSS（base.css）と同じ境目を使う */
export interface LayoutSize {
  width: number;
  sidebarAuto: boolean; // 1. 1100px 未満：サイドバーをアイコンだけに
  hideGenre: boolean; // 2. 900px 未満：Genre を隠す
  hideAlbum: boolean; //    820px 未満：Album も隠す
  artSize: number; // 3. 720px 未満：ジャケット 44 → 36
  rowH: number; // 4. 600px 未満：行の高さ 60 → 48
}

/**
 * 幅ごとの大きさ。artSize は CSS の --row-art、rowH は --row-h（どちらも src\theme\tokens.css と base.css の幅の段）と同じ値にする。
 * rowH は仮想スクロールの1行の高さにもなる（TrackTable が --row-h に入れて渡す）。v3.7.0（デザインの見直し）：64 → 60px・48 → 44px。
 * 合っていることは scripts\ui-density.test.mts で確かめる
 */
export function layoutSizeFor(w: number): LayoutSize {
  return {
    width: w,
    sidebarAuto: w < 1100,
    hideGenre: w < 900,
    hideAlbum: w < 820,
    artSize: w < 720 ? 36 : 44,
    rowH: w < 600 ? 48 : 60,
  };
}

export function useLayoutSize(): LayoutSize {
  const [s, setS] = useState(() => layoutSizeFor(window.innerWidth));
  useEffect(() => {
    let raf = 0;
    const on = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => setS(layoutSizeFor(window.innerWidth)));
    };
    window.addEventListener("resize", on);
    return () => {
      window.removeEventListener("resize", on);
      cancelAnimationFrame(raf);
    };
  }, []);
  return s;
}
