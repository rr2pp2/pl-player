// invoke を呼ぶのはここだけ。型付きの関数にして外へ出す。
import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  AppSettings,
  ArtChange,
  AvatarList,
  AvatarSummary,
  BackupReport,
  CaptureSource,
  DeviceInfo,
  FolderEntry,
  Group,
  HotkeyStatus,
  InboxReport,
  LatencyReport,
  LatencySetup,
  LegacyRereadProgress,
  LyricsFetchMode,
  LyricsFetchOne,
  LyricsFetchProgress,
  LyricsPayload,
  LyricsPreview,
  MeterFrame,
  MetadataView,
  OutputIcon,
  OutputsView,
  PickedImage,
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
  ViewMode,
  YearlyTop,
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
  | "seekBackward"
  // v3.8.0：詳細表示の切替・出力 1〜3 のオン/オフ（Rust の playback_action と hotkeys の dispatch が同じ関数を呼ぶ）
  | "toggleDetail"
  | "toggleOutput1"
  | "toggleOutput2"
  | "toggleOutput3";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),

  // ライブラリ
  libraryQuery: (query: TrackQuery) => invoke<Track[]>("library_query", { query }),
  /** v3.6.0：年間まとめ（画面を開いた時点の今から 365 日さかのぼって、再生した回数の上位 100 曲） */
  libraryYearlyTop: (search?: string | null) => invoke<YearlyTop>("library_yearly_top", { search: search ?? null }),
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
  /** v3.5.0：古い文字コードのタグを読み直す（DB の値だけを直す。曲ファイルには書かない）。取り込み・再走査の途中なら断る */
  legacyRereadStart: () => invoke<LegacyRereadProgress>("legacy_reread_start"),
  legacyRereadCancel: () => invoke<void>("legacy_reread_cancel"),
  legacyRereadStatus: () => invoke<LegacyRereadProgress>("legacy_reread_status"),

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
    // v3.6.0：曲ファイルには書いたが、その間に曲がライブラリから外された（ライブラリには戻していない）ときは null
    invoke<Track | null>("metadata_write", { id, values, art }),
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

  // 同時再生（v3.8.0）。出力先の設定を書くのは Rust だけ。変わるたびに pl://outputs で全部が来る
  outputsGet: () => invoke<OutputsView>("outputs_get"),
  /** 出力先を足す（null は既定のデバイス）。足した直後はオフ。足した枠の番号を返す。3つあると output_invalid */
  outputAdd: (deviceId: string | null) => invoke<number>("output_add", { deviceId }),
  /** 外す（確認なし。設定は覚えておき、同じデバイスを足したら戻る） */
  outputRemove: (slot: number) => invoke<void>("output_remove", { slot }),
  outputSetOn: (slot: number, on: boolean) => invoke<void>("output_set_on", { slot, on }),
  /** 出力ごとの音量（動かしている間）。離したら outputCommitVolume */
  outputSetVolume: (slot: number, volume: number) => invoke<void>("output_set_volume", { slot, volume }),
  outputCommitVolume: () => invoke<void>("output_commit_volume"),
  /** 遅らせ量（0〜1000ms の整数）。その出力だけ約150ms 後に入り直す */
  outputSetDelay: (slot: number, delayMs: number) =>
    invoke<void>("output_set_delay", { slot, delayMs: Math.max(0, Math.min(1000, Math.round(delayMs))) }),
  outputSetFollow: (slot: number, follow: boolean) => invoke<void>("output_set_follow", { slot, follow }),
  /** アイコンの選び直し（null で自動に戻す） */
  outputSetIcon: (slot: number, icon: OutputIcon | null) => invoke<void>("output_set_icon", { slot, icon }),
  /** デバイスの差し替え（null は既定のデバイス）。前のデバイスの設定は覚える */
  outputSetDevice: (slot: number, deviceId: string | null) => invoke<void>("output_set_device", { slot, deviceId }),
  /** 表示のモード（通常／詳細）。音量の組がそのモードのものに入れ替わる */
  viewModeSet: (mode: ViewMode) => invoke<void>("view_mode_set", { mode }),
  /**
   * メーターを受け始める（メーターが出ている間だけ）。止めるときは meterStop。次の meterStart でも前のものは止まる。
   * seq は開始・停止のたびに増やす通し番号（実行が入れ替わっても、いちばん新しい番号だけが効く。Rust の MeterControl）
   */
  meterStart: (seq: number, onFrame: (f: MeterFrame) => void) => {
    const ch = new Channel<MeterFrame>();
    ch.onmessage = onFrame;
    return invoke<void>("meter_start", { seq, onFrame: ch });
  },
  meterStop: (seq: number) => invoke<void>("meter_stop", { seq }),
  // 遅れを測る（v3.8.0 段3）。曲を流している間・ほかの測定の間は、Rust が始めずに abort で返す
  latencySetup: () => invoke<LatencySetup>("latency_setup"),
  /**
   * 測る（通常は約 6 秒）。run は測るたびに増やす番号（0 は使わない）。choices は録る所を選び直した出力だけ
   */
  latencyMeasure: (run: number, choices: { slot: number; capture: CaptureSource }[]) =>
    invoke<LatencyReport>("latency_measure", { run, choices }),
  /** やめる。run は latencyMeasure に渡した番号（まだ始まっていなくても、その測定に効く） */
  latencyCancel: (run: number) => invoke<void>("latency_cancel", { run }),
  /** 遅らせ量にまとめて入れる。測ったときと同じデバイスの枠にだけ入る。入れた枠の番号を返す */
  latencyApply: (items: { slot: number; deviceId: string | null; delayMs: number }[]) => invoke<number[]>("latency_apply", { items }),

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
  /** 「既定の見た目に戻す」：スキンを既定にし、手で選んだ色も外す（v3.10.0）。立ち絵は残す */
  skinsReset: () => invoke<SkinState>("skins_reset"),
  /** v3.10.0：手で選んだ色を1つ変える（null なら外す）。Rust が確かめて形をそろえ、保存した設定を返す。決まりに合わなければ skin_color_invalid */
  skinsColorSet: (name: string, value: string | null) => invoke<AppSettings>("skins_color_set", { name, value }),
  /** v3.10.0：手で選んだ色を全部外す（スキンと立ち絵は変えない） */
  skinsColorsClear: () => invoke<AppSettings>("skins_colors_clear"),
  skinsExitSafeMode: () => invoke<SkinState>("skins_exit_safe_mode"),
  skinsDelete: (id: string) => invoke<SkinState>("skins_delete", { id }),
  skinsExport: (id: string) => invoke<SkinExportReport | null>("skins_export", { id }),

  // 立ち絵（v3.9.0）。画面からパスは渡さない・受け取らない（画像は Rust がファイルを選ぶ画面を出し、確かめた結果を印 token で返す。立ち絵は id で指す）
  avatarsList: () => invoke<AvatarList>("avatars_list"),
  avatarsPickImage: (slot: "playing" | "stopped") => invoke<PickedImage | null>("avatars_pick_image", { slot }),
  avatarsCancelPick: () => invoke<void>("avatars_cancel_pick"),
  avatarsCreate: (name: string, playing: string, stopped: string | null) => invoke<AvatarSummary>("avatars_create", { name, playing, stopped }),
  /** 作った立ち絵を変える（v3.9.0 直し2回目）。playing・stopped は差し替える画像の印（null ならそのまま）、removeStopped なら停止中を外す。名前が空なら今の名前のまま */
  avatarsUpdate: (id: string, name: string, playing: string | null, stopped: string | null, removeStopped: boolean) =>
    invoke<AvatarSummary>("avatars_update", { id, name, playing, stopped, removeStopped }),
  avatarsDelete: (id: string) => invoke<SkinState>("avatars_delete", { id }),
  /** "skin"・"none"・"avatar:<id>"・"skin:<id>" */
  avatarsUse: (choice: string) => invoke<SkinState>("avatars_use", { choice }),
  /** 立ち絵を出さない（avatars_use の "none" と同じ。設定の「既定の見た目に戻す」の隣のボタン） */
  avatarsHide: () => invoke<SkinState>("avatars_hide"),
};
