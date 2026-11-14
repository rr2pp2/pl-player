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
  // 着せ替え（v3.2.0）
  | "skin_zip_invalid"
  | "skin_zip_encrypted"
  | "skin_too_large"
  | "skin_path_unsafe"
  | "skin_file_not_allowed"
  | "skin_executable"
  | "skin_svg_not_allowed"
  | "skin_css_not_allowed"
  | "skin_font_not_allowed"
  | "skin_manifest_invalid"
  | "skin_schema_unsupported"
  | "skin_placement_unsupported"
  | "skin_asset_missing"
  | "skin_image_invalid"
  | "skin_image_limit"
  | "skin_id_exists"
  | "skin_not_found"
  | "skin_builtin_readonly"
  | "skin_apply_failed"
  | "skin_import_expired"
  // 曲をごみ箱へ移す（v3.3.0）
  | "track_not_in_library"
  | "trash_not_allowed"
  | "trash_unavailable"
  | "file_in_use"
  | "trash_too_many"
  // 古い文字コードのタグの読み直し（v3.5.0）
  | "legacy_reread_busy"
  | "library_import_busy"
  // タグの編集画面の保存（v3.6.0）
  | "track_removed_before_save"
  | "other";

/** v3.3.0：曲をごみ箱へ移した結果。移せなかった曲は、ファイルもライブラリもそのまま */
export interface TrashReport {
  moved: number[];
  failed: { trackId: number; code: ErrorCode; detail: string }[];
  /** ごみ箱へは移せたのに、ライブラリから外せなかった曲（moved にも入る。「ライブラリから外す」で外せる） */
  leftInLibrary: { trackId: number; code: ErrorCode; detail: string }[];
}

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
  | "yearly_top"
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

/** v3.6.0：年間まとめの1曲（Rust の db::history::RankedTrack） */
export interface RankedTrack {
  /** 順位（1 から）。検索で絞っても、絞る前の順位のまま */
  rank: number;
  /** 期間の中で再生した回数 */
  plays: number;
  /** 期間の中で最後に再生した時刻（ミリ秒） */
  lastPlayedAt: number;
  track: Track;
}

/** v3.6.0：年間まとめ（Rust の db::history::YearlyTop）。時刻はどれも 1970-01-01 UTC からのミリ秒 */
export interface YearlyTop {
  since: number;
  until: number;
  /** 数え始めた時刻（3.6.0 に上げて最初に起動した時刻） */
  startedAt: number | null;
  /** 検索で絞る前の曲の数 */
  ranked: number;
  entries: RankedTrack[];
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
  /** v3.3.0：playerButtons・sidebarItems（隠すものだけ false。無いキーは出す）・trashButton（既定はオフ）。古い設定には無い */
  ui: {
    sidebarCollapsed: boolean;
    columns: Record<string, boolean>;
    lyricsRow: boolean;
    playerButtons?: Record<string, boolean>;
    sidebarItems?: Record<string, boolean>;
    trashButton?: boolean;
    /** v3.3.0：設定のタブの並び（空・無ければ既定の順。知らない名前・抜けは画面が合わせる） */
    settingsTabs?: string[];
    /** v3.4.0：曲一覧のコンパクト表示（"off" 使わない・"always" いつも・"narrow" 狭いときだけ）。無ければ使わない。読み方は src\layout\density.ts */
    compactList?: string;
    /** v3.4.0：サイドバーのアイコンの大きさ（px。14〜32）。無ければ 20 */
    sidebarIconPx?: number;
  };
  shortcuts: Record<string, string>;
  hotkeys: Record<string, string>;
  closeToTray: boolean;
  rescanOnStart: boolean;
  /** v3.1.0：歌詞をネットから取る（LRCLIB）。既定はオフ。古い設定には無い（無ければオフ） */
  lyricsOnline?: boolean;
  /** v3.2.0：着せ替え。activeSkinId は skins_apply だけが変える（設定の保存では変わらない） */
  skin: SkinSettings;
}

export interface SkinSettings {
  activeSkinId: string;
  characterVisible: boolean;
  /** 0.75〜1.25 */
  characterScale: number;
  /** 次の起動だけ既定の見た目にする */
  safeModeNextStart: boolean;
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

/** v3.5.0：古い文字コードのタグの読み直しの進み具合と結果（Rust 側 library/legacy_reread.rs の RereadProgress と同じ形） */
export interface LegacyRereadProgress {
  running: boolean;
  finished: boolean;
  /** 「やめる」で止めた */
  cancelled: boolean;
  /** 始めたときのライブラリの曲数 */
  total: number;
  /** 見た曲の数 */
  done: number;
  /** 値を直した曲の数 */
  fixed: number;
  /** 手で直した印のある項目なので、直さなかった曲の数 */
  skippedManual: number;
  /** 読み直しの途中で値が変わった・ライブラリから外されたので、書かなかった曲の数 */
  changedDuringRun: number;
  /** 途中で止まったときのエラーの種別（それまでに直した曲はそのまま） */
  error: ErrorCode | null;
}

export interface LyricsFetchOne {
  trackId: number;
  /** synced / plain / instrumental / not_found / too_large / kept / no_metadata / manual / error / stopped */
  status: string;
  error: string | null;
}

// ---- 着せ替え（v3.2.0）。Rust 側 skins/package.rs・skins/runtime.rs と同じ形 ----

/** 画像。パスは Rust が確かめた skins の下の場所だけ（画面は fileUrl で URL にするだけ） */
export interface SkinAsset {
  path: string;
  /** 動く画像の最初の1コマ（「動きを減らす」のとき使う）。静止画は null */
  still: string | null;
  width: number;
  height: number;
  animated: boolean;
}

export interface SkinThemedAsset {
  dark: SkinAsset | null;
  light: SkinAsset | null;
}

export interface SkinThemeAppearance {
  tokens: Record<string, string>;
  shadow: "none" | "soft" | "standard" | null;
}

export interface SkinAppearance {
  dark: SkinThemeAppearance | null;
  light: SkinThemeAppearance | null;
  fontFamily: string[];
  tokens: Record<string, string>;
}

export interface SkinCharacter {
  playing: SkinAsset;
  stopped: SkinAsset;
  widthPx: number;
  maxHeightPx: number;
  minWindowWidthPx: number;
}

export interface ResolvedSkin {
  id: string;
  name: string;
  version: string;
  author: string;
  appearance: SkinAppearance | null;
  /** アイコンの名前 → 状態 → 画像 */
  icons: Record<string, Record<string, SkinThemedAsset>>;
  hasApplication: boolean;
  character: SkinCharacter | null;
}

export interface SkinNotice {
  /** broken_fallback / safe_mode / safe_mode_second */
  code: string;
  id: string | null;
  error: ErrorCode | null;
}

export interface SkinState {
  activeId: string;
  effectiveId: string;
  safeMode: boolean;
  skin: ResolvedSkin | null;
  notice: SkinNotice | null;
}

export interface SkinParts {
  appearance: boolean;
  icons: number;
  application: boolean;
  character: boolean;
  animated: boolean;
}

export interface SkinSummary {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  builtin: boolean;
  parts: SkinParts;
  preview: string | null;
  /** 読めないスキン（消すことだけできる） */
  broken: ErrorCode | null;
}

export interface SkinWarning {
  code: string;
  detail: string;
}

export interface SkinFileRow {
  path: string;
  bytes: number;
  width: number;
  height: number;
  frames: number;
  durationMs: number;
  animated: boolean;
}

export interface SkinInspection {
  token: string;
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  minAppVersion: string | null;
  parts: SkinParts;
  files: SkinFileRow[];
  zipBytes: number;
  unpackedBytes: number;
  fileCount: number;
  maxFiles: number;
  /** 種類ごとに3件まで（Rust の WARNINGS_PER_CODE） */
  warnings: SkinWarning[];
  /** 上の上限を超えて出さなかった数（種類ごと）。画面は「ほか N 件」と出す */
  warningsOmitted: { code: string; count: number }[];
  /** 飛ばした OS のファイル（`__MACOSX/`・`.DS_Store`・`Thumbs.db`・`desktop.ini` の種類ごとの数） */
  ignored: { name: string; count: number }[];
  /** フォルダごと zip にした形だったので、そのフォルダを外して読んだ */
  unwrapped: boolean;
  /** 同じ id のスキンが入っている（取り込めない） */
  conflict: boolean;
  previewDataUrl: string | null;
  appearance: SkinAppearance | null;
}

export interface SkinExportReport {
  files: number;
  bytes: number;
}
