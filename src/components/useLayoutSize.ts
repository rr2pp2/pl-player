import { useEffect, useState } from "react";
import { createStore, useStore } from "../stores/store";
import { listLayoutFor, type ListLayout } from "../layout/density";

/**
 * ウィンドウの幅で決めるもの。v3.8.0 から、ここに残るのはサイドバーの自動のたたみだけ（仕様 3.2「サイドバー：今の決まりのまま」）。
 * 曲一覧の列・ジャケット・行の高さ・自動のコンパクトは、曲一覧の側の幅で決める（下の useListLayout。境目は src\layout\density.ts の LIST_BELOW）
 */
export interface LayoutSize {
  width: number;
  /** v3.8.0 段2-2：窓の高さ（詳細の左右・上下と、曲一覧の行の数を決める。src\layout\detailLayout.ts） */
  height: number;
  sidebarAuto: boolean; // 1100px 未満：サイドバーをアイコンだけに
}

export function layoutSizeFor(w: number, h = 0): LayoutSize {
  return {
    width: w,
    height: h,
    sidebarAuto: w < 1100,
  };
}

export function useLayoutSize(): LayoutSize {
  const [s, setS] = useState(() => layoutSizeFor(window.innerWidth, window.innerHeight));
  useEffect(() => {
    let raf = 0;
    const on = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => setS(layoutSizeFor(window.innerWidth, window.innerHeight)));
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
 * 測る前（最初の描画）は、ウィンドウの幅からサイドバーをたたんだときの式（density.ts の LIST_BELOW の説明）で見積もる。
 * v3.9.0 直し7回目：立ち絵を出しているときは、専用の幅（railPx）を引いた曲一覧の実際の幅（src\skins\character.ts の listSideWidth）。
 * 曲一覧の列・ジャケット・行の高さ・自動のコンパクトはこの幅で決める
 */
export const listPaneWidth = createStore<number>(typeof window !== "undefined" ? Math.max(0, window.innerWidth - 94) : 1000);

/**
 * v3.9.0 直し7回目：.main の幅そのもの（立ち絵の専用の幅を引かない）。上の帯（.topbar）は立ち絵の上にかかる全部の幅を使うので、
 * 上の帯の文字を隠す幅（topbarNarrow）とモードの文字はこの幅で決める（3.8.1 までと同じ見た目）
 */
export const mainPaneWidth = createStore<number>(typeof window !== "undefined" ? Math.max(0, window.innerWidth - 94) : 1000);

/**
 * v3.9.0 直し5回目：立ち絵を出すか決める曲一覧の側の幅（専用の幅を取る前）。AppShell が決めて入れる（詳細の表示でミキサーを横に並べていれば
 * その左の幅）。設定の大きさの案内（SettingsAvatars.tsx）が、実際に出す決まりと同じ値で「今の窓で出せるか」を言うため
 */
export const characterPaneWidth = createStore<number>(typeof window !== "undefined" ? Math.max(0, window.innerWidth - 94) : 1000);

/**
 * v3.8.0 段2-2：詳細を抜けたあとの曲一覧の側の幅（サイドバーを除いた中央の幅。ミキサーを引かない）。AppShell が入れる。
 * 詳細のときに上の帯の「通常」「コンパクト」を押したら、この幅で設定を決める（仕様 13.5 の11。density.ts の pressMode）
 */
export const listPaneExitWidth = createStore<number>(typeof window !== "undefined" ? Math.max(0, window.innerWidth - 94) : 1000);

/** 曲一覧の大きさ（列・ジャケット・行の高さ）。曲一覧の側の幅で決める */
export function useListLayout(): ListLayout {
  return listLayoutFor(useStore(listPaneWidth));
}
