// 歌詞（v3）の表示用の状態：今の曲の歌詞・読み込みの世代番号・今の行・自動追従・歌詞画面の開閉。
// 位置は既存の通知（pl://playback・pl://position）だけを使い、画面側で requestAnimationFrame で補う。
// 補った位置は画面にだけ使い、毎フレーム store には書かない（今の行が変わったときだけ書く）。
import { api } from "../ipc/commands";
import type { InboxReport, LyricsLine, LyricsPayload } from "../ipc/types";
import { errorMessage, inboxReportText } from "../messages/ja";
import { route, type Route } from "./library";
import { notify } from "./notices";
import { playback, position } from "./playback";
import { createStore } from "./store";
import { findActive, findNext, shownTime, type ActiveRange } from "./lyricsTime";

export interface LyricsState {
  trackId: number | null;
  status: "idle" | "loading" | "ready" | "error";
  payload: LyricsPayload | null;
  error: { text: string; detail: string } | null;
}

export const lyrics = createStore<LyricsState>({ trackId: null, status: "idle", payload: null, error: null });

/** 読み込みの世代番号。前の曲の遅い返事を捨てるのに使う */
let generation = 0;

export async function loadLyrics(trackId: number | null, keepShown = false) {
  const gen = ++generation;
  if (trackId === null) {
    lyrics.set({ trackId: null, status: "idle", payload: null, error: null });
    return;
  }
  // 曲が変わったら古い歌詞をすぐ消す
  if (!keepShown || lyrics.get().trackId !== trackId) lyrics.set({ trackId, status: "loading", payload: null, error: null });
  try {
    const p = await api.getLyrics(trackId);
    if (gen !== generation) return;
    lyrics.set({ trackId, status: "ready", payload: p, error: null });
  } catch (e) {
    if (gen !== generation) return;
    lyrics.set({ trackId, status: "error", payload: null, error: errorMessage(e) });
  }
}

/** 今の曲の歌詞を読み直す（表示は消さずに差し替える） */
export function reloadLyrics() {
  const id = playback.get().track?.id ?? null;
  return loadLyrics(id, true);
}

/** 保存・削除の返事（読み解き済み）をそのまま使う */
export function applyLyricsPayload(p: LyricsPayload) {
  if (p.trackId !== (playback.get().track?.id ?? null)) return;
  generation++;
  lyrics.set({ trackId: p.trackId, status: "ready", payload: p, error: null });
}

// ---- 曲ごとの補正 ----

export const MAX_DELAY_MS = 600_000;

export async function changeDelay(next: number) {
  const st = lyrics.get();
  const p = st.payload;
  if (!p) return;
  const v = Math.max(-MAX_DELAY_MS, Math.min(MAX_DELAY_MS, Math.round(next)));
  // 先に画面へ反映
  lyrics.set({ ...st, payload: { ...p, userDelayMs: v } });
  try {
    await api.setLyricsDelay(p.trackId, v);
  } catch (e) {
    notify({ level: "error", error: e });
    void reloadLyrics();
  }
}

// ---- 外部歌詞の取り込み（ボタンを押したときだけ） ----

export const lastInboxReport = createStore<InboxReport | null>(null);

export async function importLyricsInbox(): Promise<InboxReport | null> {
  try {
    const r = await api.importLyricsInbox();
    lastInboxReport.set(r);
    const m = inboxReportText(r);
    notify({ level: "info", text: m.text, detail: m.problems });
    const cur = playback.get().track?.id;
    if (cur !== undefined && r.trackIds.includes(cur)) void reloadLyrics();
    return r;
  } catch (e) {
    notify({ level: "error", error: e });
    return null;
  }
}

export async function openLyricsInbox() {
  try {
    await api.openLyricsInbox();
  } catch (e) {
    notify({ level: "error", error: e });
  }
}

// ---- 歌詞画面の開閉（開く前の画面へ戻る） ----

let returnTo: Route | null = null;

export function openLyricsView() {
  const r = route.get();
  if (r.view === "lyrics") return;
  returnTo = r;
  route.set({ view: "lyrics" });
}

export function closeLyricsView() {
  if (route.get().view !== "lyrics") return;
  route.set(returnTo ?? { view: "all" });
  returnTo = null;
}

export function toggleLyricsView() {
  if (route.get().view === "lyrics") closeLyricsView();
  else openLyricsView();
}

// ---- 歌詞の編集画面 ----

export interface EditorTarget {
  trackId: number;
  title: string;
  artist: string;
  initialText: string;
  hasManual: boolean;
}

export const lyricsEditor = createStore<EditorTarget | null>(null);

export function openLyricsEditor() {
  const t = playback.get().track;
  if (!t) return;
  const p = lyrics.get().payload;
  const mine = p && p.trackId === t.id ? p : null;
  lyricsEditor.set({
    trackId: t.id,
    title: t.title,
    artist: t.artist,
    initialText: mine?.rawText ?? "",
    hasManual: mine?.source === "manual",
  });
}

// ---- 自動追従 ----

export const FOLLOW_RESUME_MS = 8000;
export const follow = createStore<{ paused: boolean }>({ paused: false });
let followTimer: number | undefined;

/** 手でスクロールした：追従を止め、最後に動かしてから 8 秒で戻す */
export function pauseFollow() {
  window.clearTimeout(followTimer);
  followTimer = window.setTimeout(resumeFollow, FOLLOW_RESUME_MS);
  if (!follow.get().paused) follow.set({ paused: true });
}

export function resumeFollow() {
  window.clearTimeout(followTimer);
  if (follow.get().paused) follow.set({ paused: false });
}

// ---- 今の行 ----

// 表示時刻と「今の行」の計算は lyricsTime.ts（純粋な関数。Node だけで確かめられる）
export { findActive, findNext, isShown, shownTime, type ActiveRange } from "./lyricsTime";

export const activeLine = createStore<ActiveRange | null>(null);
/** 再生バーで今の行の右に出す「次の行」（時刻つきのときだけ。無ければ null） */
export const nextLine = createStore<ActiveRange | null>(null);

let lastPos = 0;
let lastAt = 0;
let raf = 0;

function nowPosition(): number {
  let p = lastPos;
  if (playback.get().status === "playing") p += performance.now() - lastAt;
  const dur = position.get().durationMs;
  if (dur > 0) p = Math.min(p, dur);
  return Math.max(0, p);
}

function updateActive() {
  const p = lyrics.get().payload;
  const cur = activeLine.get();
  const curNext = nextLine.get();
  let next: ActiveRange | null = null;
  let upcoming: ActiveRange | null = null;
  if (p && p.format === "timed" && p.trackId === (playback.get().track?.id ?? null)) {
    const pos = nowPosition();
    next = findActive(p.lines, p.userDelayMs, pos);
    upcoming = findNext(p.lines, p.userDelayMs, pos);
  }
  if (cur?.start !== next?.start || cur?.end !== next?.end) activeLine.set(next);
  if (curNext?.start !== upcoming?.start || curNext?.end !== upcoming?.end) nextLine.set(upcoming);
}

function tick() {
  raf = 0;
  updateActive();
  const p = lyrics.get().payload;
  if (playback.get().status === "playing" && p?.format === "timed") raf = requestAnimationFrame(tick);
}

function kick() {
  if (raf) cancelAnimationFrame(raf);
  raf = 0;
  tick();
}

/** 行をクリック（Enter）したとき：その行の表示時刻へシークする（手の操作には数えない） */
export function seekToLine(l: LyricsLine) {
  const p = lyrics.get().payload;
  if (!p || l.timeMs === null) return;
  const ms = shownTime(l, p.userDelayMs);
  void api.seek(ms);
  position.set({ positionMs: ms, durationMs: position.get().durationMs });
}

export function initLyrics() {
  lastPos = position.get().positionMs;
  lastAt = performance.now();
  position.subscribe(() => {
    lastPos = position.get().positionMs;
    lastAt = performance.now();
    kick();
  });
  playback.subscribe(() => {
    const id = playback.get().track?.id ?? null;
    if (id !== lyrics.get().trackId) void loadLyrics(id);
    kick();
  });
  lyrics.subscribe(kick);
  void loadLyrics(playback.get().track?.id ?? null);
}
