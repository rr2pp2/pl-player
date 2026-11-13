// v3.5.0：古い文字コードのタグの読み直し（設定の「ライブラリ」の「古い文字コードのタグを読み直す…」）の画面側を確かめる。
// 文（確認・進み具合・終わりの知らせ・断ったとき）と、Rust 側と合わせる名前（結果の形・通知の名前・窓口の名前・エラーの種別）、設定画面の置き方。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/legacy-reread.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { errorMessage, legacyRereadConfirm, legacyRereadDoneText, legacyRereadProgressText, noticeText } from "../src/messages/ja.ts";

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");

const base = { running: false, finished: true, cancelled: false, total: 120, done: 120, fixed: 0, skippedManual: 0, changedDuringRun: 0, error: null };

test("確認の小窓：曲ファイルに書かないこと・手で直した名前を書き換えないこと・途中でやめられることを出す", () => {
  assert.equal(legacyRereadConfirm.title, "古い文字コードのタグを読み直す");
  assert.equal(legacyRereadConfirm.ok, "読み直す");
  for (const s of ["曲ファイルには書き込みません", "手で直した名前は書き換えません", "途中でやめられます", "全曲"]) {
    assert.ok(legacyRereadConfirm.body.includes(s), s);
  }
});

test("進み具合と終わりの知らせ：直した数・直すものが無い・やめた・エラー・手で直した曲・途中で変わった曲", () => {
  assert.equal(legacyRereadProgressText({ ...base, running: true, finished: false, done: 40, fixed: 3 }), "120曲中 40曲・直した 3曲");
  const fixed = legacyRereadDoneText({ ...base, fixed: 7 });
  assert.equal(fixed.level, "info");
  assert.equal(fixed.text, "古い文字コードのタグを読み直しました。7曲の名前を直しました（120曲を見ました）");
  assert.equal(fixed.detail, "");
  const none = legacyRereadDoneText(base);
  assert.equal(none.text, "古い文字コードのタグを読み直しました。直せる名前はありませんでした（120曲を見ました）");
  const cancelled = legacyRereadDoneText({ ...base, cancelled: true, done: 40, fixed: 2 });
  assert.equal(cancelled.level, "info");
  assert.equal(cancelled.text, "古い文字コードのタグの読み直しをやめました（40曲を見て、2曲を直しました）");
  const failed = legacyRereadDoneText({ ...base, error: "database", done: 60, fixed: 5 });
  assert.equal(failed.level, "error");
  assert.ok(failed.text.includes("途中で止めました") && failed.text.includes("5曲を直しました"), failed.text);
  assert.ok(failed.detail.startsWith("ライブラリのデータベースでエラーが起きました。"), failed.detail);
  const more = legacyRereadDoneText({ ...base, fixed: 1, skippedManual: 2, changedDuringRun: 3 });
  assert.ok(more.detail.includes("手で直した項目がある 2曲"), more.detail);
  assert.ok(more.detail.includes("ライブラリから外された 3曲"), more.detail);
});

test("断ったときの文：読み直しの二重押し・取り込みの途中・読み直しの途中の取り込み", () => {
  assert.ok(errorMessage({ code: "legacy_reread_busy", detail: "" }).text.includes("読み直している途中"));
  assert.ok(errorMessage({ code: "library_import_busy", detail: "" }).text.includes("取り込み・再走査の途中"));
  const n = noticeText({ level: "info", code: "import_refused_legacy_reread", detail: "" } as any);
  assert.ok(n.text.includes("取り込み・再走査をしませんでした"), n.text);
});

test("結果の形・通知の名前・窓口の名前・エラーの種別が Rust と同じ", () => {
  const rs = read("src-tauri/src/library/legacy_reread.rs");
  const struct = rs.slice(rs.indexOf("pub struct RereadProgress"), rs.indexOf("}", rs.indexOf("pub struct RereadProgress")));
  const rustFields = [...struct.matchAll(/pub (\w+):/g)].map((m) => m[1].replace(/_(\w)/g, (_, c) => c.toUpperCase())).sort();
  const ts = read("src/ipc/types.ts");
  const iface = ts.slice(ts.indexOf("export interface LegacyRereadProgress"), ts.indexOf("}", ts.indexOf("export interface LegacyRereadProgress")));
  const tsFields = [...iface.matchAll(/^\s+(\w+):/gm)].map((m) => m[1]).sort();
  assert.deepEqual(tsFields, rustFields);
  assert.deepEqual(Object.keys(base).sort(), rustFields);
  // 通知の名前
  const ev = read("src-tauri/src/commands/mod.rs");
  const events = read("src/ipc/events.ts");
  for (const name of ["pl://legacy-reread-progress", "pl://legacy-reread-done"]) {
    assert.ok(ev.includes(`"${name}"`), name);
    assert.ok(events.includes(`"${name}"`), name);
  }
  // 窓口の名前（画面が呼ぶもの・Rust が受けるもの）
  const cmds = read("src/ipc/commands.ts");
  const lib = read("src-tauri/src/lib.rs");
  for (const name of ["legacy_reread_start", "legacy_reread_cancel", "legacy_reread_status"]) {
    assert.ok(cmds.includes(`"${name}"`), name);
    assert.ok(lib.includes(`commands::library::${name},`), name);
  }
  // エラーの種別（Rust の ErrorKind は snake_case で画面へ渡る）
  const err = read("src-tauri/src/error.rs");
  assert.ok(err.includes("LegacyRereadBusy,") && err.includes("LibraryImportBusy,"));
  assert.ok(ts.includes('"legacy_reread_busy"') && ts.includes('"library_import_busy"'));
  // 取り込みの入口が断るときの知らせの記号
  assert.ok(rs.includes('Err("import_refused_legacy_reread")'));
});

test("設定画面：ライブラリのタブに置き、押すと確認を出してから始める。走っている間は押せず、やめるを出す", () => {
  const view = read("src/views/SettingsView.tsx");
  const tab = view.slice(view.indexOf('{tab === "library" ? ('), view.indexOf('{tab === "lyrics" ? ('));
  assert.ok(tab.includes("<LegacyRereadField />"), "ライブラリのタブにある");
  const field = view.slice(view.indexOf("function LegacyRereadField()"), view.indexOf("const BG_ZONE"));
  assert.ok(field.includes("古い文字コードのタグを読み直す…"));
  assert.ok(field.includes("disabled={running}"));
  assert.ok(field.includes("cancelLegacyReread()") && field.includes("やめる"));
  const confirmAt = field.indexOf("confirmDialog(c.title, c.body, c.ok)");
  assert.ok(confirmAt > 0 && confirmAt < field.indexOf("startLegacyReread()"), "確認のあとで始める");
  // 曲名は画面に出さない（件数だけ）
  assert.ok(!field.includes(".title}"));
});
