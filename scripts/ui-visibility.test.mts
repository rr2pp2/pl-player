// v3.3.0：隠す設定（再生バーのボタン・サイドバーの項目）・サイドバーの切り替えボタン・ごみ箱へ移すときの対象と確認の文・
// 設定のタブの並びの合わせ方・吹き出し（Tip）の包み方を確かめる。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/ui-visibility.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  DEFAULT_TAB_ORDER,
  fallbackView,
  isDefaultTabOrder,
  mergeTabOrder,
  moveTab,
  PLAYER_BUTTONS,
  PLAYER_FIXED,
  playerButtonShown,
  SETTINGS_TABS,
  SIDEBAR_AUTO_HINT,
  SIDEBAR_FIXED,
  SIDEBAR_ITEMS,
  sidebarItemShown,
  sidebarToggleInfo,
  trashButtonShown,
  trashConfirmText,
  trashMenuShown,
  TRASH_LISTS,
  TRASH_MAX_TRACKS,
  trashTargets,
  trashTooMany,
  TRASH_LIST_MAX,
  uniqueIds,
} from "../src/layout/visibility.ts";
import { tipParts } from "../src/components/tipParts.ts";

test("古い設定（値が無い）では全部出す", () => {
  for (const ui of [undefined, null, {}, { playerButtons: {} }, { sidebarItems: {} }]) {
    for (const b of PLAYER_BUTTONS) assert.equal(playerButtonShown(ui as any, b.id), true, b.id);
    for (const s of SIDEBAR_ITEMS) assert.equal(sidebarItemShown(ui as any, s.id), true, s.id);
  }
  // true・false 以外の値（壊れた設定）も出す側
  assert.equal(playerButtonShown({ playerButtons: { shuffle: null as any } }, "shuffle"), true);
  assert.equal(playerButtonShown({ playerButtons: { shuffle: false } }, "shuffle"), false);
  assert.equal(sidebarItemShown({ sidebarItems: { genre: false } }, "genre"), false);
  assert.equal(sidebarItemShown({ sidebarItems: { genre: false } }, "artist"), true);
});

test("隠せないもの（再生・一時停止・前・次・シークバー・設定）は、値があっても隠れない", () => {
  const allOff = (ids: string[]) => Object.fromEntries(ids.map((x) => [x, false]));
  const ui = {
    playerButtons: allOff([...PLAYER_FIXED, "seekBar", "play", ...PLAYER_BUTTONS.map((b) => b.id)]),
    sidebarItems: allOff([...SIDEBAR_FIXED, "Settings", ...SIDEBAR_ITEMS.map((s) => s.id)]),
  };
  for (const id of [...PLAYER_FIXED, "seekBar", "play"]) assert.equal(playerButtonShown(ui, id), true, id);
  for (const id of [...SIDEBAR_FIXED, "Settings"]) assert.equal(sidebarItemShown(ui, id), true, id);
  // 隠せるものは隠れる
  for (const b of PLAYER_BUTTONS) assert.equal(playerButtonShown(ui, b.id), false, b.id);
  for (const s of SIDEBAR_ITEMS) assert.equal(sidebarItemShown(ui, s.id), false, s.id);
  // 隠せる一覧に、隠せないものが混ざっていない
  for (const id of PLAYER_FIXED) assert.ok(!PLAYER_BUTTONS.some((b) => b.id === id), id);
  assert.ok(!SIDEBAR_ITEMS.some((s) => s.id === "settings"));
});

test("隠せる名前は Rust（settings\\store.rs）と同じ", () => {
  const rs = readFileSync(new URL("../src-tauri/src/settings/store.rs", import.meta.url), "utf8");
  const list = (name: string) => {
    const m = rs.match(new RegExp(`pub const ${name}: &\\[&str\\] =\\s*&\\[([^\\]]*)\\]`));
    assert.ok(m, name);
    return [...m![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  };
  assert.deepEqual(list("PLAYER_BUTTONS_HIDEABLE"), PLAYER_BUTTONS.map((b) => b.id));
  assert.deepEqual(list("SIDEBAR_ITEMS_HIDEABLE"), SIDEBAR_ITEMS.map((s) => s.id));
});

test("開いている画面の入口を隠したら、出している最初の項目へ移る", () => {
  assert.equal(fallbackView({}, "all"), null);
  assert.equal(fallbackView({ sidebarItems: { all: false } }, "all"), "artist");
  assert.equal(fallbackView({ sidebarItems: { all: false, artist: false } }, "artist"), "album");
  // 隠した項目とは別の画面なら移らない
  assert.equal(fallbackView({ sidebarItems: { genre: false } }, "album"), null);
  // 設定・歌詞は移らない（設定は隠せない。歌詞は再生バーから開く）
  assert.equal(fallbackView({ sidebarItems: Object.fromEntries(SIDEBAR_ITEMS.map((s) => [s.id, false])) }, "settings"), null);
  assert.equal(fallbackView({ sidebarItems: { all: false } }, "lyrics"), null);
  // プレイリストの節・キューを隠したとき
  assert.equal(fallbackView({ sidebarItems: { playlists: false } }, "playlist"), "all");
  assert.equal(fallbackView({ sidebarItems: { queue: false } }, "queue"), "all");
  // LIBRARY を全部隠したら「すべての曲」の画面（入口は無いまま）
  const none = Object.fromEntries(SIDEBAR_ITEMS.map((s) => [s.id, false]));
  assert.equal(fallbackView({ sidebarItems: none }, "favorites"), "all");
  // 「すべての曲」だけを出していて、ほかを隠したら、そこへ
  assert.equal(fallbackView({ sidebarItems: { ...none, all: true } }, "genre"), "all");
});

test("サイドバーの切り替えボタン：狭いときは押せない見た目と、理由の吹き出し", () => {
  const narrow = sidebarToggleInfo(true, true);
  assert.equal(narrow.disabled, true);
  assert.equal(narrow.hint, SIDEBAR_AUTO_HINT);
  assert.match(narrow.hint!, /狭い/);
  assert.match(narrow.hint!, /幅を広げると/);
  const wide = sidebarToggleInfo(false, false);
  assert.equal(wide.disabled, false);
  assert.equal(wide.hint, null);
  assert.equal(wide.label, "サイドバーをたたむ");
  assert.equal(sidebarToggleInfo(false, true).label, "サイドバーを広げる");
});

test("ごみ箱へ移す対象：選んでいる行の中を押したら全部、外なら押した行だけ", () => {
  assert.deepEqual(trashTargets(new Set([5, 2, 9]), 2), [2, 5, 9]);
  assert.deepEqual(trashTargets(new Set([5, 2, 9]), 3), [3]);
  assert.deepEqual(trashTargets(new Set(), 7), [7]);
  assert.deepEqual(trashTargets([4], 4), [4]);
  assert.deepEqual(uniqueIds([3, 1, 3, 2, 1]), [3, 1, 2]);
});

test("ごみ箱へ移す確認の文：曲名・ファイルの場所・戻せること・ごみ箱の無い場所", () => {
  const one = trashConfirmText([{ title: "架空の曲", path: "D:\\Music\\架空の曲.mp3" }]);
  assert.equal(one.title, "曲をごみ箱へ移す");
  assert.equal(one.ok, "ごみ箱へ移す");
  assert.match(one.body, /架空の曲/);
  assert.match(one.body, /D:\\Music\\架空の曲\.mp3/);
  assert.match(one.body, /ごみ箱から戻せます/);
  assert.match(one.body, /完全に消えてしまうので移しません/);
  const many = trashConfirmText(Array.from({ length: 8 }, (_, i) => ({ title: `曲${i}`, path: `D:\\m\\${i}.mp3` })));
  assert.equal(many.title, "8 曲をごみ箱へ移す");
  assert.match(many.body, /選んでいる 8 曲/);
  assert.match(many.body, new RegExp(`ほか ${8 - TRASH_LIST_MAX} 曲`));
  assert.ok(many.body.includes("曲4") && !many.body.includes("曲5"));
});

test("ごみ箱へ移すを出す場所：ライブラリの曲一覧だけ。右クリックはいつも、行のボタンは設定がオンのときだけ", () => {
  const on = { trashButton: true };
  assert.equal(trashButtonShown(on, "library"), true);
  for (const l of ["playlist", "queue", "playerBar"] as const) assert.equal(trashButtonShown(on, l), false, l);
  // 設定がオフ・古い設定（値なし）・壊れた値なら、どこにも出さない
  for (const ui of [{ trashButton: false }, {}, undefined, null, { trashButton: "true" as any }]) {
    for (const l of ["library", "playlist", "queue", "playerBar"] as const) assert.equal(trashButtonShown(ui as any, l), false, `${l}`);
  }
  assert.deepEqual([...TRASH_LISTS], ["library"]);
  // 右クリックのメニュー：設定にかかわらず、ライブラリの曲一覧ならいつも出す。ほかには出さない
  assert.equal(trashMenuShown("library"), true);
  for (const l of ["playlist", "queue", "playerBar"] as const) assert.equal(trashMenuShown(l), false, l);
  // 画面のどの一覧が、どの場所として描かれているか（TrackTable の list と、再生バーの右クリック）
  const src = (f: string) => readFileSync(new URL(`../src/${f}`, import.meta.url), "utf8");
  const listOf = (f: string) => [...src(f).matchAll(/<TrackTable[\s\S]*?\blist="([a-zA-Z]+)"/g)].map((m) => m[1]);
  assert.deepEqual(listOf("views/LibraryView.tsx"), ["library"]);
  assert.deepEqual(listOf("views/PlaylistView.tsx"), ["playlist"]);
  assert.deepEqual(listOf("views/QueueView.tsx"), ["queue"]);
  assert.match(src("layout/PlayerBar.tsx"), /trackMenu\(\{ tracks: \[t\], list: "playerBar" \}\)/);
  // 出すかどうかは visibility.ts の関数だけで決める（設定の値を直に見て出す所が無い）。
  // 行のボタン（TrackTable）は trashButtonShown、右クリック（trackActions）は trashMenuShown で、設定を見ない
  for (const f of ["components/TrackTable.tsx", "components/trackActions.tsx"]) assert.ok(!/trashButton\s*===/.test(src(f)), f);
  assert.match(src("components/TrackTable.tsx"), /trashButtonShown\(ui, props\.list\)/);
  const menu = src("components/trackActions.tsx");
  assert.match(menu, /trashMenuShown\(opts\.list\)/);
  assert.ok(!/trashButtonShown|settings\.get\(\)/.test(menu), "the menu does not depend on the setting");
  // 右クリックには「ライブラリから外す…」と「ごみ箱へ移す…」が両方並ぶ（どちらも確認を出す）
  assert.ok(menu.indexOf('"ライブラリから外す…"') >= 0 && menu.indexOf('"ごみ箱へ移す…"') > menu.indexOf('"ライブラリから外す…"'));
  // Rust の窓口は設定を見ない（右クリックからの操作を通すため。確認は画面が必ず出す）
  const cmd = readFileSync(new URL("../src-tauri/src/commands/library.rs", import.meta.url), "utf8");
  const body = cmd.slice(cmd.indexOf("pub async fn library_trash_tracks"));
  const fnEnd = body.search(/\r?\n\}\r?\n/);
  assert.ok(fnEnd > 0);
  assert.ok(!/trash_button/.test(body.slice(0, fnEnd)), "library_trash_tracks does not read ui.trash_button");
});

test("ごみ箱の列：見出しの行に「ごみ箱」、行のボタンはアイコンだけ（読み上げと吹き出しは残す）。列の幅は窓の幅にかかわらず同じ。お気に入りの見出しは文字「お気に入り」", () => {
  const tt = readFileSync(new URL("../src/components/TrackTable.tsx", import.meta.url), "utf8");
  // 見出し：「時間」「ジャンル」と同じ見出しの行に出す
  assert.match(tt, /\{cols\.trash \? <span className="col-trash">ごみ箱<\/span> : null\}/);
  // 列の幅は1つの値だけ（窓の幅で出し分けない）。見出しの「ごみ箱」（12px の字で3字＝36px）が切れない幅
  const m = tt.match(/cols\.trash \? "(\d+)px" : null/);
  assert.ok(m, "one fixed width for the trash column");
  assert.ok(Number(m![1]) >= 36);
  assert.ok(!/trashIconOnly/.test(tt));
  const ls = readFileSync(new URL("../src/components/useLayoutSize.ts", import.meta.url), "utf8");
  assert.ok(!/trash/i.test(ls));
  // 行のボタン：アイコンだけ。文字は出さず、読み上げ（aria-label）と吹き出し（title）で伝える
  const row = tt.slice(tt.indexOf('<div className="t-trash">'), tt.indexOf("</div>", tt.indexOf('<div className="t-trash">')));
  assert.match(row, /className="icon-btn small"/);
  assert.match(row, /aria-label=\{p\.trashLabel\}/);
  assert.match(row, /title=\{`\$\{p\.trashLabel\}/);
  assert.ok(!/ごみ箱へ<\/span>/.test(row), "no text label in the row button");
  // お気に入りの列も同じ見出しの行に「お気に入り」（12px の字で5字＝60px が切れない幅）。
  // v3.5.0 で一度星のマークにしたが、文字に戻した（星は行のボタンとサイドバーに出す）
  assert.match(tt, /\{cols\.favorite \? <span className="col-fav">お気に入り<\/span> : null\}/);
  const f = tt.match(/cols\.favorite \? "(\d+)px" : null/);
  assert.ok(f && Number(f[1]) >= 60, "favorite column fits its header");
});

test("設定の見出し：いちばん上の帯の左（検索欄の場所）に出し、設定の画面の中には見出しの行を置かない", () => {
  const top = readFileSync(new URL("../src/layout/TopBar.tsx", import.meta.url), "utf8");
  assert.match(top, /r\.view === "settings" \? <h1 className="view-title topbar-title">設定<\/h1> : null/);
  const view = readFileSync(new URL("../src/views/SettingsView.tsx", import.meta.url), "utf8");
  assert.ok(!/view-head/.test(view), "no heading row inside the settings view");
  assert.ok(!/>設定<\/h1>/.test(view));
  // ほかの画面は、今までどおり画面の中に見出しの行を持つ
  for (const f of ["LibraryView", "PlaylistView", "QueueView"]) {
    assert.match(readFileSync(new URL(`../src/views/${f}.tsx`, import.meta.url), "utf8"), /className="view-head"/, f);
  }
});

test("設定のタブ：複数行に折り返す（横に送らない）。並べ替えは格子向きの rectSortingStrategy で、縦横に動かす", () => {
  const tabs = readFileSync(new URL("../src/views/SettingsTabs.tsx", import.meta.url), "utf8");
  assert.match(tabs, /strategy=\{rectSortingStrategy\}/);
  assert.ok(!/import[^;]*horizontalListSortingStrategy/.test(tabs), "the one-row strategy is not used");
  assert.ok(!/scrollIntoView/.test(tabs), "no horizontal scrolling to the active tab");
  // 位置は縦横とも動かす（行をまたぐため）
  assert.match(tabs, /translate3d\(\$\{Math\.round\(s\.transform\.x\)\}px, \$\{Math\.round\(s\.transform\.y\)\}px, 0\)/);
  const css = readFileSync(new URL("../src/theme/base.css", import.meta.url), "utf8");
  const rule = (sel: string) => {
    const at = css.indexOf("\n" + sel + " {");
    assert.ok(at >= 0, sel);
    return css.slice(at, css.indexOf("}", at));
  };
  const list = rule(".settings-tablist");
  assert.match(list, /flex-wrap: wrap;/);
  assert.ok(!/overflow-x/.test(list));
  assert.match(rule(".settings-tabs"), /flex-wrap: wrap;/);
});

test("ごみ箱へ移す数の上限：Rust と同じ 1,000 曲。超えたら確認の前に断る", () => {
  const rs = readFileSync(new URL("../src-tauri/src/library/trash.rs", import.meta.url), "utf8");
  const m = rs.match(/pub const MAX_TRACKS: usize = (\d+);/);
  assert.ok(m);
  assert.equal(TRASH_MAX_TRACKS, Number(m![1]));
  assert.equal(TRASH_MAX_TRACKS, 1000);
  assert.equal(trashTooMany(1000), false);
  assert.equal(trashTooMany(1001), true);
  assert.equal(trashTooMany(1), false);
  // 断るときの文（Rust の種別 trash_too_many と同じ文を使う）
  const ja = readFileSync(new URL("../src/messages/ja.ts", import.meta.url), "utf8");
  assert.match(ja, /trash_too_many: "一度にごみ箱へ移せるのは 1,000 曲まで/);
  const flow = readFileSync(new URL("../src/components/trackActions.tsx", import.meta.url), "utf8");
  assert.ok(flow.indexOf("trashTooMany(list.length)") >= 0 && flow.indexOf("trashTooMany(list.length)") < flow.indexOf("confirmDialog(c.title"), "checked before the confirmation");
});

test("設定のタブ：古い設定（並びが無い）は既定の順", () => {
  for (const v of [undefined, null, [], "display", 3, {}]) assert.deepEqual(mergeTabOrder(v), DEFAULT_TAB_ORDER);
  assert.ok(isDefaultTabOrder(mergeTabOrder(undefined)));
  assert.equal(DEFAULT_TAB_ORDER.length, SETTINGS_TABS.length);
  assert.equal(new Set(DEFAULT_TAB_ORDER).size, DEFAULT_TAB_ORDER.length);
});

test("設定のタブ：保存した順を守り、知らない名前・重なりを捨て、抜けたタブを足す（全部が1回ずつ出る）", () => {
  const all = ["a", "b", "c", "d", "e"];
  // そのまま
  assert.deepEqual(mergeTabOrder(["e", "d", "c", "b", "a"], all), ["e", "d", "c", "b", "a"]);
  // 知らない名前（前の版・後の版）と重なりと文字列でないもの
  assert.deepEqual(mergeTabOrder(["c", "zzz", "a", "c", 1, null, "b", "e", "d"], all), ["c", "a", "b", "e", "d"]);
  // 抜けたタブ（版が上がって増えた）：既定の順で前にあるタブのすぐ後ろ
  assert.deepEqual(mergeTabOrder(["e", "a", "d"], all), ["e", "a", "b", "c", "d"]);
  assert.deepEqual(mergeTabOrder(["d", "c"], all), ["a", "b", "d", "e", "c"]);
  // 先頭が抜けたら先頭へ。前のタブが後ろへ動いていれば、そのすぐ後ろ
  assert.deepEqual(mergeTabOrder(["c", "b"], all), ["a", "c", "d", "e", "b"]);
  // どんな入力でも、今のタブが全部1回ずつ
  const inputs: unknown[] = [["x"], ["e", "e", "e"], all.slice().reverse(), ["b", "y", "a"], [], "e"];
  for (const v of inputs) {
    const out = mergeTabOrder(v, all);
    assert.deepEqual([...out].sort(), [...all].sort(), JSON.stringify(v));
  }
  // 本物のタブでも
  const real = mergeTabOrder(["keys", "removed_tab", "display"]);
  assert.deepEqual([...real].sort(), [...DEFAULT_TAB_ORDER].sort());
  assert.ok(real.indexOf("keys") < real.indexOf("display"));
});

test("設定のタブ：並べ替え", () => {
  const o = ["a", "b", "c", "d"];
  assert.deepEqual(moveTab(o, "a", "c"), ["b", "c", "a", "d"]);
  assert.deepEqual(moveTab(o, "d", "a"), ["d", "a", "b", "c"]);
  assert.deepEqual(moveTab(o, "b", "b"), o);
  assert.deepEqual(moveTab(o, "x", "a"), o);
  assert.ok(!isDefaultTabOrder(moveTab(DEFAULT_TAB_ORDER, DEFAULT_TAB_ORDER[0], DEFAULT_TAB_ORDER[2])));
});

test("吹き出しの包み方：文の有無で形を変えない（サイドバーを開け閉めしても、包んだボタンが作り直されない）", () => {
  // 文が無いとき：結びつけ・見えない文・吹き出しを出さないだけ
  for (const t of [null, undefined, ""]) {
    assert.deepEqual(tipParts(t, "d1"), { childProps: {}, srText: null, canShow: false }, String(t));
  }
  assert.deepEqual(tipParts("設定", "d1"), { childProps: { "aria-describedby": "d1" }, srText: "設定", canShow: true });
  // 画面（Tip.tsx）：文が無いときに子をそのまま返す近道が無く、いつも同じ span で包み、子へ渡す属性は tipParts から取る
  const tip = readFileSync(new URL("../src/components/Tip.tsx", import.meta.url), "utf8");
  assert.ok(!/return\s+props\.children/.test(tip), "no early return of the bare child");
  assert.match(tip, /tipParts\(props\.text, descId\)/);
  const returns = [...tip.matchAll(/return\s*\(\s*<(\w+)/g)].map((m) => m[1]);
  assert.deepEqual(returns, ["span"], "one shape: always wrapped in a span");
  assert.match(tip, /cloneElement\(props\.children, parts\.childProps\)/);
  // サイドバーは、たたんだときだけ文を渡す（広げたときは null）。それでも Tip で包む形は同じ
  const side = readFileSync(new URL("../src/layout/Sidebar.tsx", import.meta.url), "utf8");
  assert.match(side, /<Tip text=\{props\.collapsed \? props\.text : null\}/);
});
