// event を受けるのはここだけ。名前は Rust 側（commands/mod.rs の ev）と同じ。
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import type {
  AppSettings,
  HotkeyStatus,
  ImportProgress,
  ImportSummary,
  LegacyRereadProgress,
  LyricsFetchDone,
  LyricsFetchProgress,
  Notice,
  OutputsView,
  SkinState,
  Snapshot,
} from "./types";

export const EV = {
  playback: "pl://playback",
  position: "pl://position",
  queue: "pl://queue",
  notice: "pl://notice",
  devices: "pl://devices",
  library: "pl://library",
  playlists: "pl://playlists",
  importProgress: "pl://import-progress",
  importDone: "pl://import-done",
  hotkeys: "pl://hotkeys",
  settings: "pl://settings",
  lyricsFetchProgress: "pl://lyrics-fetch-progress",
  lyricsFetchDone: "pl://lyrics-fetch-done",
  skin: "pl://skin",
  legacyRereadProgress: "pl://legacy-reread-progress",
  legacyRereadDone: "pl://legacy-reread-done",
  // v3.8.0：出力先の設定・状態・表示のモード（変わるたびに全部）
  outputs: "pl://outputs",
} as const;

type Handlers = {
  playback: Snapshot;
  position: { positionMs: number; durationMs: number };
  queue: null;
  notice: Notice;
  devices: null;
  library: null;
  playlists: null;
  importProgress: ImportProgress;
  importDone: ImportSummary;
  hotkeys: HotkeyStatus[];
  settings: AppSettings;
  lyricsFetchProgress: LyricsFetchProgress;
  lyricsFetchDone: LyricsFetchDone;
  skin: SkinState;
  legacyRereadProgress: LegacyRereadProgress;
  legacyRereadDone: LegacyRereadProgress;
  outputs: OutputsView;
};

export function on<K extends keyof Handlers>(name: K, cb: (payload: Handlers[K]) => void): Promise<UnlistenFn> {
  return listen<Handlers[K]>(EV[name], (e) => cb(e.payload));
}

/**
 * ローカルの画像ファイルを画面で表示できる URL にする（asset プロトコル。読めるのはデータフォルダの artwork・backgrounds・skins だけ）。
 * v3.2.0：スキンの画像のパスは、Rust が確かめて返したもの（skins の下）だけを渡す
 */
export function fileUrl(path: string | null | undefined): string | null {
  if (!path) return null;
  return convertFileSrc(path);
}
