// v3.4.0：表示の詰め方（曲一覧のコンパクト表示・サイドバーのアイコンの大きさ）を確かめる。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/ui-density.test.mts
// 仮想スクロールの部品（@tanstack/virtual-core）を実際に動かすテストが1つあるので、先に npm ci を済ませる。
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Virtualizer } from "@tanstack/virtual-core";
import {
  COMPACT_AUTO_BELOW,
  COMPACT_LIST_MODES,
  COMPACT_ROW_H,
  compactAutoOn,
  compactListMode,
  compactListOn,
  LIST_BELOW,
  listLayoutFor,
  SIDEBAR_ICON_PX,
  sidebarIconPx,
  trackRowHeight,
} from "../src/layout/density.ts";
import { SORT_STAND_IN, SORT_STAND_IN_HEADS, sortHead, sortStandInHint } from "../src/layout/sortHead.ts";

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");

test("コンパクト表示：古い設定（値が無い）・知らない値は「使わない」", () => {
  for (const ui of [undefined, null, {}, { compactList: undefined }, { compactList: null }, { compactList: "" }, { compactList: "big" }, { compactList: "Always" }, { compactList: 1 }, { compactList: true }, { compactList: ["always"] }]) {
    assert.equal(compactListMode(ui as any), "off", JSON.stringify(ui));
    for (const w of [300, 719, 720, 1400]) assert.equal(compactListOn(ui as any, w), false, `${JSON.stringify(ui)} ${w}`);
  }
  // v3.8.0：「使わない／いつも」の2つ（3.7.0 の「ウィンドウが狭いときだけ」は、チェック「窓が狭いときは自動でコンパクトにする」へ分けた）
  assert.deepEqual(
    COMPACT_LIST_MODES.map((m) => m.label),
    ["使わない", "いつも"],
  );
  assert.equal(COMPACT_LIST_MODES[0].id, "off", "既定（使わない）を先頭に出す");
  for (const ui of [undefined, null, {}, { compactAuto: null }, { compactAuto: "true" }, { compactAuto: 1 }]) assert.equal(compactAutoOn(ui as any), false, JSON.stringify(ui));
});

test("コンパクト表示（v3.8.0）：いつも・自動（曲一覧の側の幅が 626px 未満）。3.7.0 の \"narrow\" は 使わない＋自動 と読む", () => {
  for (const w of [320, 625, 626, 1100, 2560]) assert.equal(compactListOn({ compactList: "always" }, w), true, String(w));
  for (const w of [320, 625, 626, 1100, 2560]) assert.equal(compactListOn({ compactList: "always", compactAuto: true }, w), true, String(w));
  assert.equal(COMPACT_AUTO_BELOW, 626);
  for (const ui of [{ compactList: "off", compactAuto: true }, { compactList: "narrow" }, { compactList: "narrow", compactAuto: false }]) {
    assert.equal(compactListMode(ui), "off", JSON.stringify(ui));
    assert.equal(compactAutoOn(ui), true, JSON.stringify(ui));
    assert.equal(compactListOn(ui, 625), true);
    assert.equal(compactListOn(ui, 625.5), true);
    assert.equal(compactListOn(ui, 626), false);
    assert.equal(compactListOn(ui, 1400), false);
  }
  assert.equal(compactListOn({ compactList: "off" }, 300), false);
  assert.equal(compactListOn({ compactList: "off", compactAuto: false }, 300), false);
});

test("自動のコンパクトの境目は、ジャケットが縮む段（曲一覧の側の幅 626px 未満）と同じ。どちらも曲一覧の側の幅で決め、窓の幅の @media は残さない", () => {
  assert.equal(COMPACT_AUTO_BELOW, LIST_BELOW.art);
  assert.equal(listLayoutFor(LIST_BELOW.art - 0.5).artSize, 36);
  assert.equal(listLayoutFor(LIST_BELOW.art).artSize, 44);
  // 上の帯のファイル・フォルダの文字も同じ段で隠す（3.7.0 は窓の幅 720px 未満の .hide-narrow）
  assert.equal(listLayoutFor(LIST_BELOW.art - 1).topbarNarrow, true);
  assert.equal(listLayoutFor(LIST_BELOW.art).topbarNarrow, false);
  const css = read("src/theme/base.css");
  assert.match(css, /\n\.topbar\.narrow \.hide-narrow \{\s*display: none;/);
  assert.match(read("src/layout/TopBar.tsx"), /<header className=\{"topbar" \+ \(narrow \? " narrow" : ""\)\}>/);
  // 曲一覧の列・ジャケット・行の高さ・ファイルの文字を、窓の幅の @media で変えていない
  for (const m of css.matchAll(/@media \(max-width: (\d+)px\) \{([\s\S]*?)\n\}\n/g)) {
    for (const bad of ["--row-h", "--row-art", "--player-h", ".col-genre", ".col-album", ".hide-narrow", ".track-row", ".player"]) {
      assert.ok(!m[2].includes(bad), `@media ${m[1]}px: ${bad}`);
    }
  }
});

test("行の高さ：コンパクトなら 36px（幅の段にかかわらず）、そうでなければ通常の高さ。行の中のボタンが収まる", () => {
  assert.equal(COMPACT_ROW_H, 36);
  for (const normal of [60, 48]) {
    assert.equal(trackRowHeight(true, normal), COMPACT_ROW_H);
    assert.equal(trackRowHeight(false, normal), normal);
  }
  // 行の中で一番高い部品：お気に入り・ごみ箱のボタン（v3.7.0：通常 32px・コンパクト 28px の面）。上下に 4px 以上残す
  const css = read("src/theme/base.css");
  const normalBtn = css.match(/\.t-fav \.icon-btn,\s*\.t-trash \.icon-btn \{\s*width: (\d+)px;\s*height: (\d+)px;/);
  assert.ok(normalBtn, "row buttons");
  const compactBtn = css.match(/\.track-table\.compact \.t-fav \.icon-btn,\s*\.track-table\.compact \.t-trash \.icon-btn \{\s*width: (\d+)px;\s*height: (\d+)px;/);
  assert.ok(compactBtn, "compact row buttons");
  assert.equal(Number(normalBtn![2]), 32);
  assert.ok(COMPACT_ROW_H >= Number(compactBtn![2]) + 8, `${COMPACT_ROW_H} >= ${compactBtn![2]} + 8`);
  // 通常の行（いちばん低い 600px 未満の 48px でも）に 32px の面が収まる
  for (const w of [300, 517, 518, 1400]) assert.ok(listLayoutFor(w).rowH >= Number(normalBtn![2]) + 8, String(w));
  // 通常の行（48・60px）より低い
  assert.ok(COMPACT_ROW_H < 48);
});

test("曲一覧の行の高さ・ジャケットの大きさ：既定は CSS（tokens.css）と JS（density.ts の listLayoutFor）で同じ。狭い段の値は JS だけが持ち、曲一覧の要素に入れて渡す", () => {
  const tokens = read("src/theme/tokens.css");
  const css = read("src/theme/base.css");
  const px = (src: string, name: string) => {
    const m = src.match(new RegExp(`${name}: (\\d+)px;`));
    assert.ok(m, name);
    return Number(m![1]);
  };
  // 既定（広い）：60px・44px（3.6.0 までは 64・48）
  assert.equal(px(tokens, "--row-h"), 60);
  assert.equal(px(tokens, "--row-art"), 44);
  assert.equal(px(tokens, "--player-h"), 80);
  assert.equal(px(tokens, "--radius-card"), 12);
  for (const w of [1826, 1306, 1005, 806, 726, 626]) {
    assert.equal(listLayoutFor(w).rowH, px(tokens, "--row-h"), `rowH ${w}`);
    assert.equal(listLayoutFor(w).artSize, px(tokens, "--row-art"), `artSize ${w}`);
  }
  // 曲一覧の側が 626px 未満：ジャケット 36px（行は 518px 以上なら既定のまま）。518px 未満：行 48px（v3.7.0 の値）
  for (const w of [625, 518]) {
    assert.equal(listLayoutFor(w).artSize, 36, `artSize ${w}`);
    assert.equal(listLayoutFor(w).rowH, 60, `rowH ${w}`);
    assert.equal(listLayoutFor(w).tight, false, `tight ${w}`);
  }
  for (const w of [517, 398, 300]) {
    assert.equal(listLayoutFor(w).rowH, 48, `rowH ${w}`);
    assert.equal(listLayoutFor(w).artSize, 36, `artSize ${w}`);
    assert.equal(listLayoutFor(w).tight, true, `tight ${w}`);
  }
  // 曲一覧の行は JS の行の高さを --row-h、ジャケットの大きさを --row-art として受け取る（仮想スクロールの1行と同じ）。ジャケットの列の幅は artSize
  const tt = read("src/components/TrackTable.tsx");
  assert.match(tt, /\["--row-h" as any\]: `\$\{rowH\}px`/);
  assert.match(tt, /\["--row-art" as any\]: `\$\{size\.artSize\}px`/);
  assert.match(tt, /compact \? null : `\$\{size\.artSize\}px`,/);
  assert.match(tt, /const size = useListLayout\(\);/);
  assert.match(tt, /\(size\.tight \? " tight" : ""\)/);
  assert.match(css, /\.t-art \{\s*width: var\(--row-art\);\s*height: var\(--row-art\);/);
  assert.match(css, /\.track-row \{\s*height: var\(--row-h\);/);
  assert.match(css, /\.track-table\.tight \.track-head,\n\.track-table\.tight \.track-row \{\s*padding: 0 10px;\s*column-gap: 10px;/);
  // スキンからは変えさせない（Rust と画面の LOCKED_VARS）
  const rs = read("src-tauri/src/skins/tokens.rs");
  for (const v of ["--row-h", "--row-art", "--player-h"]) assert.ok(rs.includes(`"${v}"`) && read("src/skins/tokens.ts").includes(`"${v}"`), v);
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
  // v3.8.0：3.7.0 までの「狭いときだけ」の名前（読むときだけ受ける）も Rust と同じ
  assert.match(rs, /pub const COMPACT_LIST_LEGACY_NARROW: &str = "narrow";/);
  // 画面の既定値（src\stores\settings.ts）も同じ
  const st = read("src/stores/settings.ts");
  assert.match(st, /compactList: "off",/);
  assert.match(st, /compactAuto: false,/);
  assert.match(rs, /compact_auto: false,/);
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
  // v3.5.0（作者の要望）：タイトルとアーティストは別の列。タイトルの列は1行、アーティストはアルバム・ジャンルと同じ形の列
  assert.match(tt, /<span className="t-line">\{t\.title\}<\/span>/);
  assert.ok(!/t-line-artist| — \{t\.artist/.test(tt), "no more 「タイトル — アーティスト」");
  // アーティストの列は、コンパクト表示で設定がオンのときだけ（cols.artist。下の「アーティストの出し入れ」）
  assert.match(tt, /\{p\.cols\.artist \? <div className="t-cell col-artist">\{t\.artist \|\| "不明なアーティスト"\}<\/div> : null\}/);
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
  // （v3.5.0：アーティストだけは、コンパクトかどうかで列か段かが変わる。それ以外の列はコンパクトにかかわらず同じ）
  const colsBlock = tt.slice(tt.indexOf("const cols = {"), tt.indexOf("};", tt.indexOf("const cols = {")));
  for (const k of ["rank", "album", "genre", "plays", "duration", "favorite", "trash"]) {
    const l = colsBlock.split(/\r?\n/).find((x) => x.trim().startsWith(`${k}:`));
    assert.ok(l, k);
    assert.ok(!/compact/.test(l!), `列の出し方はコンパクトにかかわらず同じ：${k}`);
  }
  // CSS：1行は省略記号で切る
  const css = read("src/theme/base.css");
  const line = css.match(/\.t-line \{([^}]*)\}/);
  assert.ok(line);
  assert.match(line![1], /text-overflow: ellipsis;/);
  assert.match(line![1], /white-space: nowrap;/);
  assert.match(line![1], /min-width: 0;/);
});

test("曲一覧（TrackTable）：列の幅・見出しの行・曲の行は、同じ条件の列を同じ順に並べる（コンパクトのアーティストの列・幅の段で消えるアルバムとジャンル・年間まとめの順位と回数を含む）", () => {
  const tt = read("src/components/TrackTable.tsx");
  // 列の幅（template）：1項目1行。`X ? null : …` は X でないとき、`X ? … : null` は X のとき、条件の無いものはいつも
  const tplBlock = tt.slice(tt.indexOf("const template = ["), tt.indexOf("]", tt.indexOf("const template = [")));
  const tpl = tplBlock
    .split(/\r?\n/)
    .slice(1)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith("//"))
    .map((l) => {
      const m = l.match(/^([\w.]+) \? (null)?/);
      if (!m) return "always";
      return m[2] ? `!${m[1]}` : m[1];
    });
  // 見出しの行・曲の行：いちばん外側の {…}（コメントは除く）。複数行の式は、同じ字下げの「) : null}」か「)}」で閉じる
  const items = (block: string, indent: number) => {
    const lines = block.split(/\r?\n/);
    const out: string[] = [];
    const pad = " ".repeat(indent);
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      if (!l.startsWith(pad + "{") || l.startsWith(pad + "{/*")) continue;
      const m = l.match(/^\s*\{(?:p\.)?([\w.]+) \? (null :)?/);
      assert.ok(m, `unexpected item: ${l}`);
      const cond = m![1].replace(/^p\./, "");
      if (m![2]) {
        out.push(`!${cond}`);
        continue;
      }
      let end = l;
      if (/\($/.test(l.trimEnd())) {
        let j = i + 1;
        while (j < lines.length && !(lines[j].startsWith(pad + ")") && /\}\s*$/.test(lines[j]))) j++;
        assert.ok(j < lines.length, `unclosed item: ${l}`);
        end = lines[j];
        i = j;
      }
      out.push(/: null\}\s*$/.test(end) ? cond : "always");
    }
    return out;
  };
  const headStart = tt.indexOf('<div className="track-head" role="row">');
  const head = items(tt.slice(headStart, tt.indexOf('<div className="track-scroll"', headStart)), 8);
  const rowStart = tt.indexOf("onContextMenu={p.onContextMenu}");
  const row = items(tt.slice(rowStart, tt.indexOf("export function useTrackIdsMemo", rowStart)), 6).map((c) => c.replace(/^(!?)p\./, "$1"));
  // v3.6.0：年間まとめの順位（左端）と期間中の回数（時間の前）
  const expected = ["cols.rank", "!compact", "always", "cols.artist", "cols.album", "cols.genre", "cols.plays", "cols.duration", "cols.favorite", "cols.trash"];
  assert.deepEqual(tpl, expected, "template");
  assert.deepEqual(head, expected, "header");
  assert.deepEqual(row, expected, "row");
  // v3.5.0：アーティストの列は「コンパクト表示 かつ 設定がオン」、タイトルの下の段は「通常の表示 かつ 設定がオン」（cols の作り方）
  assert.match(tt, /const artistOn = cfg\.artist !== false;/);
  assert.match(tt, /\n\s*artist: compact && artistOn,\r?\n/);
  assert.match(tt, /\n\s*artistLine: !compact && artistOn,\r?\n/);
  // v3.6.0：順位と回数の列は、年間まとめ（props.ranking があるとき）だけ。どちらも同じ条件
  assert.match(tt, /\n\s*rank: !!props\.ranking,\r?\n/);
  assert.match(tt, /\n\s*plays: !!props\.ranking,\r?\n/);
  // 幅の段（900・820px 未満でジャンル・アルバムが消える）×コンパクトの有無×アーティストの出し入れ×列の設定×年間まとめかどうかで、
  // 見出しと行の列の数が列の幅の数と同じ（cols.artist は上の作り方どおり compact と設定から決める）
  const count = (seq: string[], v: Record<string, boolean>) =>
    seq.filter((c) => c === "always" || (c.startsWith("!") ? !v[c.slice(1)] : v[c])).length;
  let combos = 0;
  for (const ranking of [false, true])
  for (const compact of [false, true])
    for (const artistOn of [false, true])
      for (const album of [false, true])
        for (const genre of [false, true])
          for (const rest of [false, true]) {
            const v = {
              compact,
              "cols.rank": ranking,
              "cols.plays": ranking,
              "cols.artist": compact && artistOn,
              "cols.album": album,
              "cols.genre": genre,
              "cols.duration": rest,
              "cols.favorite": rest,
              "cols.trash": !rest,
            };
            const label = JSON.stringify({ ...v, artistOn, ranking });
            assert.equal(count(head, v), count(tpl, v), label);
            assert.equal(count(row, v), count(tpl, v), label);
            // 列の数：ジャケット（通常のとき）＋タイトル＋アーティスト（コンパクトで出すとき）＋…。アーティストを切ったコンパクトは1つ減る
            // 年間まとめなら、順位と回数の2つが増える
            const want = (ranking ? 2 : 0) + (compact ? 0 : 1) + 1 + (compact && artistOn ? 1 : 0) + (album ? 1 : 0) + (genre ? 1 : 0) + (rest ? 2 : 1);
            assert.equal(count(tpl, v), want, label);
            combos++;
          }
  assert.equal(combos, 64);
  // アルバム・ジャンルは幅の段で消える（JS の cols と、CSS の .col-album・.col-genre の両方）。アーティストの列はどの幅でも消さない
  assert.match(tt, /album: cfg\.album !== false && !size\.hideAlbum,/);
  assert.match(tt, /genre: cfg\.genre !== false && !size\.hideGenre,/);
  assert.ok(!/col-artist[^}]*display: none/.test(read("src/theme/base.css")));
});

test("コンパクト表示の見出し：「タイトル」と「アーティスト」。どちらも並べ替えのボタン（headBtn）で、アーティストの鍵は Rust の並べ替えにある。通常の表示は「タイトル / アーティスト」のまま（アーティストを切ったときは「タイトル」）", () => {
  const tt = read("src/components/TrackTable.tsx");
  assert.match(tt, /\{cols\.artistLine \? headBtn\("title", "タイトル \/ アーティスト"\) : headBtn\("title", "タイトル"\)\}/);
  assert.match(tt, /\{cols\.artist \? headBtn\("artist", "アーティスト", "col-artist"\) : null\}/);
  // アーティストの列の幅は、アルバムの列と同じ 2fr。タイトルは 3fr
  assert.match(tt, /\n\s*"minmax\(0, 3fr\)",\r?\n/);
  assert.match(tt, /cols\.artist \? "minmax\(0, 2fr\)" : null,/);
  assert.match(tt, /cols\.album \? "minmax\(0, 2fr\)" : null,/);
  // 並べ替えの鍵 "artist" を Rust（db\tracks.rs の order_clause）が知っている
  const rs = read("src-tauri/src/db/tracks.rs");
  const oc = rs.slice(rs.indexOf("fn order_clause"), rs.indexOf("\n}\n", rs.indexOf("fn order_clause")));
  assert.match(oc, /Some\("artist"\) => format!\("t\.artist COLLATE NOCASE \{dir\}/);
  // 見出しは狭い窓で隣の列へはみ出さない（コンパクトのときだけ。通常の見出しの決まりは変えない）
  const css = read("src/theme/base.css");
  const hb = css.match(/\.track-table\.compact \.track-head button \{([^}]*)\}/);
  assert.ok(hb);
  for (const d of ["min-width: 0;", "max-width: 100%;", "overflow: hidden;", "text-overflow: ellipsis;"]) assert.ok(hb![1].includes(d), d);
});

test("並べ替えの矢印（v3.8.0）：アーティストで並べ替えたまま、アーティストの見出しが無いときは、並びを変えずにタイトルの見出しへ矢印と説明を出す。年間まとめは出さない", () => {
  // 並べていない・壊れた状態：どこにも出さない
  for (const s of [undefined, null, { by: null, desc: false }, { by: "", desc: true }, { by: 3, desc: false }]) {
    for (const artistColumn of [false, true]) {
      assert.deepEqual(sortHead(s as any, { artistColumn }), { at: null, desc: false, hint: null }, JSON.stringify({ s, artistColumn }));
    }
  }
  // アーティストの見出しがある（コンパクト表示で列を出している）：今までどおりアーティストの見出しに。説明は無い
  assert.deepEqual(sortHead({ by: "artist", desc: false }, { artistColumn: true }), { at: "artist", desc: false, hint: null });
  assert.deepEqual(sortHead({ by: "artist", desc: true }, { artistColumn: true }), { at: "artist", desc: true, hint: null });
  // アーティストの見出しが無い（通常の表示に戻った・コンパクトで列を切った）：タイトルの見出しに、同じ向きの矢印と説明
  const up = sortHead({ by: "artist", desc: false }, { artistColumn: false });
  const down = sortHead({ by: "artist", desc: true }, { artistColumn: false });
  assert.deepEqual(up, { at: "title", desc: false, hint: sortStandInHint(false) });
  assert.deepEqual(down, { at: "title", desc: true, hint: sortStandInHint(true) });
  for (const h of [up.hint!, down.hint!]) {
    assert.match(h, /^アーティストで並べ替え中/);
    assert.match(h, /押すとタイトルで並べ替えます。$/);
  }
  assert.ok(!up.hint!.includes("逆順"));
  assert.ok(down.hint!.includes("（逆順）"));
  // ほかの鍵は、アーティストの見出しの有無にかかわらずその見出しに（説明は無い）。並べ替えの状態そのものは変えない
  for (const by of ["title", "album", "genre", "duration"]) {
    for (const artistColumn of [false, true]) {
      for (const desc of [false, true]) {
        const st = { by, desc };
        assert.deepEqual(sortHead(st, { artistColumn }), { at: by, desc, hint: null }, JSON.stringify({ by, artistColumn, desc }));
        assert.deepEqual(st, { by, desc });
      }
    }
  }
  // 年間まとめ（fixedOrder）：どの鍵・どの形でも出さない
  for (const by of ["artist", "title", "album"]) {
    for (const artistColumn of [false, true]) {
      assert.deepEqual(sortHead({ by, desc: true }, { fixedOrder: true, artistColumn }), { at: null, desc: false, hint: null }, by);
    }
  }
  // 代わりに出す見出しはタイトルだけ
  assert.deepEqual({ ...SORT_STAND_IN }, { artist: "title" });
  assert.deepEqual([...SORT_STAND_IN_HEADS], ["title"]);

  // 画面（TrackTable）：sortHead の結果で矢印と色（sorted）を出し、アーティストの見出しがあるかは cols.artist（コンパクト かつ 設定がオン）で渡す
  const tt = read("src/components/TrackTable.tsx");
  assert.match(tt, /const mark = sortHead\(sortState, \{ fixedOrder: !!props\.fixedOrder, artistColumn: cols\.artist \}\);/);
  assert.match(tt, /const on = mark\.at === key;/);
  assert.match(tt, /className=\{cls \+ \(on \? " sorted" : ""\)\}/);
  assert.match(tt, /\{on \? \(mark\.desc \? " ↓" : " ↑"\) : ""\}/);
  assert.ok(!/sortedBy/.test(tt), "the old sortedBy is gone");
  // 押したときの動きは今までどおり（押した見出しの鍵で並べ替える。代わりの見出しを押せばタイトルの並べ替え）
  assert.match(tt, /sort\.set\(\(s\) => \(s\.by === key \? \(s\.desc \? \{ by: null, desc: false \} : \{ by: key, desc: true \}\) : \{ by: key, desc: false \}\)\)/);
  // 説明は自前の吹き出し（Tip。読み上げは aria-describedby で同じ文）。包むのは代わりの見出しだけで、文の有無で包み方を変えない
  assert.match(tt, /return SORT_STAND_IN_HEADS\.includes\(key\) \? <Tip text=\{on \? mark\.hint : null\} place="below">\{btn\}<\/Tip> : btn;/);
  // 見出しの並びは今までどおり（タイトルの見出しはいつも headBtn("title", …)）
  assert.match(tt, /\{cols\.artistLine \? headBtn\("title", "タイトル \/ アーティスト"\) : headBtn\("title", "タイトル"\)\}/);
});

test("アーティストの出し入れ（v3.5.0。作者の要望）：設定の「曲一覧の列」のアルバムの前。名前と既定（オン）が Rust と画面で同じ。通常の表示ではタイトルの下の段を出し入れする", () => {
  // 設定の画面：チェックの並びは列の順（アーティスト → アルバム → ジャンル → 時間 → お気に入り）
  const v = read("src/views/SettingsView.tsx");
  const colsDef = v.slice(v.indexOf("const COLS = ["), v.indexOf("];", v.indexOf("const COLS = [")));
  const ids = [...colsDef.matchAll(/\{ id: "(\w+)", label: "([^"]+)" \}/g)].map((x) => [x[1], x[2]]);
  assert.deepEqual(ids, [["artist", "アーティスト"], ["album", "アルバム"], ["genre", "ジャンル"], ["duration", "時間"], ["favorite", "お気に入り"]]);
  // チェックは「無いキーはオン」で読む（古い設定）
  assert.match(v, /checked=\{s\.ui\.columns\[c\.id\] !== false\}/);
  // 説明：アーティストはコンパクトでは列・通常では段。狭い窓で自動で隠れるのはジャンルとアルバムだけ
  const field = v.slice(v.indexOf('<span className="flabel">曲一覧の列</span>'), v.indexOf("<CompactField />"));
  assert.match(field, /アーティストは、コンパクト表示では列を、通常の表示ではタイトルの下の段を出し入れします。/);
  assert.match(field, /ジャンル → アルバムの順に自動で隠れます（アーティストは隠れません）/);
  // Rust（settings\store.rs の TRACK_COLUMNS）と同じ名前・同じ並び
  const rs = read("src-tauri/src/settings/store.rs");
  const tc = rs.match(/pub const TRACK_COLUMNS: &\[&str\] = &\[([^\]]*)\];/);
  assert.ok(tc, "TRACK_COLUMNS");
  assert.deepEqual([...tc![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]), ids.map((x) => x[0]));
  assert.match(rs, /columns: all_shown\(TRACK_COLUMNS\),/);
  // 画面の既定値もオン（全部）
  assert.match(read("src/stores/settings.ts"), /columns: \{ artist: true, album: true, genre: true, duration: true, favorite: true \},/);
  // 通常の表示：アーティストの段は設定がオンのときだけ。行の高さ（rowH）は設定を見ない
  const tt = read("src/components/TrackTable.tsx");
  assert.match(tt, /\{p\.cols\.artistLine \? <div className="t-artist">\{t\.artist \|\| "不明なアーティスト"\}<\/div> : null\}/);
  assert.equal((tt.match(/className="t-artist"/g) ?? []).length, 1, "アーティストの段は1か所だけ");
  assert.ok(!/artist/i.test(read("src/layout/density.ts")), "行の高さはアーティストの設定で変わらない");
  // アーティストを切ったときの、タイトルだけの行：行の中で縦の真ん中に来る（grid の align-items: center）
  const css = read("src/theme/base.css");
  assert.match(css, /\.track-head,\s*\.track-row \{[^}]*align-items: center;/);
});

test("仮想スクロール（@tanstack/virtual-core を実際に動かす）：行の高さを替えただけでは並びが変わらず、measure で新しい高さに追いつく", () => {
  // TrackTable と同じ使い方：estimateSize は描くたびに今の行の高さを返し、行の高さが変わったら measure() を呼ぶ
  const normal = listLayoutFor(1400).rowH; // v3.7.0：60px
  let rowH = trackRowHeight(false, normal);
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
  assert.equal(v.getTotalSize(), 100 * normal);
  assert.equal(v.getVirtualItems()[1].start, normal);
  const shownNormal = v.getVirtualItems().length;
  // コンパクトへ：estimateSize を替えて（描き直しで setOptions）も、覚えた並びのまま
  rowH = trackRowHeight(true, normal);
  v.setOptions({ ...opts, estimateSize: () => rowH });
  assert.equal(v.getTotalSize(), 100 * normal, "measure の前は前の高さ（だから TrackTable は measure を呼ぶ）");
  v.measure();
  assert.equal(v.getTotalSize(), 100 * COMPACT_ROW_H);
  assert.deepEqual(v.getVirtualItems().slice(0, 3).map((x) => [x.index, x.start, x.size]), [[0, 0, 36], [1, 36, 36], [2, 72, 36]]);
  assert.ok(v.getVirtualItems().length > shownNormal, "同じ高さの窓に、より多くの行を描く");
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

test("設定の画面：表示のタブに2つの欄。コンパクト表示は2つから選び（v3.8.0）、自動のチェックを添える。アイコンはスライダー（範囲は density.ts の値）", () => {
  const v = read("src/views/SettingsView.tsx");
  const display = v.slice(v.indexOf('{tab === "display" ? ('), v.indexOf('{tab === "skin"'));
  assert.ok(display.indexOf("<CompactField />") > display.indexOf("曲一覧の列"), "曲一覧の列のすぐ下");
  assert.ok(display.indexOf("<SidebarIconField />") > display.indexOf("<HideFields />"), "サイドバーの項目のすぐ下");
  assert.match(v, /COMPACT_LIST_MODES\.map\(\(m\) =>/);
  assert.match(v, /min=\{SIDEBAR_ICON_PX\.min\}/);
  assert.match(v, /max=\{SIDEBAR_ICON_PX\.max\}/);
  assert.match(v, /<span className="flabel">曲一覧のコンパクト表示<\/span>/);
  assert.match(v, /<span className="flabel">サイドバーのアイコンの大きさ<\/span>/);
  // v3.8.0：チェック「窓が狭いときは自動でコンパクトにする」は compactAuto を書く。どちらを変えても、もう一方は今の値（3.7.0 の "narrow" も読み替えた値）で書く
  const field = v.slice(v.indexOf("function CompactField()"), v.indexOf("function SidebarIconField()"));
  assert.match(field, /<span>窓が狭いときは自動でコンパクトにする<\/span>/);
  assert.match(field, /checked=\{auto\}/);
  assert.match(field, /compactList: m\.id, compactAuto: compactAutoOn\(x\.ui\)/);
  assert.match(field, /compactList: compactListMode\(x\.ui\), compactAuto: e\.target\.checked/);
  assert.ok(!/ウィンドウが狭いときだけ/.test(field.slice(field.indexOf("return ("))), "画面に出す文に古い3つ目の選び方が残っていない");
});
