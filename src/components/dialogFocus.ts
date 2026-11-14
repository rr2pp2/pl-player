// v3.7.0（デザインの見直し）：確認の小窓で、最初にフォーカスを置くボタン。
// 危険な確認（danger。削除・ごみ箱へ移す・ライブラリから外す・キューを全削除 など）は「キャンセル」に置く。
// 誤って Enter を押しても、消す操作が走らないように。押したあとの処理は変えない（確定ボタンを押せば今までどおり進む）。
// 危険でない確認は、今までどおり確定のボタンに置く。
// ほかに依存しない純粋な関数だけを置く（scripts\ui-design.test.mts から Node だけで確かめるため）。

export type ConfirmFocus = "cancel" | "ok";

export function confirmInitialFocus(danger: boolean | undefined): ConfirmFocus {
  return danger === true ? "cancel" : "ok";
}
