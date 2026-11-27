// v3.6.0：年間まとめ（サイドバーの項目・画面・文）と、タグの編集画面の保存で外された曲を戻さないこと（画面側）を確かめる。
// 数え方・期間・並び・曲を外したときの記録の消え方は Rust のテスト（db::history・db::mod・commands::metadata）。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/yearly.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fallbackView, SIDEBAR_ITEMS, sidebarItemOfView, sidebarItemShown, TRASH_LISTS } from "../src/layout/visibility.ts";
import {
  errorMessage,
  metadataSavedText,
  YEARLY_SINCE_VERSION,
  yearlyDate,
  yearlyEmptyText,
  yearlyPeriodText,
  yearlyStartNote,
} from "../src/messages/ja.ts";

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");
const DAY = 24 * 60 * 60 * 1000;

test("サイドバー：「年間まとめ」は「最近再生」の次。名前と並びが Rust の SIDEBAR_ITEMS_HIDEABLE と同じで、3.5.0 までの設定（値が無い）では出す", () => {
  const ids = SIDEBAR_ITEMS.map((s) => s.id);
  assert.equal(ids.indexOf("yearly_top"), ids.indexOf("recent_played") + 1);
  assert.equal(SIDEBAR_ITEMS.find((s) => s.id === "yearly_top")!.label, "年間まとめ");
  const rs = read("src-tauri/src/settings/store.rs");
  const m = rs.match(/pub const SIDEBAR_ITEMS_HIDEABLE: &\[&str\] =\s*&\[([^\]]*)\]/);
  assert.ok(m, "SIDEBAR_ITEMS_HIDEABLE");
  assert.deepEqual([...m![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]), ids);
  // 3.5.0 までの設定：ほかの項目は書いてあっても yearly_top は無い → 出す
  const old = { sidebarItems: { all: true, artist: true, album: true, genre: false, recent_added: true, recent_played: true, favorites: true, queue: true, playlists: true } };
  assert.equal(sidebarItemShown(old, "yearly_top"), true);
  assert.equal(sidebarItemShown({ sidebarItems: { yearly_top: false } }, "yearly_top"), false);
  // 隠したとき、開いていれば出している最初の項目へ移る（ほかの項目と同じ決まり）
  assert.equal(sidebarItemOfView("yearly_top"), "yearly_top");
  assert.equal(fallbackView({ sidebarItems: { yearly_top: false } }, "yearly_top"), "all");
  assert.equal(fallbackView({ sidebarItems: { yearly_top: false } }, "recent_played"), null);
  // サイドバーの入口と、画面の振り分け
  const sb = read("src/layout/Sidebar.tsx");
  assert.match(sb, /\{ view: "recent_played", label: "最近再生", icon: <IconHistory \/> \},\r?\n(\s*\/\/[^\n]*\r?\n)?\s*\{ view: "yearly_top", label: "年間まとめ", icon: <IconRanking \/> \},/);
  assert.match(read("src/ipc/types.ts"), /\| "yearly_top"/);
  assert.match(read("src/layout/AppShell.tsx"), /if \(r\.view === "yearly_top"\) return <YearlyView \/>;/);
});

test("アイコン：IconRanking は着せ替えで差し替えられる（Rust の ICONS と、Icons.tsx の差し替えの名前が同じ。v3.12.0 で IconShufflePlay を足して 35 個）", () => {
  const icons = read("src/components/Icons.tsx");
  const tsNames = [...icons.matchAll(/<Skinned name="(\w+)"/g)].map((x) => x[1]);
  assert.ok(tsNames.includes("IconRanking"));
  assert.match(icons, /export const IconRanking = \(p: P\) => \(/);
  const rs = read("src-tauri/src/skins/manifest.rs");
  const m = rs.match(/pub const ICONS: \[\(&str, &\[&str\]\); (\d+)\] = \[([\s\S]*?)\n\];/);
  assert.ok(m, "ICONS");
  const rsNames = [...m![2].matchAll(/\("(\w+)",/g)].map((x) => x[1]);
  assert.equal(Number(m![1]), 35);
  assert.deepEqual([...rsNames].sort(), [...new Set(tsNames)].sort());
  // スキンを作る人向けの手引きにも同じ数と名前
  const doc = read("docs/スキンの作り方.md");
  assert.match(doc, /名前は次の 35 個：/);
  assert.match(doc, /`IconRanking`/);
});

test("画面：曲一覧と同じ TrackTable（ライブラリの一覧として。右クリック・ごみ箱は今までどおり）に順位と回数を渡し、並べ替えない", () => {
  const v = read("src/views/YearlyView.tsx");
  assert.match(v, /<TrackTable\b/);
  assert.match(v, /list="library"/);
  assert.ok(TRASH_LISTS.includes("library"));
  assert.match(v, /onPlay=\{play\}/);
  assert.match(v, /ranking=\{ranking\}/);
  assert.match(v, /\n\s*fixedOrder\r?\n/);
  assert.match(v, /api\s*\.libraryYearlyTop\(q \|\| null\)/);
  // ライブラリが変わったら（曲を外した など）引き直す
  assert.match(v, /\}, \[q, ver\]\);/);
  // 窓口の名前が Rust と同じ（lib.rs に登録してある）
  const cmds = read("src/ipc/commands.ts");
  assert.match(cmds, /invoke<YearlyTop>\("library_yearly_top", \{ search: search \?\? null \}\)/);
  assert.match(read("src-tauri/src/lib.rs"), /commands::library::library_yearly_top,/);
  // 結果の形（camelCase）が Rust と同じ
  const types = read("src/ipc/types.ts");
  const rs = read("src-tauri/src/db/history.rs");
  for (const [ts, rust] of [["rank", "rank"], ["plays", "plays"], ["lastPlayedAt", "last_played_at"], ["track", "track"], ["since", "since"], ["until", "until"], ["startedAt", "started_at"], ["ranked", "ranked"], ["entries", "entries"]]) {
    assert.match(types, new RegExp(`\\n\\s*${ts}: `), ts);
    assert.match(rs, new RegExp(`\\n\\s*pub ${rust}: `), rust);
  }
  // 期間と数は Rust の決まり（365 日・100 曲）
  assert.match(rs, /pub const YEAR_MS: i64 = 365 \* 24 \* 60 \* 60 \* 1000;/);
  assert.match(rs, /pub const TOP_N: i64 = 100;/);
  // TrackTable：並びが決まっている一覧では見出しを押せず、矢印も出さない
  const tt = read("src/components/TrackTable.tsx");
  // v3.8.0：矢印を出す見出しは sortHead（src\layout\sortHead.ts）が決め、fixedOrder なら出さない（決まりは ui-density.test.mts で確かめる）
  assert.match(tt, /const mark = sortHead\(sortState, \{ fixedOrder: !!props\.fixedOrder, artistColumn: cols\.artist \}\);/);
  assert.match(tt, /disabled=\{props\.sortable \|\| props\.fixedOrder\}/);
  assert.match(tt, /\{cols\.rank \? <span className="col-rank">順位<\/span> : null\}/);
  assert.match(tt, /\{cols\.plays \? <span className="col-plays">回数<\/span> : null\}/);
});

test("文：期間・記録が無いとき・1年に満たないとき（3.6.0 から数え始めたこと）", () => {
  assert.equal(YEARLY_SINCE_VERSION, "3.6.0");
  // 日付はこの PC の時刻で（試しの時刻は、どの時差でも日付が変わらない正午）
  const noon = new Date(2026, 10, 14, 12, 0, 0).getTime();
  assert.equal(yearlyDate(noon), "2026年11月14日");
  assert.equal(yearlyPeriodText({ since: noon - 365 * DAY, until: noon }), "2025年11月14日〜2026年11月14日");
  assert.ok(yearlyEmptyText.includes("3.6.0 から"), yearlyEmptyText);
  assert.ok(yearlyEmptyText.includes("まだ再生の記録がありません"));
  const since = noon - 365 * DAY;
  // 記録が無い：一覧の文が言うので、見出しの知らせは出さない
  assert.equal(yearlyStartNote({ since, startedAt: noon - 3 * DAY, ranked: 0 }), null);
  // 数え始めたのが期間の始まりより後
  assert.equal(yearlyStartNote({ since, startedAt: noon - 3 * DAY, ranked: 4 }), "3.6.0 から数え始めたので、2026年11月11日より前の再生は入っていません。");
  // 1年たてば出さない（境目ちょうども出さない）
  assert.equal(yearlyStartNote({ since, startedAt: since, ranked: 4 }), null);
  assert.equal(yearlyStartNote({ since, startedAt: since - DAY, ranked: 4 }), null);
  assert.equal(yearlyStartNote({ since, startedAt: null, ranked: 4 }), null);
});

test("タグの編集画面：保存の時点で外されていた曲は、曲ファイルにも書かず知らせて閉じる。書いたあとに外されたときは、戻していないことを知らせる", () => {
  const e = errorMessage({ code: "track_removed_before_save", detail: "track 1" });
  assert.equal(e.text, "この曲はライブラリから外されたので、保存しませんでした（曲ファイルは変えていません）。");
  assert.match(read("src/ipc/types.ts"), /\| "track_removed_before_save"/);
  assert.match(read("src-tauri/src/error.rs"), /\n\s*TrackRemovedBeforeSave,/);
  assert.equal(metadataSavedText(true), "タグを保存しました");
  assert.ok(metadataSavedText(false).includes("ライブラリには戻していません"));
  const ed = read("src/components/MetadataEditor.tsx");
  assert.match(ed, /notify\(\{ level: "info", text: metadataSavedText\(saved !== null\) \}\);/);
  const branch = ed.slice(ed.indexOf('=== "track_removed_before_save"'), ed.indexOf("setError(errorMessage(e));"));
  assert.match(branch, /props\.onClose\(\);\s*return;/, "読み直さずに閉じる（外した曲は読めない）");
  assert.match(read("src/ipc/commands.ts"), /invoke<Track \| null>\("metadata_write"/);
});
