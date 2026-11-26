// v3.8.0：詳細（仕様の「配信モード」）の配置を、今の窓の大きさ・設定・出力先の数で決める（AppShell が使う）。
// 決まりは src\layout\detailLayout.ts（純粋な関数）。ここでは、そこへ渡す曲一覧の行の高さ（density.ts）と再生バーの高さ（playerLayout.ts）を組み立てる。
// 詳細でなければ null（画面の作りを今のまま変えない）。
import type { AppSettings } from "../ipc/types";
import { isDetail, outputs } from "../stores/outputs";
import { useStore } from "../stores/store";
import { barWidth, centerWidth, DETAIL, detailLayout, type DetailInput, type DetailLayout } from "./detailLayout";
import { MIN_LIST_SIDE_PX } from "../skins/character";
import { compactListOn, listLayoutFor, trackRowHeight } from "./density";
import { registered } from "./outputs";
import { PLAYER, playerLayout } from "./playerLayout";
import { playerButtonShown } from "./visibility";

/** detailLayout に渡す値（窓の幅を変えて計算し直せるよう、フックから分けた。v3.9.0：立ち絵の案内が使う） */
export function detailInputFor(winW: number, winH: number, settings: AppSettings, collapsed: boolean, lyricsRow: boolean, slots: number): DetailInput {
  // 再生バーが3列か1段か（高さ 80・76）は、出力ボタンの幅だけで決まる（ほかのボタンは右の列の最低幅に入れていない）
  const outputW = playerButtonShown(settings.ui, "outputs") ? PLAYER.output3 : 0;
  return {
    winW,
    winH,
    sidebarCollapsed: collapsed,
    slots,
    lyricsRow,
    rowHeight: (w) => trackRowHeight(compactListOn(settings.ui, w), listLayoutFor(w).rowH),
    playerH: playerLayout(barWidth(winW), () => true, outputW).mode === "one" ? PLAYER.h1 : PLAYER.h3,
  };
}

export function useDetailLayout(size: { width: number; height: number }, settings: AppSettings, collapsed: boolean, lyricsRow: boolean): DetailLayout | null {
  const ov = useStore(outputs);
  if (!isDetail(ov)) return null;
  return detailLayout(detailInputFor(size.width, size.height, settings, collapsed, lyricsRow, registered(ov).length));
}

/** 立ち絵（v3.9.0）：隠すかを決める曲一覧の側の幅（専用の幅を取る前）。詳細ならミキサーの左（上下のときは中央の幅）、そうでなければ中央の幅 */
export function characterPaneOf(winW: number, D: DetailLayout | null, collapsed: boolean): number {
  return D ? D.listW : centerWidth(winW, collapsed);
}

/** 立ち絵：曲一覧の側に残す幅（隠す境目）。詳細なら詳細の配置が曲一覧に必ず残す幅（DETAIL.listMin。詳細の表示でも出す）、そうでなければ 634 */
export function characterMinListOf(D: DetailLayout | null): number {
  return D ? DETAIL.listMin : MIN_LIST_SIDE_PX;
}

/**
 * 立ち絵の案内：窓の幅 → 今の表示での曲一覧の側の幅。詳細なら、窓の幅を変えて詳細の配置を計算し直す（左右・上下も窓で決まる）。
 * サイドバーは設定の値（窓 1100px 未満は centerWidth が自動でたたむ）
 */
export function useCharacterPaneFor(size: { width: number; height: number }, settings: AppSettings, lyricsRow: boolean): (winW: number) => number {
  const ov = useStore(outputs);
  const collapsed = settings.ui.sidebarCollapsed;
  if (!isDetail(ov)) return (w) => centerWidth(w, collapsed);
  const slots = registered(ov).length;
  return (w) => detailLayout(detailInputFor(w, size.height, settings, collapsed || w < 1100, lyricsRow, slots)).listW;
}
