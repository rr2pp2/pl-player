import { useEffect, useState } from "react";
import { createStore, useStore } from "../stores/store";
import { listLayoutFor, type ListLayout } from "../layout/density";

/**
 * ウィンドウの幅で決めるもの。v3.8.0 から、ここに残るのはサイドバーの自動のたたみだけ（仕様 3.2「サイドバー：今の決まりのまま」）。
 * 曲一覧の列・ジャケット・行の高さ・自動のコンパクトは、曲一覧の側の幅で決める（下の useListLayout。境目は src\layout\density.ts の LIST_BELOW）
 */
export interface LayoutSize {
  width: number;
  sidebarAuto: boolean; // 1100px 未満：サイドバーをアイコンだけに
}

export function layoutSizeFor(w: number): LayoutSize {
  return {
    width: w,
    sidebarAuto: w < 1100,
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

/**
 * v3.8.0：曲一覧の側の幅（px）。AppShell が .main（上の帯・曲一覧・立ち絵の枠）の幅を ResizeObserver で測って入れる。
 * 測る前（最初の描画）は、ウィンドウの幅からサイドバーをたたんだときの式（density.ts の LIST_BELOW の説明）で見積もる
 */
export const listPaneWidth = createStore<number>(typeof window !== "undefined" ? Math.max(0, window.innerWidth - 94) : 1000);

/** 曲一覧の大きさ（列・ジャケット・行の高さ）。曲一覧の側の幅で決める */
export function useListLayout(): ListLayout {
  return listLayoutFor(useStore(listPaneWidth));
}
