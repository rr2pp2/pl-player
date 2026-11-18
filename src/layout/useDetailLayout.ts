// v3.8.0 段2-2：詳細（仕様の「配信モード」）の配置を、今の窓の大きさ・設定・出力先の数で決める（AppShell が使う）。
// 決まりは src\layout\detailLayout.ts（純粋な関数）。ここでは、そこへ渡す曲一覧の行の高さ（density.ts）と再生バーの高さ（playerLayout.ts）を組み立てる。
// 詳細でなければ null（画面の作りを今のまま変えない）。
import type { AppSettings } from "../ipc/types";
import { isDetail, outputs } from "../stores/outputs";
import { useStore } from "../stores/store";
import { barWidth, detailLayout, type DetailLayout } from "./detailLayout";
import { compactListOn, listLayoutFor, trackRowHeight } from "./density";
import { registered } from "./outputs";
import { PLAYER, playerLayout } from "./playerLayout";
import { playerButtonShown } from "./visibility";

export function useDetailLayout(size: { width: number; height: number }, settings: AppSettings, collapsed: boolean, lyricsRow: boolean): DetailLayout | null {
  const ov = useStore(outputs);
  if (!isDetail(ov)) return null;
  // 再生バーが3列か1段か（高さ 80・76）は、出力ボタンの幅だけで決まる（ほかのボタンは右の列の最低幅に入れていない）
  const outputW = playerButtonShown(settings.ui, "outputs") ? PLAYER.output3 : 0;
  return detailLayout({
    winW: size.width,
    winH: size.height,
    sidebarCollapsed: collapsed,
    slots: registered(ov).length,
    lyricsRow,
    rowHeight: (w) => trackRowHeight(compactListOn(settings.ui, w), listLayoutFor(w).rowH),
    playerH: playerLayout(barWidth(size.width), () => true, outputW).mode === "one" ? PLAYER.h1 : PLAYER.h3,
  });
}
