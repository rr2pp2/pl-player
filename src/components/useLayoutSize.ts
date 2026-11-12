import { useEffect, useState } from "react";

/** 幅による4段階の縮み方（原文の順番）。CSS（base.css）と同じ境目を使う */
export interface LayoutSize {
  width: number;
  sidebarAuto: boolean; // 1. 1100px 未満：サイドバーをアイコンだけに
  hideGenre: boolean; // 2. 900px 未満：Genre を隠す
  hideAlbum: boolean; //    820px 未満：Album も隠す
  artSize: number; // 3. 720px 未満：ジャケット 48 → 36
  rowH: number; // 4. 600px 未満：行の高さ 64 → 48
}

function compute(w: number): LayoutSize {
  return {
    width: w,
    sidebarAuto: w < 1100,
    hideGenre: w < 900,
    hideAlbum: w < 820,
    artSize: w < 720 ? 36 : 48,
    rowH: w < 600 ? 48 : 64,
  };
}

export function useLayoutSize(): LayoutSize {
  const [s, setS] = useState(() => compute(window.innerWidth));
  useEffect(() => {
    let raf = 0;
    const on = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => setS(compute(window.innerWidth)));
    };
    window.addEventListener("resize", on);
    return () => {
      window.removeEventListener("resize", on);
      cancelAnimationFrame(raf);
    };
  }, []);
  return s;
}
