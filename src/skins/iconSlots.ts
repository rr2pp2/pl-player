// 手で入れるアイコン（v3.11.0）：設定の「アイコン」の欄に並べる枠と、ドラッグ＆ドロップの振り分け。
// 枠は Rust の skins::icons::all_slots と同じ（ボタンのアイコン 34 個。状態のある4つは default を除いた状態ごと。ほかに
// ウィンドウ・トレイのアイコン）。並べ方は今の画面の場所ごと、名前は日本語。大きさは、その場所で実際に出す大きさ（px）。
// ほかに依存しない純粋な関数と表だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

export type IconGroupId = "player" | "list" | "sidebar" | "topbar" | "other" | "app";

export const ICON_GROUPS: { id: IconGroupId; label: string }[] = [
  { id: "player", label: "再生バー" },
  { id: "list", label: "曲一覧" },
  { id: "sidebar", label: "サイドバー" },
  { id: "topbar", label: "上の帯" },
  { id: "other", label: "そのほか（メニュー・ボタン・歌詞の画面）" },
  { id: "app", label: "アプリのアイコン" },
];

/** サイドバーの項目は、設定の「サイドバーのアイコンの大きさ」（14〜32px）で出す */
export const SIDEBAR_PX = "sidebar" as const;

export interface IconSlot {
  /** アイコンの名前（src\components\Icons.tsx の名前・AppWindow・AppTray） */
  name: string;
  /** 状態（状態の無いものは default） */
  state: string;
  group: IconGroupId;
  /** 名前（日本語） */
  label: string;
  /** 状態の名前（状態のあるものだけ） */
  stateLabel: string | null;
  /** 実際に出す大きさ（px）。サイドバーは設定の値 */
  px: number | typeof SIDEBAR_PX;
}

const one = (name: string, group: IconGroupId, label: string, px: IconSlot["px"]): IconSlot => ({ name, state: "default", group, label, stateLabel: null, px });
const states = (name: string, group: IconGroupId, label: string, px: number, list: [string, string][]): IconSlot[] =>
  list.map(([state, stateLabel]) => ({ name, state, group, label, stateLabel, px }));

/** 並べる枠（場所ごと。場所の中は画面で見る順） */
export const ICON_SLOTS: IconSlot[] = [
  // 再生バー：ボタン 20px、再生・一時停止は 22px
  one("IconPlay", "player", "再生", 22),
  one("IconPause", "player", "一時停止", 22),
  one("IconPrev", "player", "前の曲", 20),
  one("IconNext", "player", "次の曲", 20),
  ...states("IconShuffle", "player", "シャッフル", 20, [
    ["off", "切"],
    ["on", "入"],
  ]),
  ...states("IconRepeat", "player", "リピート", 20, [
    ["off", "切"],
    ["all", "全曲"],
    ["one", "1曲"],
  ]),
  ...states("IconVolume", "player", "音量", 20, [
    ["muted", "消音"],
    ["zero", "0"],
    ["low", "小"],
    ["high", "大"],
  ]),
  one("IconLyrics", "player", "歌詞", 20),
  one("IconLyricsRow", "player", "流れる歌詞", 20),
  // 曲一覧：行のボタン 16px、再生中の印 14px
  ...states("IconStar", "list", "お気に入り", 16, [
    ["filled", "オン"],
    ["outline", "オフ"],
  ]),
  one("IconTrash", "list", "ごみ箱・削除", 16),
  one("IconBars", "list", "再生中の印", 14),
  // サイドバー：項目のアイコン（設定の大きさ）
  one("IconMusic", "sidebar", "すべての曲", SIDEBAR_PX),
  one("IconUser", "sidebar", "アーティスト", SIDEBAR_PX),
  one("IconDisc", "sidebar", "アルバム", SIDEBAR_PX),
  one("IconTag", "sidebar", "ジャンル", SIDEBAR_PX),
  one("IconClockPlus", "sidebar", "最近追加", SIDEBAR_PX),
  one("IconHistory", "sidebar", "最近再生", SIDEBAR_PX),
  one("IconRanking", "sidebar", "年間まとめ", SIDEBAR_PX),
  one("IconQueue", "sidebar", "再生キュー", SIDEBAR_PX),
  one("IconList", "sidebar", "プレイリスト", SIDEBAR_PX),
  one("IconPlus", "sidebar", "作成・追加", SIDEBAR_PX),
  one("IconSettings", "sidebar", "設定", SIDEBAR_PX),
  one("IconSidebar", "sidebar", "サイドバーをたたむ", 20),
  // 上の帯：検索 18px、ほかは 16px
  one("IconSearch", "topbar", "検索", 18),
  one("IconX", "topbar", "消す・閉じる", 16),
  one("IconFilePlus", "topbar", "ファイルを取り込む", 16),
  one("IconFolderPlus", "topbar", "フォルダを取り込む", 16),
  // そのほか：メニューとボタンは 16px、歌詞の画面の「戻る」は 20px
  one("IconEdit", "other", "編集", 16),
  one("IconImage", "other", "画像", 16),
  one("IconFolder", "other", "フォルダ", 16),
  one("IconRefresh", "other", "読み直す", 16),
  one("IconChevron", "other", "メニューの続き", 16),
  one("IconBack", "other", "戻る", 20),
  // アプリのアイコン：ウィンドウ・タスクバー 32px、タスクトレイ 16px（拡大率 100% のとき）
  one("AppWindow", "app", "ウィンドウ・タスクバー", 32),
  one("AppTray", "app", "タスクトレイ", 16),
];

export const isAppIcon = (name: string) => name === "AppWindow" || name === "AppTray";

/** 手で入れた数（設定の skin.icons） */
export function manualIconCount(icons: Record<string, Record<string, string>> | null | undefined): number {
  let n = 0;
  for (const m of Object.values(icons ?? {})) n += Object.keys(m ?? {}).length;
  return n;
}

/** その枠に手で入れているか */
export const hasManualIcon = (icons: Record<string, Record<string, string>> | null | undefined, name: string, state: string): boolean =>
  !!icons?.[name]?.[state];

/** その枠の大きさ（px） */
export const slotPx = (slot: IconSlot, sidebarIconPx: number): number => (slot.px === SIDEBAR_PX ? sidebarIconPx : slot.px);

// ---------------- ドラッグ＆ドロップの振り分け ----------------

/** アイコンの枠の受け手の印（data-drop）。ほかの受け手（背景・アルバムの画像など）と重ならない頭 */
export const ICON_ZONE_PREFIX = "icon:";

export const iconZoneKey = (name: string, state: string) => `${ICON_ZONE_PREFIX}${name}:${state}`;

export function parseIconZone(key: string | null | undefined): { name: string; state: string } | null {
  if (!key || !key.startsWith(ICON_ZONE_PREFIX)) return null;
  const [name, state, ...rest] = key.slice(ICON_ZONE_PREFIX.length).split(":");
  if (!name || !state || rest.length) return null;
  return { name, state };
}

/**
 * 窓に落としたものをどこへ回すか（src\components\FileDrop.tsx）。
 * - アイコンの枠の上：その枠へ（何が落ちたかにかかわらず。1つのファイルか、複数・フォルダ・ファイルでないものかは Rust が見て断る）
 * - 落としたパスが無い：何もしない（今までどおり）
 * - ほかの受け手の上（背景・アルバムの画像など）：その受け手へ（今までどおり）
 * - どの受け手の上でもない：曲の取り込み（今までどおり）
 */
export function routeDrop(zone: string | null, pathCount: number): "zone" | "import" | "none" {
  if (zone && parseIconZone(zone)) return "zone";
  if (pathCount === 0) return "none";
  if (zone) return "zone";
  return "import";
}
