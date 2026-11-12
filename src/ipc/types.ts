// Rust 側（serde, camelCase）と同じ形。

export type ErrorCode =
  | "file_not_found"
  | "access_denied"
  | "unsupported_format"
  | "tag_read_failed"
  | "tag_write_failed"
  | "device_disconnected"
  | "playback_failed"
  | "database"
  | "lyrics_format_invalid"
  | "lyrics_encoding_unsupported"
  | "lyrics_online_disabled"
  | "lyrics_online_busy"
  | "network_path_unsupported"
  | "other";

export interface AppError {
  code: ErrorCode;
  detail: string;
}

export interface Track {
  id: number;
  path: string;
  format: string;
  durationMs: number;
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  genre: string;
  year: number | null;
  trackNo: number | null;
  discNo: number | null;
  composer: string;
  comment: string;
  hasArt: boolean;
  artPath: string | null;
  albumKey: string;
  addedAt: number;
  lastPlayedAt: number | null;
  playCount: number;
  favorite: boolean;
  missing: boolean;
  tagError: boolean;
}

export type ViewKind =
  | "all"
  | "artist"
  | "album"
  | "genre"
  | "recent_added"
  | "recent_played"
  | "favorites"
  | "playlist"
  | "queue"
  | "settings"
  | "lyrics";

export interface TrackQuery {
  view: string;
  key?: string | null;
  search?: string | null;
  sort?: string | null;
  desc?: boolean | null;
}

export interface Group {
  key: string;
  name: string;
  sub: string;
  count: number;
  artPath: string | null;
}

export interface Playlist {
  id: number;
  name: string;
  count: number;
}

export interface PlaylistEntry {
  itemId: number;
  track: Track;
}

export type Status = "stopped" | "playing" | "paused";
export type Repeat = "off" | "all" | "one";

export interface Snapshot {
  status: Status;
  track: Track | null;
  positionMs: number;
  durationMs: number;
  volume: number;
  muted: boolean;
  shuffle: boolean;
  repeat: Repeat;
  deviceId: string | null;
  deviceName: string;
  queueLen: number;
  contextLen: number;
}

export interface Notice {
  level: "error" | "info";
  code: string;
  detail: string;
  from: string | null;
  to: string | null;
}

export interface DeviceInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface ImportProgress {
  done: number;
  total: number;
  current: string;
}

export interface ImportSummary {
  added: number;
  updated: number;
  unchanged: number;
  unsupported: string[];
  tagErrors: string[];
  notFound: string[];
  accessDenied: string[];
  otherErrors: string[];
  /** v3.1.1：ネットワーク上の場所（\\ で始まる・ネットワークドライブ）なので取り込まなかったもの */
  network: string[];
  elapsedMs: number;
}

export interface TagValues {
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  genre: string;
  year: number | null;
  trackNo: number | null;
  discNo: number | null;
  composer: string;
  comment: string;
}

export interface MetadataView {
  track: Track;
  values: TagValues;
  albumOverride: string | null;
}

export type ArtChange =
  | { kind: "keep" }
  | { kind: "set"; path: string }
  | { kind: "remove" };

export interface Background {
  path: string | null;
  dim: number;
  blur: number;
  uiOpacity: number;
  fit: "cover" | "contain";
}

export interface AppSettings {
  theme: string;
  background: Background;
  /** lyricsRow：再生バーの下の流れる歌詞の列（v3.0.2。既定はオン） */
  ui: { sidebarCollapsed: boolean; columns: Record<string, boolean>; lyricsRow: boolean };
  shortcuts: Record<string, string>;
  hotkeys: Record<string, string>;
  closeToTray: boolean;
  rescanOnStart: boolean;
  /** v3.1.0：歌詞をネットから取る（LRCLIB）。既定はオフ。古い設定には無い（無ければオフ） */
  lyricsOnline?: boolean;
}

export interface HotkeyStatus {
  action: string;
  accelerator: string;
  registered: boolean;
  error: string | null;
}

export interface BackupReport {
  playlists: number;
  playlistTracks: number;
  playlistTracksMissing: number;
  favorites: number;
  favoritesMissing: number;
  folders: number;
  background: boolean;
  /** v3.1.1：ネットワーク上の場所なので入れなかったライブラリのフォルダ */
  foldersRejected: number;
  /** v3.1.1：zip の背景が画像として読めなかったので入れなかった */
  backgroundRejected: boolean;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  trackCount: number;
}

export interface FolderEntry {
  path: string;
  recursive: boolean;
}

// ---- 歌詞（v3）。Rust 側 lyrics/service.rs・lyrics/inbox.rs と同じ形 ----

export type LyricsSourceKind = "manual" | "external" | "embedded_tag";
export type LyricsFormat = "timed" | "plain";

export interface LyricsLine {
  key: string;
  /** 元の時刻 − offset（0 未満もあり得る）。表示時刻は max(0, timeMs + 曲ごとの補正)（stores/lyricsTime.ts） */
  timeMs: number | null;
  text: string;
  kind: "lyric" | "clear" | "untimed";
}

export interface LyricsPayload {
  trackId: number;
  /** null は歌詞なし */
  source: LyricsSourceKind | null;
  format: LyricsFormat | null;
  rawText: string;
  lines: LyricsLine[];
  plainText: string;
  userDelayMs: number;
  /** 警告のコード。文は messages/ja.ts */
  warnings: string[];
}

export interface LyricsPreview {
  format: LyricsFormat;
  lineCount: number;
  hasContent: boolean;
  tooLarge: boolean;
  warnings: string[];
}

export interface InboxReport {
  imported: number;
  manualKept: number;
  unknownTrack: number;
  duplicate: number;
  encoding: number;
  emptyOrTooLarge: number;
  keptInInbox: number;
  ignored: number;
  /** v3.1.1：行が多すぎて、先頭の行だけを表示するもの */
  linesTruncated: number;
  trackIds: number[];
}

// ---- 歌詞をネットから取る（v3.1.0）。Rust 側 lyrics/online.rs と同じ形。件数と状態だけ（歌詞の本文は来ない） ----

export type LyricsFetchMode = "missing" | "refetch";

export interface LyricsFetchProgress {
  running: boolean;
  finished: boolean;
  /** 送る曲を選んでいる途中（この間は prepareDone / prepareTotal で進み具合を出す） */
  preparing: boolean;
  prepareTotal: number;
  prepareDone: number;
  mode: LyricsFetchMode | null;
  /** 今回送る予定の曲数 */
  total: number;
  done: number;
  synced: number;
  plain: number;
  instrumental: number;
  notFound: number;
  tooLarge: number;
  errors: number;
  /** 取り直しで取れなかったので、今の外部の歌詞を残した曲 */
  kept: number;
  /** 曲のタグに歌詞があるので送らなかった曲 */
  hasTag: number;
  /** 曲名かアーティストが空で送らなかった曲 */
  noMetadata: number;
  /** 前に見つからなかったので聞き直さなかった曲 */
  skippedKnown: number;
  /** 一括の途中でライブラリから外された・手動の歌詞が付いたので送らなかった曲（エラーに数えない） */
  changedDuringRun: number;
  /** 最後にエラーになった曲の記号（打ち切りの文言を原因ごとに分けるため） */
  lastError: string | null;
  requests: number;
  aborted: string | null;
}

export interface LyricsFetchDone extends LyricsFetchProgress {
  trackIds: number[];
}

export interface LyricsFetchOne {
  trackId: number;
  /** synced / plain / instrumental / not_found / too_large / kept / no_metadata / manual / error / stopped */
  status: string;
  error: string | null;
}
