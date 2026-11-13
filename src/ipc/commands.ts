// invoke を呼ぶのはここだけ。型付きの関数にして外へ出す。
import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  AppSettings,
  ArtChange,
  BackupReport,
  DeviceInfo,
  FolderEntry,
  Group,
  HotkeyStatus,
  InboxReport,
  LyricsFetchMode,
  LyricsFetchOne,
  LyricsFetchProgress,
  LyricsPayload,
  LyricsPreview,
  MetadataView,
  Playlist,
  PlaylistEntry,
  TrashReport,
  SkinExportReport,
  SkinInspection,
  SkinState,
  SkinSummary,
  Snapshot,
  TagValues,
  Track,
  TrackQuery,
} from "./types";

export type PlaybackAction =
  | "toggle"
  | "play"
  | "pause"
  | "next"
  | "previous"
  | "toggleMute"
  | "toggleShuffle"
  | "cycleRepeat"
  | "volumeUp"
  | "volumeDown"
  | "seekForward"
  | "seekBackward";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),

  // ライブラリ
  libraryQuery: (query: TrackQuery) => invoke<Track[]>("library_query", { query }),
  libraryGroups: (kind: string, search?: string | null) =>
    invoke<Group[]>("library_groups", { kind, search: search || null }),
  trackGet: (id: number) => invoke<Track | null>("track_get", { id }),
  tracksGet: (ids: number[]) => invoke<Track[]>("tracks_get", { ids }),
  setFavorite: (ids: number[], favorite: boolean) => invoke<void>("set_favorite", { ids, favorite }),
  libraryImport: (paths: string[]) => invoke<void>("library_import", { paths }),
  libraryPickFiles: () => invoke<boolean>("library_pick_files"),
  libraryPickFolder: () => invoke<boolean>("library_pick_folder"),
  libraryFolders: () => invoke<FolderEntry[]>("library_folders"),
  libraryRemoveFolder: (path: string) => invoke<void>("library_remove_folder", { path }),
  libraryRescan: () => invoke<void>("library_rescan"),
  libraryRemoveTracks: (ids: number[]) => invoke<void>("library_remove_tracks", { ids }),
  /** v3.3.0：曲ファイルをごみ箱へ移し、移せた曲をライブラリから外す（曲の id だけを渡す） */
  libraryTrashTracks: (ids: number[]) => invoke<TrashReport>("library_trash_tracks", { ids }),
  revealInExplorer: (path: string) => invoke<void>("reveal_in_explorer", { path }),

  // 再生
  snapshot: () => invoke<Snapshot>("playback_snapshot"),
  playContext: (ids: number[], index: number) => invoke<void>("play_context", { ids, index }),
  action: (action: PlaybackAction) => invoke<void>("playback_action", { action }),
  seek: (ms: number) => invoke<void>("playback_seek", { ms: Math.max(0, Math.round(ms)) }),
  setVolume: (volume: number) => invoke<void>("playback_set_volume", { volume }),
  commitVolume: () => invoke<void>("playback_commit_volume"),
  setMuted: (muted: boolean) => invoke<void>("playback_set_muted", { muted }),
  setShuffle: (shuffle: boolean) => invoke<void>("playback_set_shuffle", { shuffle }),
  setRepeat: (repeat: string) => invoke<void>("playback_set_repeat", { repeat }),

  // キュー
  queueGet: () => invoke<Track[]>("queue_get"),
  queueAdd: (ids: number[]) => invoke<void>("queue_add", { ids }),
  queuePlayNext: (ids: number[]) => invoke<void>("queue_play_next", { ids }),
  queueRemove: (positions: number[]) => invoke<void>("queue_remove", { positions }),
  queueMove: (from: number, to: number) => invoke<void>("queue_move", { from, to }),
  queueClear: () => invoke<void>("queue_clear"),
  queuePlayAt: (position: number) => invoke<void>("queue_play_at", { position }),

  // プレイリスト
  playlists: () => invoke<Playlist[]>("playlists_list"),
  playlistCreate: (name: string, trackIds?: number[]) =>
    invoke<number>("playlist_create", { name, trackIds: trackIds ?? null }),
  playlistRename: (id: number, name: string) => invoke<void>("playlist_rename", { id, name }),
  playlistDelete: (id: number) => invoke<void>("playlist_delete", { id }),
  playlistAddTracks: (id: number, trackIds: number[]) => invoke<void>("playlist_add_tracks", { id, trackIds }),
  playlistRemoveItems: (id: number, itemIds: number[]) => invoke<void>("playlist_remove_items", { id, itemIds }),
  playlistMoveItem: (id: number, itemId: number, to: number) =>
    invoke<void>("playlist_move_item", { id, itemId, to }),
  playlistEntries: (id: number) => invoke<PlaylistEntry[]>("playlist_entries", { id }),
  playlistPlay: (id: number, index?: number | null, shuffle?: boolean | null) =>
    invoke<void>("playlist_play", { id, index: index ?? null, shuffle: shuffle ?? null }),

  // メタデータ・アート
  metadataRead: (id: number) => invoke<MetadataView>("metadata_read", { id }),
  metadataWrite: (id: number, values: TagValues, art: ArtChange) =>
    invoke<Track>("metadata_write", { id, values, art }),
  pickImage: (title?: string) => invoke<string | null>("pick_image", { title: title ?? null }),
  stageImage: (path: string) => invoke<string>("stage_image", { path }),
  albumArtSetExternal: (albumKey: string, imagePath: string) =>
    invoke<string>("album_art_set_external", { albumKey, imagePath }),
  albumArtClearExternal: (albumKey: string) => invoke<void>("album_art_clear_external", { albumKey }),

  // 設定
  settingsGet: () => invoke<AppSettings>("settings_get"),
  settingsSet: (settings: AppSettings) => invoke<AppSettings>("settings_set", { settings }),
  hotkeysStatus: () => invoke<HotkeyStatus[]>("hotkeys_status"),
  backgroundSet: (path: string) => invoke<AppSettings>("background_set", { path }),
  backgroundClear: () => invoke<AppSettings>("background_clear"),
  settingsExport: () => invoke<BackupReport | null>("settings_export"),
  settingsImport: () => invoke<BackupReport | null>("settings_import"),

  // デバイス
  devices: () => invoke<DeviceInfo[]>("devices_list"),
  setDevice: (id: string | null) => invoke<void>("device_set", { id }),

  // 歌詞（v3）。曲ファイルには書き込まない
  getLyrics: (trackId: number) => invoke<LyricsPayload>("get_lyrics", { trackId }),
  previewLyrics: (rawText: string) => invoke<LyricsPreview>("preview_lyrics", { rawText }),
  saveManualLyrics: (trackId: number, rawText: string) => invoke<LyricsPayload>("save_manual_lyrics", { trackId, rawText }),
  deleteManualLyrics: (trackId: number) => invoke<LyricsPayload>("delete_manual_lyrics", { trackId }),
  setLyricsDelay: (trackId: number, delayMs: number) => invoke<number>("set_lyrics_delay", { trackId, delayMs: Math.round(delayMs) }),
  importLyricsInbox: () => invoke<InboxReport>("import_lyrics_inbox"),
  openLyricsInbox: () => invoke<string>("open_lyrics_inbox"),

  // 歌詞をネットから取る（v3.1.0）。設定がオフなら Rust 側が入口で断る（lyrics_online_disabled）
  lyricsFetchStart: (mode: LyricsFetchMode) => invoke<LyricsFetchProgress>("lyrics_fetch_start", { mode }),
  lyricsFetchCancel: () => invoke<void>("lyrics_fetch_cancel"),
  lyricsFetchStatus: () => invoke<LyricsFetchProgress>("lyrics_fetch_status"),
  lyricsFetchTrack: (trackId: number) => invoke<LyricsFetchOne>("lyrics_fetch_track", { trackId }),

  // 着せ替え（v3.2.0）。画面からパスは渡さない（zip は Rust がファイルを選ぶ画面を出す。スキンは id、取り込みは確認の印で指す）
  skinsState: () => invoke<SkinState>("skins_state"),
  skinsList: () => invoke<SkinSummary[]>("skins_list"),
  skinsPickAndInspect: () => invoke<SkinInspection | null>("skins_pick_and_inspect"),
  skinsImportCommit: (token: string) => invoke<SkinSummary>("skins_import_commit", { token }),
  skinsImportCancel: (token: string) => invoke<void>("skins_import_cancel", { token }),
  skinsApply: (id: string) => invoke<SkinState>("skins_apply", { id }),
  skinsReset: () => invoke<SkinState>("skins_reset"),
  skinsExitSafeMode: () => invoke<SkinState>("skins_exit_safe_mode"),
  skinsDelete: (id: string) => invoke<SkinState>("skins_delete", { id }),
  skinsExport: (id: string) => invoke<SkinExportReport | null>("skins_export", { id }),
};
