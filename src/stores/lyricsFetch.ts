// 歌詞をネットから取る（v3.1.0）：一括の進み具合と、1曲だけ取るときの状態。
// 設定がオフのときは Rust 側が入口で断る（画面のボタンを隠すだけに頼らない）。
// 知らせに出すのは件数と状態だけ（歌詞の本文・LRCLIB 側の曲名は Rust から来ない）。
import { api } from "../ipc/commands";
import { on } from "../ipc/events";
import type { LyricsFetchMode, LyricsFetchProgress } from "../ipc/types";
import { lyricsFetchDoneText, lyricsFetchOneText } from "../messages/ja";
import { reloadLyrics } from "./lyrics";
import { notify } from "./notices";
import { playback } from "./playback";
import { createStore } from "./store";

/** 一括の進み具合（まだ一度も走らせていなければ null） */
export const lyricsFetch = createStore<LyricsFetchProgress | null>(null);
/** 1曲だけ取っている曲の ID（取っていなければ null） */
export const lyricsFetchOne = createStore<number | null>(null);

export const lyricsFetchRunning = (p: LyricsFetchProgress | null): boolean => p?.running === true;

export async function initLyricsFetch() {
  await on("lyricsFetchProgress", (p) => lyricsFetch.set(p));
  await on("lyricsFetchDone", (d) => {
    lyricsFetch.set(d);
    const m = lyricsFetchDoneText(d);
    notify({ level: d.aborted && d.aborted !== "cancelled" ? "error" : "info", text: m.text, detail: m.detail });
    const cur = playback.get().track?.id;
    if (cur !== undefined && d.trackIds.includes(cur)) void reloadLyrics();
  });
  try {
    const p = await api.lyricsFetchStatus();
    if (p.running || p.finished) lyricsFetch.set(p);
  } catch {
    /* 起動直後は無視 */
  }
}

export async function startLyricsFetch(mode: LyricsFetchMode) {
  // 先に「走っている」にしておく。返事より先に進み具合・終わりの通知が届くことがあるので、返事では上書きしない
  const prev = lyricsFetch.get();
  lyricsFetch.set({
    running: true, finished: false, preparing: true, prepareTotal: 0, prepareDone: 0, mode, total: 0, done: 0, synced: 0, plain: 0, instrumental: 0, notFound: 0,
    tooLarge: 0, errors: 0, kept: 0, hasTag: 0, noMetadata: 0, skippedKnown: 0, changedDuringRun: 0, lastError: null,
    requests: 0, aborted: null,
  });
  try {
    await api.lyricsFetchStart(mode);
  } catch (e) {
    lyricsFetch.set(prev);
    notify({ level: "error", error: e });
  }
}

export async function cancelLyricsFetch() {
  try {
    await api.lyricsFetchCancel();
  } catch (e) {
    notify({ level: "error", error: e });
  }
}

/** 歌詞画面の「ネットから探す」 */
export async function fetchLyricsForTrack(trackId: number) {
  if (lyricsFetchOne.get() !== null) return;
  lyricsFetchOne.set(trackId);
  try {
    const r = await api.lyricsFetchTrack(trackId);
    const m = lyricsFetchOneText(r);
    notify({ level: m.level, text: m.text });
    if ((r.status === "synced" || r.status === "plain") && playback.get().track?.id === r.trackId) void reloadLyrics();
  } catch (e) {
    notify({ level: "error", error: e });
  } finally {
    lyricsFetchOne.set(null);
  }
}
