// 古い文字コードのタグの読み直し（v3.5.0）：進み具合・始める・やめる・終わりの知らせ（確認の小窓は設定画面が出す）。
// 直すのはライブラリの DB の値だけで、曲ファイルには書かない（Rust 側 library/legacy_reread.rs）。
// 取り込み・再走査の途中なら Rust 側が入口で断る（library_import_busy）。画面のボタンを止めるだけに頼らない。
import { api } from "../ipc/commands";
import { on } from "../ipc/events";
import type { LegacyRereadProgress } from "../ipc/types";
import { legacyRereadDoneText } from "../messages/ja";
import { notify } from "./notices";
import { createStore } from "./store";

/** 進み具合（まだ一度も走らせていなければ null） */
export const legacyReread = createStore<LegacyRereadProgress | null>(null);

export const legacyRereadRunning = (p: LegacyRereadProgress | null): boolean => p?.running === true;

export async function initLegacyReread() {
  await on("legacyRereadProgress", (p) => {
    // 終わりの知らせのあとに、遅れて届いた進み具合で「走っている」に戻さない
    if (legacyReread.get()?.finished && p.running) return;
    legacyReread.set(p);
  });
  await on("legacyRereadDone", (p) => {
    legacyReread.set(p);
    const m = legacyRereadDoneText(p);
    notify({ level: m.level, text: m.text, detail: m.detail });
  });
  try {
    const p = await api.legacyRereadStatus();
    if (p.running || p.finished) legacyReread.set(p);
  } catch {
    /* 起動直後は無視 */
  }
}

/** 始める（確認は呼ぶ側の設定画面が先に出す） */
export async function startLegacyReread() {
  if (legacyRereadRunning(legacyReread.get())) return;
  // 先に「走っている」にしておく。返事より先に進み具合・終わりの通知が届くことがあるので、返事では上書きしない
  const prev = legacyReread.get();
  legacyReread.set({ running: true, finished: false, cancelled: false, total: 0, done: 0, fixed: 0, skippedManual: 0, changedDuringRun: 0, error: null });
  try {
    await api.legacyRereadStart();
  } catch (e) {
    legacyReread.set(prev);
    notify({ level: "error", error: e });
  }
}

export async function cancelLegacyReread() {
  try {
    await api.legacyRereadCancel();
  } catch (e) {
    notify({ level: "error", error: e });
  }
}
