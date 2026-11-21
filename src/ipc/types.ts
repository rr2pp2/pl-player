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
  // 手で選ぶ色（v3.10.0）
  | "skin_color_invalid"
  // 立ち絵（v3.9.0。画像から追加する立ち絵）
  | "avatar_image_format"
  | "avatar_too_large"
  | "avatar_name_invalid"
  | "avatar_limit"
  | "avatar_not_found"
  | "avatar_broken"
  | "avatar_pick_expired"
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
  | "output_invalid"
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
  /** v3.8.0：Windows が教える機器の種類から決めた自動のアイコン */
  kind: OutputIcon;
}

// ---- 同時再生（v3.8.0）。形は Rust の playback\output_control.rs・playback\outputs.rs と同じ ----

/** 表示のモード（コンパクトは表示の設定の側なので、ここでは通常と同じ） */
export type ViewMode = "normal" | "detail";

/** 出力先のアイコン（選び直せるもの。仕様 4.3） */
export type OutputIcon = "headphones" | "speaker" | "stream" | "monitor";

/** 出力先の状態（仕様 6.4） */
export type OutputStatus = "off" | "ok" | "disconnected" | "unavailable" | "duplicate";

/** 「開けません」の理由：形式が使えない／ほかのアプリが独り占め／それ以外 */
export type OutputUnavailableReason = "format" | "busy" | "other";

/** 通常用・詳細用の2組 */
export interface PerMode {
  normal: number;
  detail: number;
}

export interface OutputSlotView {
  /** 枠の番号（0〜2。ショートカットの「出力1〜3」は 0〜2） */
  slot: number;
  /** null なら既定のデバイス（Windows に従う） */
  deviceId: string | null;
  /** 保存してある名前（切断中の行に出す。3.7.0 から引き継いだ行は空＝「前に使っていたデバイス」） */
  savedName: string;
  /** 実際のデバイスの名前（開けた・一覧で見つかったとき。既定のデバイスなら今の既定の名前） */
  deviceName: string | null;
  on: boolean;
  delayMs: number;
  /** 選び直したアイコン（null なら autoIcon） */
  icon: OutputIcon | null;
  autoIcon: OutputIcon;
  /** 今のモードの出力ごとの音量（0〜1） */
  volume: number;
  volumes: PerMode;
  /** 全体の音量に従う（詳細でだけ効く。通常ではいつも従う） */
  followMaster: boolean;
  status: OutputStatus;
  reason: OutputUnavailableReason | null;
}

/** pl://outputs と outputsGet（変わるたびに全部） */
export interface OutputsView {
  mode: ViewMode;
  /** 今のモードの全体の音量 */
  master: number;
  masters: PerMode;
  muted: boolean;
  /** 枠3つ（空の枠は null） */
  slots: (OutputSlotView | null)[];
}

/**
 * 遅れを測る（v3.8.0 段3）。録る所：その出力のループバック（Windows がその出力へ渡した音）か、録音デバイス（仮想ケーブルの出口など）
 */
export type CaptureSource = { kind: "loopback" } | { kind: "input"; id: string };

/** 測れなかった理由（文は src\layout\latency.ts） */
export type LatencyFail =
  | "output_missing"
  | "output_busy"
  | "output_format"
  | "output_open"
  | "capture_missing"
  | "capture_busy"
  | "capture_format"
  | "capture_open"
  | "capture_denied"
  | "open_timeout"
  | "clock"
  | "no_timing"
  | "capture_silent"
  | "stream_error"
  | "late"
  | "gap"
  | "no_signal"
  | "weak"
  | "noisy"
  | "unstable";

/** 全体をやめた・始めなかった理由 */
export type LatencyAbort = "playing" | "cancelled" | "busy" | "not_enough" | "failed";

/** 小窓を開いたときの様子（latency_setup） */
export interface LatencySetup {
  rows: {
    slot: number;
    name: string;
    status: OutputStatus;
    /** 測る（オンで鳴らせる）か */
    measurable: boolean;
    /** 名前から選んだ録る所 */
    autoCapture: CaptureSource;
    delayMs: number;
  }[];
  /** 録る所の候補（録音デバイス） */
  inputs: { id: string; name: string }[];
}

export interface LatencyRow {
  slot: number;
  /** 測ったときのデバイスの設定（入れるときに同じかを確かめる） */
  deviceId: string | null;
  name: string;
  status: OutputStatus;
  measured: boolean;
  capture: CaptureSource | null;
  /** 届くまでの遅れ（ms） */
  arrivalMs: number | null;
  spreadMs: number | null;
  hits: number;
  /** 入れる値（ms）。測れなかった・揃える相手がいないときは null */
  fillMs: number | null;
  /** 測ったときの遅らせ量 */
  delayMs: number;
  fail: LatencyFail | null;
}

export interface LatencyReport {
  abort: LatencyAbort | null;
  rows: LatencyRow[];
}

/** メーター（約 33ms ごと）。枠3つの [左, 右] の最大（音量を掛けたあと・切り詰める前。1.0 がフルスケール）。dB への換算は画面側 */
export interface MeterFrame {
  peaks: [number, number][];
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
    /** v3.4.0：曲一覧のコンパクト表示（"off" 使わない・"always" いつも）。無ければ使わない。読み方は src\layout\density.ts。
     *  3.7.0 までの "narrow"（狭いときだけ）は、Rust が読むときに "off" ＋ compactAuto オンへ読み替える */
    compactList?: string;
    /** v3.8.0：窓が狭いときは自動でコンパクトにする（無ければオフ） */
    compactAuto?: boolean;
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
  /** v3.9.0 からは avatar が決める（avatar が "none" でなければ true。Rust の sanitize が合わせる。画面からは変えない） */
  characterVisible: boolean;
  /**
   * v3.9.0：使う立ち絵。"skin"（スキンの立ち絵。既定）・"none"（出さない）・"avatar:<id>"（登録した立ち絵）・"skin:<id>"（取り込んだスキンの立ち絵）。
   * 変えるのは avatars_use・削除だけ（設定の保存では変わらない）
   */
  avatar: string;
  /** 0.75〜2（v3.9.0。3.8.1 までは 0.75〜1.25） */
  characterScale: number;
  /** 次の起動だけ既定の見た目にする */
  safeModeNextStart: boolean;
  /**
   * v3.10.0：手で選んだ色（色の名前 → 値。15 個の名前と、スキンの色と同じ値の形だけ。Rust が形をそろえる）。Dark・Light 共通。
   * 変えるのは skins_color_set・skins_colors_clear・「既定の見た目に戻す」だけ（設定の保存では変わらない）。3.9.1 までの設定には無い
   */
  colors?: Record<string, string>;
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
  /** v3.8.0：出力先の設定を書き出した・読み込んだ（読み込んだときは一時停止して全部オフ） */
  outputs: boolean;
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
  /** broken_fallback / safe_mode / safe_mode_second / avatar_fallback（v3.9.0） */
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
  /** v3.9.0：使う立ち絵（設定の skin.avatar と同じ値） */
  avatar: string;
  /** v3.9.0：いま画面に出す立ち絵（使う立ち絵を解いたもの。出さないときは null）。画面はスキンの character ではなくこちらを出す */
  character: SkinCharacter | null;
}

// ---- 立ち絵（v3.9.0）。Rust 側 skins/avatars.rs と同じ形 ----

/** 登録した立ち絵。preview は Rust が確かめた avatars の下の静止画のパス */
export interface AvatarSummary {
  id: string;
  name: string;
  preview: string | null;
  hasStopped: boolean;
  /** 「変更」の小窓の見本（静止画のパス。停止中が無ければ stoppedPreview は null） */
  playingPreview: string | null;
  stoppedPreview: string | null;
  animated: boolean;
  /** 読めない立ち絵（消すことだけできる） */
  broken: ErrorCode | null;
}

/** 取り込んだスキンのうち、立ち絵を持つもの（スキンの中の絵を指すだけ） */
export interface SkinCharacterRow {
  skinId: string;
  name: string;
  preview: string | null;
  animated: boolean;
}

export interface AvatarList {
  skins: SkinCharacterRow[];
  avatars: AvatarSummary[];
  /** 登録できる数（30） */
  max: number;
}

/** 選んで確かめた画像（パスは返らない。作るときは token で指す） */
export interface PickedImage {
  token: string;
  slot: "playing" | "stopped";
  previewDataUrl: string | null;
  /** 名前の既定（ファイル名から作った名前。決まりに合わなければ詰め、それでも空なら「立ち絵」。空にはならない）。名前を空のまま追加すると、再生中の絵のこの名前になる */
  suggestedName: string;
  width: number;
  height: number;
  frames: number;
  durationMs: number;
  animated: boolean;
  bytes: number;
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
