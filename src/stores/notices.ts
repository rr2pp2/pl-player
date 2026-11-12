// 画面の右下に出す知らせ（トースト）。
import { on } from "../ipc/events";
import { errorMessage, noticeText } from "../messages/ja";
import { createStore } from "./store";

export interface Toast {
  id: number;
  level: "error" | "info";
  text: string;
  detail: string;
}

export const toasts = createStore<Toast[]>([]);
let seq = 1;

export function notify(n: { level: "error" | "info"; text?: string; detail?: string; error?: unknown }) {
  let text = n.text ?? "";
  let detail = n.detail ?? "";
  if (n.error !== undefined) {
    const m = errorMessage(n.error);
    text = text || m.text;
    detail = detail || m.detail;
  }
  const id = seq++;
  // 同じ文が続いたら重ねない
  const exists = toasts.get().find((t) => t.text === text);
  if (exists) return;
  toasts.set((list) => [...list.slice(-4), { id, level: n.level, text, detail }]);
  setTimeout(() => dismiss(id), n.level === "error" ? 9000 : 5000);
}

export function dismiss(id: number) {
  toasts.set((list) => list.filter((t) => t.id !== id));
}

export async function initNotices() {
  await on("notice", (n) => {
    const m = noticeText(n);
    notify({ level: n.level, text: m.text, detail: m.detail });
  });
}
