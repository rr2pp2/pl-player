// v3.7.0（デザインの見直し）の決まりを確かめる：危険な確認の最初のフォーカス・お気に入りの星の色・行のボタンの面・選んだ行と項目の線・
// 再生バーのボタンの大きさ・設定の欄とタブ・メニュー・操作部の輪郭・動きを減らす設定・外の資源を読まないこと・確認の文。
// 見た目そのものは画面で確かめる。ここでは、決めた値と決まりが CSS と部品に入っていて、変えないと決めたものが変わっていないことを見る。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/ui-design.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { confirmInitialFocus } from "../src/components/dialogFocus.ts";
import { trashConfirmText } from "../src/layout/visibility.ts";

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");
const css = read("src/theme/base.css");
/** `sel {` で始まる決まりの中身（最初の1つ）。sel は行の頭から書いたもの */
const rule = (sel: string) => {
  const at = css.indexOf("\n" + sel + " {");
  assert.ok(at >= 0, sel);
  return css.slice(at, css.indexOf("}", at));
};

test("危険な確認（danger）だけ、最初のフォーカスを「キャンセル」に置く。危険でない確認は今までどおり確定のボタン", () => {
  assert.equal(confirmInitialFocus(true), "cancel");
  assert.equal(confirmInitialFocus(false), "ok");
  assert.equal(confirmInitialFocus(undefined), "ok");
  const modal = read("src/components/Modal.tsx");
  const host = modal.slice(modal.indexOf("export function DialogHost()"));
  assert.match(host, /const focus = confirmInitialFocus\(d\.danger\);/);
  // キャンセルのボタン：押すと false（処理は変えない）。最初のフォーカスは danger のとき
  assert.match(host, /<button className="btn" onClick=\{\(\) => done\(false\)\} autoFocus=\{focus === "cancel"\}>\s*キャンセル/);
  // 確定のボタン：押すと true。危険なら .btn.danger（赤で塗らない）、そうでなければ .btn.primary
  assert.match(host, /<button className=\{"btn " \+ \(d\.danger \? "danger" : "primary"\)\} onClick=\{\(\) => done\(true\)\} autoFocus=\{focus === "ok"\}>/);
  // autoFocus は2つのボタンのどちらか片方だけに効く（無条件の autoFocus を残していない）
  assert.equal((host.match(/autoFocus/g) ?? []).length, 2);
  assert.ok(!/autoFocus>/.test(host) && !/autoFocus\s*\n/.test(host), "no unconditional autoFocus");
  // 危険な確認を出しているところは、どれも danger を渡している（ごみ箱へ移す・ライブラリから外す・プレイリストの削除・キューを全削除・歌詞）
  const ta = read("src/components/trackActions.tsx");
  assert.match(ta, /confirmDialog\(c\.title, c\.body, c\.ok, true\)/);
  assert.match(ta, /"外す",\s*true,\s*\)/);
  assert.match(read("src/layout/Sidebar.tsx"), /confirmDialog\("プレイリストを削除", [^;]*"削除", true\)/);
  assert.match(read("src/views/QueueView.tsx"), /confirmDialog\("キューを全削除", "キューの曲をすべて外します。", "全削除", true\)/);
  // 確定のボタンの見た目：危険の色の文字と輪郭（塗らない）
  const danger = rule(".btn.danger");
  assert.match(danger, /color: var\(--c-danger\);/);
  assert.match(danger, /border-color: var\(--c-danger\);/);
  assert.ok(!/background/.test(danger), "not filled red");
});

test("確認の文：ごみ箱へ移す・ライブラリから外すで消えるものを、事実どおりに書く（年間まとめの記録も消える。3.6.0）", () => {
  const one = trashConfirmText([{ title: "架空の曲", path: "D:\\Music\\架空の曲.mp3" }]);
  assert.match(one.body, /Windows のごみ箱へ移します/);
  assert.match(one.body, /お気に入り・この曲の歌詞の設定・年間まとめの再生の記録からも外れます/);
  assert.match(one.body, /ごみ箱から戻せます/);
  const ta = read("src/components/trackActions.tsx");
  assert.match(ta, /音声ファイルそのものは消しません。プレイリストとキューからも外れます。\\nお気に入りの印・この曲の歌詞（手で入れたもの・取り込んだもの）と時刻の補正・年間まとめの再生の記録も消えます/);
  // 消えることの裏づけ：記録・歌詞・補正の表は、曲を外すと一緒に消える（ON DELETE CASCADE）。お気に入りは曲の行そのものの印
  const mig = ["001_init.sql", "002_lyrics.sql", "006_play_history.sql"].map((f) => read(`src-tauri/src/db/migrations/${f}`)).join("\n");
  for (const t of ["lyrics_sources", "track_lyrics_settings", "play_history"]) {
    assert.match(mig, new RegExp(`CREATE TABLE IF NOT EXISTS ${t} \\([\\s\\S]*?REFERENCES tracks\\(id\\) ON DELETE CASCADE`), t);
  }
  assert.match(mig, /favorite\s+INTEGER NOT NULL DEFAULT 0,/);
});

test("お気に入りの星：オンの星だけを黄色（--c-favorite）に。ほかのオンの印（シャッフル・リピート・ミュート・歌詞）はアクセントの色のまま", () => {
  assert.match(rule(".icon-btn.on"), /color: var\(--c-accent\);/);
  const on = css.slice(css.indexOf("\n.t-fav .icon-btn.on,"), css.indexOf("}", css.indexOf("\n.t-fav .icon-btn.on,")));
  assert.match(on, /\.t-fav \.icon-btn\.on:hover \{/);
  assert.match(on, /color: var\(--c-favorite\);/);
  assert.match(on, /background: var\(--c-selected\);/);
  // --c-favorite を使うのはお気に入りの列のボタンだけ
  const uses = [...css.matchAll(/var\(--c-favorite\)/g)].map((m) => css.lastIndexOf("\n.", m.index!));
  assert.equal(uses.length, 1);
  assert.ok(css.slice(uses[0], uses[0] + 20).startsWith("\n.t-fav"), "only in the favorite column");
  // 行のボタンは今までどおり：オン・オフで class の on・aria-pressed・塗った星（スキンの状態 filled／outline）
  const tt = read("src/components/TrackTable.tsx");
  assert.match(tt, /className=\{"icon-btn small" \+ \(t\.favorite \? " on" : ""\)\}/);
  assert.match(tt, /aria-pressed=\{t\.favorite\}/);
  assert.match(tt, /<IconStar filled=\{t\.favorite\} \/>/);
  // 見出しは文字「お気に入り」のまま 68px（星＋文字の 84px は採らない）
  assert.match(tt, /\{cols\.favorite \? <span className="col-fav">お気に入り<\/span> : null\}/);
  assert.match(tt, /cols\.favorite \? "68px" : null,/);
  // サイドバーの「お気に入り」の星は、ほかの項目と同じ色の決まり（オンの印ではない）
  assert.match(read("src/layout/Sidebar.tsx"), /\{ view: "favorites", label: "お気に入り", icon: <IconStar \/> \}/);
});

test("行のボタン（お気に入り・ごみ箱）：32px の面と操作部の輪郭。オフは中間の色、ごみ箱は指を乗せると危険の色。コンパクト表示は 28px。スキンの画像には色を掛けない", () => {
  const both = css.slice(css.indexOf("\n.t-fav .icon-btn,\n.t-trash .icon-btn {"), css.indexOf("}", css.indexOf("\n.t-fav .icon-btn,\n.t-trash .icon-btn {")));
  assert.match(both, /width: 32px;/);
  assert.match(both, /height: 32px;/);
  assert.match(both, /border: 1px solid var\(--c-outline\);/);
  assert.match(both, /color: var\(--c-secondary\);/);
  const th = rule(".t-trash .icon-btn:hover");
  assert.match(th, /color: var\(--c-danger\);/);
  assert.match(th, /border-color: var\(--c-danger\);/);
  // ごみ箱の見出しは文字「ごみ箱」のまま（アイコンだけにしない）。行のボタンは読み上げと吹き出しを持つ
  const tt = read("src/components/TrackTable.tsx");
  assert.match(tt, /\{cols\.trash \? <span className="col-trash">ごみ箱<\/span> : null\}/);
  assert.match(tt, /aria-label=\{p\.trashLabel\}/);
  // スキンの画像（img.skin-icon）に、色を変える決まり（filter）を足していない。行・メニュー・ボタンのどこにも
  assert.ok(!/filter:\s*(?!blur)/.test(css.replace(/backdrop-filter: blur\([^)]*\) saturate\([^)]*\);/g, "").replace(/filter: blur\(var\(--bg-blur\)\);/g, "").replace(/filter: brightness\(1\.08\);/g, "")), "no color filter");
  assert.match(rule("img.skin-icon"), /object-fit: contain;/);
});

test("選んだ行と選んだ項目：背景の色に加えて、左端に 3px のアクセントの線（色だけに頼らない）", () => {
  const row = rule(".track-row.selected::before");
  for (const d of ["position: absolute;", "left: 0;", "width: 3px;", "background: var(--c-accent);"]) assert.ok(row.includes(d), d);
  assert.match(rule(".track-row.selected"), /background: var\(--c-selected\);/);
  // 年間まとめ（3.6.0）も同じ TrackTable の行なので、同じ決まりが効く（年間まとめだけの選び方の決まりを作っていない）
  assert.ok(!/\.(t-rank|t-plays)[^{]*selected/.test(css));
  assert.match(read("src/views/YearlyView.tsx"), /<TrackTable\b/);
  // サイドバー：最小の高さ 40px（高さは固定しない）・選んだ項目の線と少し太い字・たたんだときは 2px（32px のアイコンに重ねない）
  const item = rule(".nav-item");
  assert.match(item, /min-height: 40px;/);
  assert.match(item, /position: relative;/);
  assert.match(rule(".nav-item.active"), /font-weight: 600;/);
  const line = rule(".nav-item.active::before");
  assert.match(line, /width: 3px;/);
  assert.match(line, /background: var\(--c-accent\);/);
  assert.match(rule(".sidebar-collapsed .sidebar .nav-item.active::before"), /width: 2px;/);
  // サイドバーの幅は変えない（236px・たたんで 64px）
  const tokens = read("src/theme/tokens.css");
  assert.match(tokens, /--sidebar-w: 236px;/);
  assert.match(tokens, /--sidebar-w-collapsed: 64px;/);
});

test("再生バー：再生・一時停止 44px、ほかのボタン 36px、間 4px。済んだところの帯はいつも見え、つまみは指を乗せる・キーボードで移ると出る", () => {
  assert.match(rule(".player-controls"), /gap: 4px;/);
  assert.match(rule(".player-controls .icon-btn"), /width: 36px;\s*height: 36px;/);
  const play = rule(".player-controls .play-btn");
  assert.match(play, /width: 44px;\s*height: 44px;/);
  // 帯：済んだところはアクセントの色（つまみの有無にかかわらず）
  assert.match(rule(".range::-webkit-slider-runnable-track"), /linear-gradient\(to right, var\(--c-accent\) var\(--pct\), var\(--c-border\) var\(--pct\)\)/);
  assert.match(css, /\.range:hover::-webkit-slider-thumb,\n\.range:focus-visible::-webkit-slider-thumb \{\s*opacity: 1;/);
  assert.match(rule(".range:focus-visible"), /outline: 2px solid var\(--c-accent\);/);
  // 再生バーの高さ 80px（狭いときの 76px は変えない。v3.8.0：狭い窓の @media ではなく、1段の形（.player.one-row）のとき）
  assert.match(read("src/theme/tokens.css"), /--player-h: 80px;/);
  assert.match(rule(":root:has(.player.one-row)"), /--player-h: 76px;/);
  // v3.8.0：右の列（全体の音量・歌詞のボタン・「···」）のボタンも 36px
  assert.match(rule(".player-right .icon-btn"), /width: 36px;\s*height: 36px;/);
});

test("設定：欄は 760px 以下で1列。チェックを並べた欄は項目名を1行目にそろえる。タブは選んだものに下の線と面、フォーカスの枠", () => {
  const at = css.indexOf("@media (max-width: 760px) {");
  assert.ok(at > 0);
  const block = css.slice(at, css.indexOf("\n}\n", at));
  assert.match(block, /\.field \{\s*grid-template-columns: minmax\(0, 1fr\);/);
  assert.match(block, /\.field \.hint \{\s*grid-column: 1;/);
  assert.match(css, /\n\.field\.field-top,\n\.field:has\(> \.row > label\.row\) \{\s*align-items: start;/);
  // 長い文のチェック（check-long）はそのまま
  assert.match(rule(".row.check-long"), /flex-wrap: nowrap;/);
  const sel = rule('.settings-tab[aria-selected="true"]');
  assert.match(sel, /background: var\(--c-selected\);/);
  assert.match(sel, /box-shadow: inset 0 -2px 0 var\(--c-accent\);/);
  assert.ok(!/font-weight/.test(sel), "the tab width does not change when selected");
  assert.match(rule(".settings-tab:focus-visible"), /outline: 2px solid var\(--c-accent\);/);
  // 並べ替え・折り返しの部品は変えない
  const tabs = read("src/views/SettingsTabs.tsx");
  assert.match(tabs, /strategy=\{rectSortingStrategy\}/);
  assert.match(tabs, /元の順番に戻す/);
  // 設定の初期化は足さない
  assert.ok(!/設定を初期化|初期設定に戻す/.test(read("src/views/SettingsView.tsx")));
});

test("メニュー：項目の高さ 36px 以上・指を乗せたときとキーボードの面・危険な項目はアイコンも危険の色", () => {
  assert.match(rule(".menu-item"), /min-height: 36px;/);
  assert.match(css, /\n\.menu-item:hover,\n\.menu-item:focus-visible,\n\.menu-item\.open \{\s*background: var\(--c-hover\);/);
  assert.match(rule(".menu-item.danger .icon"), /color: var\(--c-danger\);/);
});

test("操作部の輪郭は --c-outline（ボタン・入力欄・選ぶボタンの組・キーの欄・検索欄）。--c-border は飾りの区切り線に残す", () => {
  for (const sel of [".btn", ".seg", ".key-box", ".search"]) assert.match(rule(sel), /border: 1px solid var\(--c-outline\);/, sel);
  assert.match(css, /\n\.input,\nselect\.input,\ntextarea\.input \{[^}]*border: 1px solid var\(--c-outline\);/);
  assert.match(rule(".menu-sep"), /background: var\(--c-border\);/);
  assert.match(rule(".topbar"), /border-bottom: 1px solid var\(--c-border\);/);
});

test("動きを減らす設定：足した移り変わりを止め、再生ボタンの拡大を止める", () => {
  const at = css.lastIndexOf("@media (prefers-reduced-motion: reduce) {");
  const block = css.slice(at, css.indexOf("\n}\n", at));
  for (const sel of [".icon-btn", ".btn", ".nav-item", ".track-row", ".seg button", ".settings-tab", ".search"]) {
    assert.ok(new RegExp(`\\n  ${sel.replace(".", "\\.")}[,\\s]`).test(block), sel);
  }
  assert.match(block, /\.player-controls \.play-btn:hover \{\s*transform: none;/);
  // 新しい移り変わりは、決まった時間の変数（--dur-*）を使う
  for (const m of css.matchAll(/transition: ([^;]+);/g)) {
    if (m[1] === "none") continue;
    assert.ok(/var\(--dur-[a-z]+\)|200ms linear/.test(m[1]), m[1]);
  }
});

test("外の資源を読まない：テーマと骨組みの CSS に、外のフォント・URL・@import を足していない", () => {
  for (const f of ["src/theme/base.css", "src/theme/tokens.css", "src/theme/dark.css", "src/theme/light.css"]) {
    const s = read(f);
    assert.ok(!/@import|@font-face|https?:\/\/|url\(/i.test(s), f);
  }
  // 依存を足していない（3.6.0 と同じ7つ）
  const pkg = JSON.parse(read("package.json"));
  assert.deepEqual(Object.keys(pkg.dependencies).sort(), [
    "@dnd-kit/core",
    "@dnd-kit/sortable",
    "@tanstack/react-virtual",
    "@tauri-apps/api",
    "@tauri-apps/plugin-global-shortcut",
    "react",
    "react-dom",
  ]);
});
