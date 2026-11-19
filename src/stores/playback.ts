// 再生の「表示用の写し」。正しい状態は Rust 側にある。
import { api } from "../ipc/commands";
import { on } from "../ipc/events";
import type { Snapshot } from "../ipc/types";
import { createStore } from "./store";

export const playback = createStore<Snapshot>({
  status: "stopped",
  track: null,
  positionMs: 0,
  durationMs: 0,
  volume: 0.8,
  muted: false,
  shuffle: false,
  repeat: "off",
  queueLen: 0,
  contextLen: 0,
});

/** 再生位置だけは頻繁に変わるので、別の入れ物にする（再描画を小さくするため） */
export const position = createStore<{ positionMs: number; durationMs: number }>({ positionMs: 0, durationMs: 0 });

export async function initPlayback() {
  const s = await api.snapshot();
  playback.set(s);
  position.set({ positionMs: s.positionMs, durationMs: s.durationMs });
  await on("playback", (s) => {
    playback.set(s);
    position.set({ positionMs: s.positionMs, durationMs: s.durationMs || position.get().durationMs });
  });
  await on("position", (p) => position.set(p));
}
