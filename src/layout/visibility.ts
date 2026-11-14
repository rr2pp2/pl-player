// v3.3.0：使う人が要らない項目を隠す（再生バーのボタン・サイドバーの項目）と、曲をごみ箱へ移すときの決まり、
// 設定のタブの並び。ほかに依存しない純粋な関数だけを置く（scripts\ui-visibility.test.mts から Node だけで確かめるため）。
//
// 隠せる項目の名前は Rust の settings\store.rs（PLAYER_BUTTONS_HIDEABLE・SIDEBAR_ITEMS_HIDEABLE）と同じ。
// 隠せないもの（再生・一時停止・前・次・シークバー・設定）は、ここに名前が無いので、設定に値があっても隠れない。

export interface UiLike {
  playerButtons?: Record<string, boolean>;
  sidebarItems?: Record<string, boolean>;
  trashButton?: boolean;
}

/** 再生バーで隠せるボタン（並びは設定画面に出す順＝再生バーの左から） */
export const PLAYER_BUTTONS = [
  { id: "shuffle", label: "シャッフル" },
  { id: "repeat", label: "リピート" },
  { id: "volume", label: "音量（ミュートと音量の帯）" },
  { id: "lyricsRow", label: "流れる歌詞の列のオン・オフ" },
  { id: "lyrics", label: "歌詞" },
] as const;

/** 再生バーで隠せないもの（隠すと操作できなくなる） */
export const PLAYER_FIXED = ["previous", "playPause", "next", "seek"] as const;

/** サイドバーで隠せる項目（並びはサイドバーの上から） */
export const SIDEBAR_ITEMS = [
  { id: "all", label: "すべての曲" },
  { id: "artist", label: "アーティスト" },
  { id: "album", label: "アルバム" },
  { id: "genre", label: "ジャンル" },
  { id: "recent_added", label: "最近追加" },
  { id: "recent_played", label: "最近再生" },
  // v3.6.0。3.5.0 までの設定には値が無いので出す（sidebarItemShown の「値が無ければ出す」）
  { id: "yearly_top", label: "年間まとめ" },
  { id: "favorites", label: "お気に入り" },
  { id: "queue", label: "キュー" },
  { id: "playlists", label: "プレイリスト（節ごと）" },
] as const;

/** サイドバーで隠せないもの（隠すと設定へ戻れなくなる） */
export const SIDEBAR_FIXED = ["settings"] as const;

const hideable = (list: readonly { id: string }[], id: string) => list.some((x) => x.id === id);

/** 再生バーのボタンを出すか。隠せないもの・知らない名前は常に出す。値が無ければ出す（古い設定） */
export function playerButtonShown(ui: UiLike | null | undefined, id: string): boolean {
  if (!hideable(PLAYER_BUTTONS, id)) return true;
  return ui?.playerButtons?.[id] !== false;
}

/** サイドバーの項目を出すか。設定・知らない名前は常に出す。値が無ければ出す（古い設定） */
export function sidebarItemShown(ui: UiLike | null | undefined, id: string): boolean {
  if (!hideable(SIDEBAR_ITEMS, id)) return true;
  return ui?.sidebarItems?.[id] !== false;
}

/** 一覧の画面（route の view）が、サイドバーのどの項目から開くものか（プレイリストは節） */
export function sidebarItemOfView(view: string): string | null {
  if (view === "playlist") return "playlists";
  if (view === "settings" || view === "lyrics") return null;
  return hideable(SIDEBAR_ITEMS, view) ? view : null;
}

/**
 * 今開いている画面の入口を隠したとき、どこへ移るか（移らないなら null）。
 * 隠すのはサイドバーの入口だけで、画面そのものは残す（再生バーのジャケットからアルバムを開く、などは今までどおり）。
 * 設定を変えたその時に開いている画面の入口が隠れたら、サイドバーに出している最初の LIBRARY の項目へ移る。
 * どれも出していなければ「すべての曲」の画面へ（入口は無いまま。ここがいちばん基本の画面のため）。
 * 曲のキー付きの画面（アーティスト・アルバムを開いたところ）も、同じ項目の入口が隠れたら移る
 */
export function fallbackView(ui: UiLike | null | undefined, view: string): string | null {
  const item = sidebarItemOfView(view);
  if (!item || sidebarItemShown(ui, item)) return null;
  const first = SIDEBAR_ITEMS.find((x) => x.id !== "playlists" && x.id !== "queue" && sidebarItemShown(ui, x.id));
  return first ? first.id : "all";
}

/** サイドバーの切り替えボタン：ウィンドウが狭くて自動でたたんでいるときは押せない（説明を出す） */
export const SIDEBAR_AUTO_HINT = "ウィンドウが狭いので、サイドバーは自動でたたんでいます。幅を広げると切り替えられます。";
export function sidebarToggleInfo(sidebarAuto: boolean, collapsed: boolean): { disabled: boolean; label: string; hint: string | null } {
  if (sidebarAuto) return { disabled: true, label: "サイドバーの表示を切り替える（今は押せません）", hint: SIDEBAR_AUTO_HINT };
  return { disabled: false, label: collapsed ? "サイドバーを広げる" : "サイドバーをたたむ", hint: null };
}

// ---------------- 曲をごみ箱へ移す ----------------

/**
 * 曲の操作（行の右端のボタン・右クリックのメニュー）を出す場所。
 * library：ライブラリの曲一覧（すべての曲・最近追加・最近再生・お気に入り・アーティスト／アルバム／ジャンルを開いたところ。src\views\LibraryView.tsx。
 *          v3.6.0 の年間まとめ（src\views\YearlyView.tsx）も、ライブラリの曲を並べた一覧なので同じ）
 * playlist：プレイリストの一覧　queue：キューの一覧　playerBar：再生バーの曲名の右クリック
 */
export type TrackListKind = "library" | "playlist" | "queue" | "playerBar";

/**
 * 「ごみ箱へ移す」を出す場所（作者の決定・v3.3.0）：ライブラリの曲一覧だけ。
 * プレイリスト・キューの一覧では、行のボタンも右クリックのメニューも出さない（「プレイリストから削除」「キューから削除」と
 * 取り違えて、曲ファイルまでごみ箱へ移さないため）。再生バーの右クリックにも出さない（一覧ではないため）
 */
export const TRASH_LISTS: readonly TrackListKind[] = ["library"];

/**
 * 行の右端の「ごみ箱へ移す」ボタンを出すか：設定の「行に削除ボタンを出す」（ui.trashButton）がオンで、ライブラリの曲一覧のときだけ。
 * 設定が決めるのは行のボタンだけ（作者の決定・v3.3.0 の直し）。右クリックのメニューは trashMenuShown
 */
export function trashButtonShown(ui: UiLike | null | undefined, list: TrackListKind): boolean {
  return ui?.trashButton === true && TRASH_LISTS.includes(list);
}

/**
 * 右クリックのメニューに「ごみ箱へ移す…」を出すか：ライブラリの曲一覧なら、設定にかかわらずいつも出す
 * （作者の決定・v3.3.0 の直し。「ライブラリから外す…」と並べて、どちらも確認のあとで動く）。プレイリスト・キュー・再生バーには出さない
 */
export function trashMenuShown(list: TrackListKind): boolean {
  return TRASH_LISTS.includes(list);
}

/** 1回にごみ箱へ移せる曲の数。Rust（src-tauri\src\library\trash.rs の MAX_TRACKS）と同じ数 */
export const TRASH_MAX_TRACKS = 1000;

/** 選んだ曲の数が多すぎるか（多すぎれば、確認の小窓を出す前に断る。何も移さない） */
export const trashTooMany = (n: number): boolean => n > TRASH_MAX_TRACKS;

/**
 * 行の「ごみ箱へ移す」を押したとき、どの曲を対象にするか（右クリックのメニューと同じ決まり）：
 * 押した行が選んでいる行の中にあれば、選んでいる全部。選んでいない行なら、その行だけ
 */
export function trashTargets(selected: Iterable<number>, clicked: number): number[] {
  const sel = [...selected].sort((a, b) => a - b);
  return sel.includes(clicked) ? sel : [clicked];
}

/** 曲の id の重なりを除く（キューのように同じ曲が何度も並ぶ一覧のため）。順は最初に出た順 */
export function uniqueIds(ids: number[]): number[] {
  return [...new Set(ids)];
}

/** 確認の小窓に1曲ずつ並べる上限（それより多い分は「ほか N 曲」） */
export const TRASH_LIST_MAX = 5;

export interface TrashTrackLike {
  title: string;
  path: string;
}

/** 確認の小窓の見出しと本文 */
export function trashConfirmText(tracks: TrashTrackLike[]): { title: string; body: string; ok: string } {
  const n = tracks.length;
  const lines = tracks.slice(0, TRASH_LIST_MAX).map((t) => `・${t.title}\n　${t.path}`);
  if (n > TRASH_LIST_MAX) lines.push(`ほか ${n - TRASH_LIST_MAX} 曲`);
  const head = n === 1 ? "この曲のファイルを、Windows のごみ箱へ移します。" : `選んでいる ${n} 曲のファイルを、Windows のごみ箱へ移します。`;
  return {
    title: n === 1 ? "曲をごみ箱へ移す" : `${n} 曲をごみ箱へ移す`,
    body: [
      head,
      "",
      ...lines,
      "",
      "ごみ箱から戻せます（戻したあとは、取り込み直すとライブラリに入ります）。",
      "ライブラリ・プレイリスト・キュー・お気に入り・この曲の歌詞の設定からも外れます。",
      "ごみ箱の無い場所（USB メモリなど）にある曲は、完全に消えてしまうので移しません。",
    ].join("\n"),
    ok: "ごみ箱へ移す",
  };
}

// ---------------- 設定のタブ ----------------

/** 設定のタブ（既定の順） */
export const SETTINGS_TABS = [
  { id: "display", label: "表示" },
  { id: "skin", label: "着せ替え" },
  { id: "device", label: "再生デバイス" },
  { id: "library", label: "ライブラリ" },
  { id: "lyrics", label: "ネットの歌詞" },
  { id: "keys", label: "キー操作" },
  { id: "app", label: "アプリ" },
] as const;

export const DEFAULT_TAB_ORDER: string[] = SETTINGS_TABS.map((t) => t.id);

/**
 * 保存した順番と、今の版のタブを合わせる。
 * - 知らない名前（前の版・後の版のタブ）と、重なった名前は捨てる
 * - 保存した順番に無いタブ（版が上がって増えたもの・壊れた設定）は、既定の順で前にあるタブのすぐ後ろへ入れる
 *   （前にあるタブがどれも無ければ先頭）。だから、どんな値でも今の版のタブが全部、1回ずつ出る
 */
export function mergeTabOrder(saved: unknown, all: string[] = DEFAULT_TAB_ORDER): string[] {
  const out: string[] = [];
  if (Array.isArray(saved)) {
    for (const x of saved) {
      if (typeof x === "string" && all.includes(x) && !out.includes(x)) out.push(x);
    }
  }
  all.forEach((id, i) => {
    if (out.includes(id)) return;
    let at = 0;
    for (let k = i - 1; k >= 0; k--) {
      const p = out.indexOf(all[k]);
      if (p >= 0) {
        at = p + 1;
        break;
      }
    }
    out.splice(at, 0, id);
  });
  return out;
}

/** 並べ替え：from の名前を to の名前の位置へ（dnd-kit の arrayMove と同じ動き） */
export function moveTab(order: string[], from: string, to: string): string[] {
  const a = order.indexOf(from);
  const b = order.indexOf(to);
  if (a < 0 || b < 0 || a === b) return order;
  const next = [...order];
  const [x] = next.splice(a, 1);
  next.splice(b, 0, x);
  return next;
}

export const isDefaultTabOrder = (order: string[]): boolean =>
  order.length === DEFAULT_TAB_ORDER.length && order.every((x, i) => x === DEFAULT_TAB_ORDER[i]);
