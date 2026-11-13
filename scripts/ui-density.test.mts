// v3.4.0：表示の詰め方（曲一覧のコンパクト表示・サイドバーのアイコンの大きさ）を確かめる。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/ui-density.test.mts
// 仮想スクロールの部品（@tanstack/virtual-core）を実際に動かすテストが1つあるので、先に npm ci を済ませる。
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Virtualizer } from "@tanstack/virtual-core";
import {
  COMPACT_LIST_MODES,
  COMPACT_NARROW_BELOW,
  COMPACT_ROW_H,
  compactListMode,
  compactListOn,
  SIDEBAR_ICON_PX,
  sidebarIconPx,
  trackRowHeight,
} from "../src/layout/density.ts";

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");

test("コンパクト表示：古い設定（値が無い）・知らない値は「使わない」", () => {
  for (const ui of [undefined, null, {}, { compactList: undefined }, { compactList: null }, { compactList: "" }, { compactList: "big" }, { compactList: "Always" }, { compactList: 1 }, { compactList: true }, { compactList: ["always"] }]) {
    assert.equal(compactListMode(ui as any), "off", JSON.stringify(ui));
    for (const w of [300, 719, 720, 1400]) assert.equal(compactListOn(ui as any, w), false, `${JSON.stringify(ui)} ${w}`);
  }
  assert.deepEqual(
    COMPACT_LIST_MODES.map((m) => m.label),
    ["使わない", "いつも", "ウィンドウが狭いときだけ"],
  );
  assert.equal(COMPACT_LIST_MODES[0].id, "off", "既定（使わない）を先頭に出す");
});

test("コンパクト表示：いつも・狭いときだけ（720px 未満）", () => {
  for (const w of [320, 719, 720, 1100, 2560]) assert.equal(compactListOn({ compactList: "always" }, w), true, String(w));
  assert.equal(COMPACT_NARROW_BELOW, 720);
  assert.equal(compactListOn({ compactList: "narrow" }, 719), true);
  assert.equal(compactListOn({ compactList: "narrow" }, 719.5), true);
  assert.equal(compactListOn({ compactList: "narrow" }, 720), false);
  assert.equal(compactListOn({ compactList: "narrow" }, 1400), false);
  assert.equal(compactListOn({ compactList: "off" }, 300), false);
});

test("狭いときの境目は、ジャケットが縮む段（useLayoutSize の artSize と base.css の 719px）と同じ", () => {
  const ls = read("src/components/useLayoutSize.ts");
  const m = ls.match(/artSize: w < (\d+) \?/);
  assert.ok(m, "artSize の境目");
  assert.equal(Number(m![1]), COMPACT_NARROW_BELOW);
  const css = read("src/theme/base.css");
  assert.match(css, new RegExp(`@media \\(max-width: ${COMPACT_NARROW_BELOW - 1}px\\) \\{\\s*\\.hide-narrow`), "base.css の「3. ジャケットを縮める」");
});

test("行の高さ：コンパクトなら 36px（幅の段にかかわらず）、そうでなければ今までの高さ。行の中のボタンが収まる", () => {
  assert.equal(COMPACT_ROW_H, 36);
  for (const normal of [64, 48]) {
    assert.equal(trackRowHeight(true, normal), COMPACT_ROW_H);
    assert.equal(trackRowHeight(false, normal), normal);
  }
  // 行の中で一番高い部品：お気に入り・ごみ箱のボタン（.icon-btn.small）。上下に 4px 以上残す
  const css = read("src/theme/base.css");
  const small = css.match(/\.icon-btn\.small \{\s*width: (\d+)px;\s*height: (\d+)px;/);
  assert.ok(small, ".icon-btn.small");
  assert.ok(COMPACT_ROW_H >= Number(small![2]) + 8, `${COMPACT_ROW_H} >= ${small![2]} + 8`);
  // 今までの行（48・64px）より低い
  assert.ok(COMPACT_ROW_H < 48);
});

test("値の名前と範囲は Rust（settings\\store.rs）と同じ", () => {
  const rs = read("src-tauri/src/settings/store.rs");
  const list = rs.match(/pub const COMPACT_LIST_MODES: &\[&str\] =\s*&\[([^\]]*)\]/);
  assert.ok(list, "COMPACT_LIST_MODES");
  assert.deepEqual([...list![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]), COMPACT_LIST_MODES.map((m) => m.id));
  const num = (name: string) => {
    const m = rs.match(new RegExp(`pub const ${name}: u32 = (\\d+);`));
    assert.ok(m, name);
    return Number(m![1]);
  };
  assert.equal(num("SIDEBAR_ICON_PX_MIN"), SIDEBAR_ICON_PX.min);
  assert.equal(num("SIDEBAR_ICON_PX_MAX"), SIDEBAR_ICON_PX.max);
  assert.equal(num("SIDEBAR_ICON_PX_DEFAULT"), SIDEBAR_ICON_PX.default);
  // 画面の既定値（src\stores\settings.ts）も同じ
  const st = read("src/stores/settings.ts");
  assert.match(st, /compactList: "off",/);
  assert.match(st, new RegExp(`sidebarIconPx: ${SIDEBAR_ICON_PX.default},`));
});

test("サイドバーのアイコンの大きさ：既定は今の 20px。範囲の外は端へ、小数は四捨五入、数でなければ既定", () => {
  assert.deepEqual(SIDEBAR_ICON_PX, { min: 14, max: 32, default: 20 });
  // 既定は 3.3.0 までの大きさ（base.css の .nav-item .icon の値。--nav-icon が無いときの値）
  const css = read("src/theme/base.css");
  const rule = css.match(/\.nav-item \.icon \{([^}]*)\}/);
  assert.ok(rule, ".nav-item .icon");
  assert.match(rule![1], new RegExp(`width: var\\(--nav-icon, ${SIDEBAR_ICON_PX.default}px\\);`));
  assert.match(rule![1], new RegExp(`height: var\\(--nav-icon, ${SIDEBAR_ICON_PX.default}px\\);`));
  for (const ui of [undefined, null, {}, { sidebarIconPx: null }, { sidebarIconPx: "24" }, { sidebarIconPx: NaN }, { sidebarIconPx: Infinity }, { sidebarIconPx: true }]) {
    assert.equal(sidebarIconPx(ui as any), 20, JSON.stringify(ui));
  }
  const cases: [number, number][] = [[14, 14], [32, 32], [24, 24], [13, 14], [0, 14], [-5, 14], [33, 32], [1e9, 32], [20.4, 20], [20.6, 21], [31.6, 32]];
  for (const [v, want] of cases) assert.equal(sidebarIconPx({ sidebarIconPx: v }), want, String(v));
});

test("サイドバーのアイコン：たたんだサイドバーの幅に、いちばん大きい値でも収まる。スキンの画像にも同じ大きさが効く", () => {
  const tokens = read("src/theme/tokens.css");
  const css = read("src/theme/base.css");
  const collapsed = Number(tokens.match(/--sidebar-w-collapsed: (\d+)px;/)![1]);
  const side = css.match(/\.sidebar \{[^}]*padding: \d+px (\d+)px;/);
  assert.ok(side, ".sidebar の左右の余白");
  const scrollbar = Number(css.match(/::-webkit-scrollbar \{\s*width: (\d+)px;/)![1]);
  // 枠（.panel の 1px を左右）・左右の余白・項目の一覧のスクロールバーを引いた幅（たたんだ項目は左右の余白 0）
  const inner = collapsed - 2 - Number(side![1]) * 2 - scrollbar;
  assert.match(css, /\.sidebar-collapsed \.sidebar \.nav-item \{\s*justify-content: center;\s*padding: 8px 0;/);
  assert.ok(SIDEBAR_ICON_PX.max <= inner, `${SIDEBAR_ICON_PX.max} <= ${inner}`);
  // 項目の高さは固定しない（min-height だけ）ので、大きくすると行が伸びる。アイコンは縮まず、名前とは間を空ける
  const item = css.match(/\.nav-item \{([^}]*)\}/)![1];
  assert.match(item, /min-height: \d+px;/);
  assert.ok(!/\n\s*height:/.test(item), "no fixed height");
  assert.match(item, /gap: \d+px;/);
  assert.match(css.match(/\.nav-item \.icon \{([^}]*)\}/)![1], /flex: none;/);
  // スキンの画像（img.skin-icon。class は "icon skin-icon"）の 20px より、.nav-item .icon の方が強い（クラス2つ ＞ 要素1つ＋クラス1つ）
  assert.match(css, /img\.skin-icon \{\s*width: 20px;/);
  const icons = read("src/components/Icons.tsx");
  assert.match(icons, /className=\{"icon skin-icon " \+/);
  // Sidebar.tsx が設定の値を --nav-icon で渡す
  const sb = read("src/layout/Sidebar.tsx");
  assert.match(sb, /<nav className="sidebar panel"[^>]*style=\{\{ \["--nav-icon" as any\]: `\$\{sidebarIconPx\(ui\)\}px` \}\}/);
});

test("曲一覧（TrackTable）：コンパクトのときはジャケットの列と見出しの空き枠を出さず、1行にする。行の高さを仮想スクロールに渡し、変わったら測り直す", () => {
  const tt = read("src/components/TrackTable.tsx");
  assert.match(tt, /const compact = compactListOn\(ui, size\.width\);/);
  assert.match(tt, /const rowH = trackRowHeight\(compact, size\.rowH\);/);
  assert.match(tt, /compact \? null : `\$\{size\.artSize\}px`,/);
  assert.match(tt, /\{compact \? null : <span \/>\}/);
  assert.match(tt, /\{p\.compact \? null : <Artwork /);
  // 1行：「タイトル — アーティスト」
  assert.match(tt, /<span className="t-line-title">\{t\.title\}<\/span>/);
  assert.match(tt, /<span className="t-line-artist"> — \{t\.artist \|\| "不明なアーティスト"\}<\/span>/);
  // 仮想スクロール：estimateSize は行の高さ（コンパクトを含む）。変わったら measure で作り直す（estimateSize を替えただけでは並びが変わらない）
  assert.match(tt, /estimateSize: \(\) => rowH,/);
  assert.ok(!/estimateSize: \(\) => size\.rowH/.test(tt));
  assert.match(tt, /virt\.measure\(\);\s*\}, \[rowH\]\);/);
  // 行の見た目の高さ（CSS の --row-h）と、仮想スクロールの高さが同じ値
  assert.match(tt, /\["--row-h" as any\]: `\$\{rowH\}px`/);
  assert.ok(!/\$\{size\.rowH\}px/.test(tt));
  // 再生中の印・お気に入り・ごみ箱の列は、コンパクトでも同じ部品のまま（列の出し方は compact を見ない）
  const eqs = tt.match(/<IconBars className="eq"/g) ?? [];
  assert.equal(eqs.length, 2, "再生中の印は2つの形のどちらにも");
  const colsBlock = tt.slice(tt.indexOf("const cols = {"), tt.indexOf("};", tt.indexOf("const cols = {")));
  assert.ok(!/compact/.test(colsBlock), "列の出し方はコンパクトにかかわらず同じ");
  // CSS：1行は省略記号で切る
  const css = read("src/theme/base.css");
  const line = css.match(/\.t-line \{([^}]*)\}/);
  assert.ok(line);
  assert.match(line![1], /text-overflow: ellipsis;/);
  assert.match(line![1], /white-space: nowrap;/);
  assert.match(line![1], /min-width: 0;/);
});

test("仮想スクロール（@tanstack/virtual-core を実際に動かす）：行の高さを替えただけでは並びが変わらず、measure で新しい高さに追いつく", () => {
  // TrackTable と同じ使い方：estimateSize は描くたびに今の行の高さを返し、行の高さが変わったら measure() を呼ぶ
  let rowH = trackRowHeight(false, 64);
  const opts: any = {
    count: 100,
    getScrollElement: () => ({}),
    estimateSize: () => rowH,
    scrollToFn: () => {},
    observeElementRect: (_: unknown, cb: (r: { width: number; height: number }) => void) => {
      cb({ width: 800, height: 400 });
      return () => {};
    },
    observeElementOffset: (_: unknown, cb: (o: number, scrolling: boolean) => void) => {
      cb(0, false);
      return () => {};
    },
    onChange: () => {},
  };
  const v = new Virtualizer(opts);
  (v as any)._willUpdate();
  assert.equal(v.getTotalSize(), 100 * 64);
  assert.equal(v.getVirtualItems()[1].start, 64);
  const shown64 = v.getVirtualItems().length;
  // コンパクトへ：estimateSize を替えて（描き直しで setOptions）も、覚えた並びのまま
  rowH = trackRowHeight(true, 64);
  v.setOptions({ ...opts, estimateSize: () => rowH });
  assert.equal(v.getTotalSize(), 100 * 64, "measure の前は前の高さ（だから TrackTable は measure を呼ぶ）");
  v.measure();
  assert.equal(v.getTotalSize(), 100 * COMPACT_ROW_H);
  assert.deepEqual(v.getVirtualItems().slice(0, 3).map((x) => [x.index, x.start, x.size]), [[0, 0, 36], [1, 36, 36], [2, 72, 36]]);
  assert.ok(v.getVirtualItems().length > shown64, "同じ高さの窓に、より多くの行を描く");
  // 戻す
  rowH = trackRowHeight(false, 48);
  v.setOptions({ ...opts, estimateSize: () => rowH });
  v.measure();
  assert.equal(v.getTotalSize(), 100 * 48);
  assert.equal(v.getVirtualItems()[5].start, 5 * 48);
});

test("TrackTable を使う一覧（ライブラリ・プレイリスト・キュー）は、どれも同じ部品（コンパクト表示が全部に効く）", () => {
  for (const f of ["LibraryView", "PlaylistView", "QueueView"]) {
    assert.match(read(`src/views/${f}.tsx`), /<TrackTable\b/, f);
  }
  // 再生バーのジャケットはコンパクト表示の対象外（PlayerBar は設定の compactList を見ない）
  assert.ok(!/compact/i.test(read("src/layout/PlayerBar.tsx")));
});

test("並べ替え（プレイリスト・キュー）：落とした先は行の番号で決める（行の高さを使わない）", () => {
  const shell = read("src/layout/AppShell.tsx");
  assert.match(shell, /api\.queueMove\(d\.index, over\.index\)/);
  assert.match(shell, /api\.playlistMoveItem\(Number\(r\.key\), itemId, over\.index\)/);
  assert.ok(!/rowH|row-h|compact/.test(shell));
  const tt = read("src/components/TrackTable.tsx");
  // 並べ替えの行は、描いた位置（仮想スクロールの start）にドラッグ中のずれを足すだけ
  assert.match(tt, /transform: `translate3d\(0, \$\{p\.start \+ props\.ty\}px, 0\)`/);
  assert.match(tt, /index: p\.index,/);
});

test("設定の画面：表示のタブに2つの欄。コンパクト表示は3つから選び、アイコンはスライダー（範囲は density.ts の値）", () => {
  const v = read("src/views/SettingsView.tsx");
  const display = v.slice(v.indexOf('{tab === "display" ? ('), v.indexOf('{tab === "skin"'));
  assert.ok(display.indexOf("<CompactField />") > display.indexOf("曲一覧の列"), "曲一覧の列のすぐ下");
  assert.ok(display.indexOf("<SidebarIconField />") > display.indexOf("<HideFields />"), "サイドバーの項目のすぐ下");
  assert.match(v, /COMPACT_LIST_MODES\.map\(\(m\) =>/);
  assert.match(v, /min=\{SIDEBAR_ICON_PX\.min\}/);
  assert.match(v, /max=\{SIDEBAR_ICON_PX\.max\}/);
  assert.match(v, /<span className="flabel">曲一覧のコンパクト表示<\/span>/);
  assert.match(v, /<span className="flabel">サイドバーのアイコンの大きさ<\/span>/);
});
