// アプリ内ショートカット（既定値・変更・保存）。グローバルホットキーと同じ書き方（Ctrl+Alt+Shift+キー）を使う。
import { useEffect } from "react";
import { api, type PlaybackAction } from "../ipc/commands";
import { settings, toggleLyricsRow } from "../stores/settings";
import { toggleLyricsView } from "../stores/lyrics";

const MODIFIER_CODES = new Set(["ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight"]);

/** キーの名前（e.code から "Key" "Digit" を外したもの） */
export function keyName(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}

/** キーボードの操作を「Ctrl+Shift+M」の形にする。修飾キーだけのときは null */
export function comboFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  parts.push(keyName(e.code));
  return parts.join("+");
}

const KEY_LABEL: Record<string, string> = {
  ArrowRight: "→",
  ArrowLeft: "←",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Space: "Space",
};

export function comboLabel(combo: string): string {
  if (!combo) return "未設定";
  return combo
    .split("+")
    .map((p) => KEY_LABEL[p] ?? p)
    .join(" + ");
}

const ACTION_TO_IPC: Record<string, PlaybackAction> = {
  playPause: "toggle",
  next: "next",
  previous: "previous",
  seekForward: "seekForward",
  seekBackward: "seekBackward",
  volumeUp: "volumeUp",
  volumeDown: "volumeDown",
  mute: "toggleMute",
  shuffle: "toggleShuffle",
  repeat: "cycleRepeat",
  // v3.8.0（仕様 7章・13.5 の11）：詳細表示は Rust が持つので、アプリ内のキーも Rust を通す（グローバルにもできる）
  toggleDetail: "toggleDetail",
  toggleOutput1: "toggleOutput1",
  toggleOutput2: "toggleOutput2",
  toggleOutput3: "toggleOutput3",
};

function typingTarget(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable || !!el.closest?.("[data-capture-keys]");
}

/** 画面全体でショートカットを受ける。検索欄などに文字を打っている間は効かせない */
export function useAppShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat && !["ArrowRight", "ArrowLeft", "ArrowUp", "ArrowDown"].includes(e.code)) return;
      if (typingTarget(e.target)) return;
      const combo = comboFromEvent(e);
      if (!combo) return;
      const map = settings.get().shortcuts;
      const action = Object.keys(map).find((a) => map[a] === combo);
      if (!action) return;
      // 画面の操作（再生の司令塔を通さないもの）
      if (action in UI_ACTIONS) {
        e.preventDefault();
        UI_ACTIONS[action]();
        return;
      }
      const ipc = ACTION_TO_IPC[action];
      if (!ipc) return;
      e.preventDefault();
      void api.action(ipc);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

/** 画面だけで完結する操作（v3：歌詞画面の開閉、v3.0.2：流れる歌詞の列）。グローバルホットキーには出さない */
const UI_ACTIONS: Record<string, () => void> = {
  toggleLyrics: toggleLyricsView,
  toggleLyricsRow: () => void toggleLyricsRow(),
};

/** グローバルホットキーにできる操作（再生の操作だけ） */
export const HOTKEY_ACTIONS = Object.keys(ACTION_TO_IPC);
/** アプリ内ショートカットの操作（再生の操作＋画面の操作） */
export const SHORTCUT_ACTIONS = [...HOTKEY_ACTIONS, ...Object.keys(UI_ACTIONS)];
