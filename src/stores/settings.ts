// 設定の写し。変更は Rust 側へ保存してから反映する。
import { api } from "../ipc/commands";
import { on } from "../ipc/events";
import type { AppSettings, HotkeyStatus } from "../ipc/types";
import { createStore } from "./store";
import { notify } from "./notices";

export const DEFAULT_SHORTCUTS: Record<string, string> = {
  playPause: "Space",
  next: "Ctrl+ArrowRight",
  previous: "Ctrl+ArrowLeft",
  seekForward: "ArrowRight",
  seekBackward: "ArrowLeft",
  volumeUp: "Ctrl+ArrowUp",
  volumeDown: "Ctrl+ArrowDown",
  mute: "M",
  shuffle: "S",
  repeat: "R",
  // v3：歌詞画面を開く／閉じる（アプリ内だけ）
  toggleLyrics: "Ctrl+L",
  // v3.0.2：流れる歌詞の列を出す／隠す（アプリ内だけ）
  toggleLyricsRow: "Ctrl+Shift+L",
};

export const settings = createStore<AppSettings>({
  theme: "dark",
  background: { path: null, dim: 0.45, blur: 6, uiOpacity: 0.7, fit: "cover" },
  ui: {
    sidebarCollapsed: false,
    columns: { album: true, genre: true, duration: true, favorite: true },
    lyricsRow: true,
    // v3.3.0：隠すもの（無いキーは出す）・ごみ箱へ移すボタン（既定はオフ）・設定のタブの並び（空なら既定の順）
    playerButtons: {},
    sidebarItems: {},
    trashButton: false,
    settingsTabs: [],
    // v3.4.0：曲一覧のコンパクト表示（既定は使わない）・サイドバーのアイコンの大きさ（既定は 3.3.0 までの 20px）
    compactList: "off",
    sidebarIconPx: 20,
  },
  shortcuts: { ...DEFAULT_SHORTCUTS },
  hotkeys: {},
  closeToTray: true,
  rescanOnStart: false,
  lyricsOnline: false,
  skin: { activeSkinId: "builtin:default", characterVisible: true, characterScale: 1, safeModeNextStart: false },
});

export const hotkeyStatus = createStore<HotkeyStatus[]>([]);

export async function initSettings() {
  settings.set(await api.settingsGet());
  await on("settings", (s) => settings.set(s));
  await on("hotkeys", (st) => {
    hotkeyStatus.set(st);
    const bad = st.filter((h) => !h.registered);
    if (bad.length > 0) {
      notify({ level: "error", text: "グローバルホットキーを登録できないキーがあります。設定画面で確かめてください。", detail: bad.map((b) => b.accelerator).join("、") });
    }
  });
  try {
    hotkeyStatus.set(await api.hotkeysStatus());
  } catch {
    /* 起動直後は無視 */
  }
}

/** 一部だけ変えて保存する */
export async function updateSettings(patch: (s: AppSettings) => AppSettings) {
  const next = patch(structuredClone(settings.get()));
  settings.set(next); // 先に画面へ反映（すぐ動いて見えるように）
  try {
    settings.set(await api.settingsSet(next));
  } catch (e) {
    notify({ level: "error", error: e });
  }
}

/** 流れる歌詞の列（v3.0.2）を出しているか。古い設定で値が無ければオン */
export const lyricsRowOn = (s: AppSettings): boolean => s.ui.lyricsRow ?? true;

/** 流れる歌詞の列のオン・オフ（設定として保存し、次に起動したときも同じにする） */
export function toggleLyricsRow() {
  return updateSettings((x) => ({ ...x, ui: { ...x.ui, lyricsRow: !lyricsRowOn(x) } }));
}

/** 歌詞をネットから取る（v3.1.0）か。既定はオフ。古い設定で値が無ければオフ */
export const lyricsOnlineOn = (s: AppSettings): boolean => s.lyricsOnline === true;
