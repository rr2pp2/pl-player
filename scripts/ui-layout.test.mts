// v3.8.0：画面の土台（同時再生と配信モード）を確かめる。
//   - 曲一覧の列・ジャケット・行の高さ・自動のコンパクトを「曲一覧の側の幅」で決めても、3.7.0 の設定から上げた見た目が今と同じ
//   - 上の帯のモード切替（通常／コンパクト）の押し方と、文字を出す幅
//   - 再生バーの3列と1段の切り替え（中身が入るかどうか）と、ボタンの置き場所
//   - スキンから変えさせない変数（Rust と画面の LOCKED_VARS）・最小の窓 480×480 で、はみ出さない幅の計算
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/ui-layout.test.mts
// 例の名前は架空。実在の曲名・アーティスト名・ユーザー名は入れない。
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  COMPACT_AUTO_BELOW,
  compactAutoOn,
  compactListMode,
  compactListOn,
  currentViewMode,
  LIST_BELOW,
  listLayoutFor,
  pressViewMode,
  TOPBAR,
  VIEW_MODES,
  viewModeLabelsFit,
} from "../src/layout/density.ts";
import {
  CENTER_FULL_W,
  PLAYER,
  playerCenterWidth,
  playerLayout,
  playerLeftMin,
  playerRightMin,
  type MovableButton,
} from "../src/layout/playerLayout.ts";

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");
const css = read("src/theme/base.css");
const tokens = read("src/theme/tokens.css");
const rule = (sel: string) => {
  const at = css.indexOf("\n" + sel + " {");
  assert.ok(at >= 0, sel);
  return css.slice(at, css.indexOf("}", at));
};
const px = (src: string, name: string) => {
  const m = src.match(new RegExp(`${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}: (\\d+)px;`));
  assert.ok(m, name);
  return Number(m![1]);
};

// ---------- 窓の幅と、曲一覧の側の幅（.main）の関係（base.css の .app と tokens.css から読む） ----------
const appPad = Number(rule(".app").match(/padding: (\d+)px;/)![1]);
const appGap = Number(rule(".app").match(/gap: (\d+)px;/)![1]);
const narrowAt = css.indexOf("@media (max-width: 599px) {");
const narrowBlock = css.slice(narrowAt, css.indexOf("\n}\n", narrowAt));
const appPadNarrow = Number(narrowBlock.match(/\.app \{\s*gap: (\d+)px;\s*padding: (\d+)px;/)![2]);
const appGapNarrow = Number(narrowBlock.match(/\.app \{\s*gap: (\d+)px;/)![1]);
const sidebarW = px(tokens, "--sidebar-w");
const sidebarCollapsedW = px(tokens, "--sidebar-w-collapsed");

/** 窓の幅 W で、サイドバーの状態が collapsed のときの .main の幅（枠を含む） */
function mainWidth(W: number, collapsed: boolean): number {
  const pad = W < 600 ? appPadNarrow : appPad;
  const gap = W < 600 ? appGapNarrow : appGap;
  const side = collapsed || W < 1100 ? sidebarCollapsedW : sidebarW;
  return W - pad * 2 - side - gap;
}
/** 窓の幅 W のときの再生バーの幅（左右いっぱい） */
const barWidth = (W: number) => W - (W < 600 ? appPadNarrow : appPad) * 2;

/** 3.7.0 の決まり（窓の幅で決めていた。src\components\useLayoutSize.ts・density.ts・base.css の @media を写したもの） */
const v370 = {
  hideGenre: (W: number) => W < 900,
  hideAlbum: (W: number) => W < 820,
  artSize: (W: number) => (W < 720 ? 36 : 44),
  rowH: (W: number) => (W < 600 ? 48 : 60),
  tight: (W: number) => W < 600,
  topbarNarrow: (W: number) => W < 720,
  compact: (mode: string, W: number) => mode === "always" || (mode === "narrow" && W < 720),
};
/** Rust の sanitize と同じ読み替え（settings\store.rs。3.7.0 の値 → 今の形） */
const migrate = (mode: string) => ({ compactList: mode === "always" ? "always" : "off", compactAuto: mode === "narrow" });

test("窓の幅と曲一覧の側の幅：base.css と tokens.css の値（外の余白 10px・600px 未満で 6px、サイドバー 236px・たたんで 64px）", () => {
  assert.deepEqual([appPad, appGap, appPadNarrow, appGapNarrow, sidebarW, sidebarCollapsedW], [10, 10, 6, 6, 236, 64]);
  // サイドバーが自動でたたまれる 1100px 未満では、曲一覧の側の幅 ＝ 窓の幅 − 94（600px 以上）
  for (const W of [600, 720, 820, 900, 1099]) assert.equal(mainWidth(W, true), W - 94, String(W));
  // 境目の値は、3.7.0 の窓の幅の境目からこの式で出したもの
  assert.deepEqual(LIST_BELOW, { genre: 900 - 94, album: 820 - 94, art: 720 - 94, row: 518 });
  assert.equal(COMPACT_AUTO_BELOW, 720 - 94);
});

test("3.7.0 の設定（compactList が off・narrow・always）から上げても、窓の幅 480〜2560px のどこでも曲一覧の見た目が今と同じ（行の高さだけ 600〜611px の 12px の幅で 48px になる）", () => {
  const diffs: string[] = [];
  let checked = 0;
  for (let W = 480; W <= 2560; W++) {
    for (const userCollapsed of [false, true]) {
      if (W < 1100 && userCollapsed) continue; // 1100px 未満はどちらでも自動でたたむ
      const L = listLayoutFor(mainWidth(W, userCollapsed));
      const key = `${W}px${userCollapsed ? "（たたんだ）" : ""}`;
      for (const [k, old] of [
        ["hideGenre", v370.hideGenre(W)],
        ["hideAlbum", v370.hideAlbum(W)],
        ["artSize", v370.artSize(W)],
        ["topbarNarrow", v370.topbarNarrow(W)],
      ] as const) {
        assert.equal((L as any)[k], old, `${key} ${k}`);
      }
      if (L.rowH !== v370.rowH(W) || L.tight !== v370.tight(W)) diffs.push(key);
      for (const mode of ["off", "narrow", "always"]) {
        const want = v370.compact(mode, W);
        // 画面の読み方（3.7.0 の値のままでも）と、Rust が読み替えて返す形のどちらでも同じ
        assert.equal(compactListOn({ compactList: mode }, L.width), want, `${key} ${mode}`);
        assert.equal(compactListOn(migrate(mode), L.width), want, `${key} ${mode} migrated`);
        // 上の帯の切替も、そのときの見た目どおりを選んで見せる
        assert.equal(currentViewMode(migrate(mode), L.width), want ? "compact" : "normal", `${key} ${mode} view`);
      }
      checked++;
    }
  }
  assert.ok(checked > 3000);
  // 行の高さ：600px 未満で外の余白が詰まり曲一覧の側が 11px 広がるため、1つの境目では合わせきれない（density.ts の LIST_BELOW の説明）
  const want: string[] = [];
  for (let W = 600; W <= 611; W++) want.push(`${W}px`);
  assert.deepEqual(diffs, want);
  for (let W = 600; W <= 611; W++) assert.equal(listLayoutFor(mainWidth(W, true)).rowH, 48);
});

test("3.7.0 の値の読み替え（画面側）：narrow → 使わない＋自動、off → 使わない、always → いつも", () => {
  assert.deepEqual([compactListMode({ compactList: "narrow" }), compactAutoOn({ compactList: "narrow" })], ["off", true]);
  assert.deepEqual([compactListMode({ compactList: "off" }), compactAutoOn({ compactList: "off" })], ["off", false]);
  assert.deepEqual([compactListMode({ compactList: "always" }), compactAutoOn({ compactList: "always" })], ["always", false]);
  // Rust の読み替えと同じ（"narrow" を sanitize で off ＋ compact_auto にする）
  const rs = read("src-tauri/src/settings/store.rs");
  const san = rs.slice(rs.indexOf("pub fn sanitize"), rs.indexOf("\n    }\n", rs.indexOf("pub fn sanitize")));
  assert.match(san, /if self\.ui\.compact_list == COMPACT_LIST_LEGACY_NARROW \{\s*self\.ui\.compact_list = "off"\.into\(\);\s*self\.ui\.compact_auto = true;\s*\}/);
});

// ---------- 上の帯のモード切替 ----------

test("モード切替：通常／コンパクト／詳細（詳細は scripts\\ui-detail.test.mts。ミニはあとで足す）。今のモードは見た目どおり（自動でコンパクトのときもコンパクト）", () => {
  assert.deepEqual(VIEW_MODES.map((m) => [m.id, m.label]), [["normal", "通常"], ["compact", "コンパクト"], ["detail", "詳細"]]);
  const narrow = COMPACT_AUTO_BELOW - 1;
  const wide = COMPACT_AUTO_BELOW;
  assert.equal(currentViewMode({ compactList: "off" }, narrow), "normal");
  assert.equal(currentViewMode({ compactList: "off", compactAuto: true }, narrow), "compact");
  assert.equal(currentViewMode({ compactList: "off", compactAuto: true }, wide), "normal", "窓を広げたら通常へ戻る（設定は書き換えない）");
  assert.equal(currentViewMode({ compactList: "always" }, 2000), "compact");
});

test("モード切替を押したとき：押したほうがそのまま設定になる。自動でコンパクトのときに「通常」→ 自動をオフ、「コンパクト」→ いつも", () => {
  const narrow = COMPACT_AUTO_BELOW - 1;
  const wide = 1400;
  // 自動でコンパクトになっているとき
  assert.deepEqual(pressViewMode({ compactList: "off", compactAuto: true }, narrow, "normal"), { compactList: "off", compactAuto: false });
  assert.deepEqual(pressViewMode({ compactList: "off", compactAuto: true }, narrow, "compact"), { compactList: "always", compactAuto: true });
  // 3.7.0 の "narrow" のまま来ても同じ
  assert.deepEqual(pressViewMode({ compactList: "narrow" }, narrow, "normal"), { compactList: "off", compactAuto: false });
  // 広いとき：自動のチェックはそのまま
  assert.deepEqual(pressViewMode({ compactList: "off", compactAuto: true }, wide, "normal"), { compactList: "off", compactAuto: true });
  assert.deepEqual(pressViewMode({ compactList: "off", compactAuto: false }, wide, "compact"), { compactList: "always", compactAuto: false });
  assert.deepEqual(pressViewMode({ compactList: "always", compactAuto: true }, wide, "normal"), { compactList: "off", compactAuto: true });
  // いつも＋自動で、狭いときに「通常」：使わないにしても自動でコンパクトのままになるので、自動もオフ
  assert.deepEqual(pressViewMode({ compactList: "always", compactAuto: true }, narrow, "normal"), { compactList: "off", compactAuto: false });
  // どの組み合わせでも、押したあとは押したモードに見える
  for (const mode of ["off", "always", "narrow", undefined]) {
    for (const auto of [true, false, undefined]) {
      for (const w of [300, narrow, wide, 2560]) {
        // 詳細は表示の設定を変えない（pressMode。scripts\ui-detail.test.mts）ので、ここは通常とコンパクト
        for (const m of VIEW_MODES.filter((x) => x.id !== "detail") as { id: "normal" | "compact" }[]) {
          const ui = { compactList: mode, compactAuto: auto };
          const next = pressViewMode(ui, w, m.id);
          assert.equal(currentViewMode(next, w), m.id, JSON.stringify({ ui, w, m: m.id }));
          // 「コンパクト」は自動のチェックを変えない。「通常」は自動でコンパクトになっていた幅でだけオフにする
          if (m.id === "compact") assert.equal(next.compactAuto, compactAutoOn(ui));
          else assert.equal(next.compactAuto, compactAutoOn(ui) && w >= COMPACT_AUTO_BELOW);
        }
      }
    }
  }
});

test("モード切替の画面（TopBar）：上の帯の右端（ファイル・フォルダの隣）に1つを選ぶ形。文字が入らないときはアイコンだけで、吹き出しと読み上げの名前を出す", () => {
  const top = read("src/layout/TopBar.tsx");
  const actions = top.slice(top.indexOf('<div className="topbar-actions">'));
  assert.ok(actions.indexOf("<ViewModeSwitch") > actions.indexOf("フォルダ</span>"), "フォルダのボタンの右");
  assert.match(top, /role="radiogroup" aria-label="表示のモード"/);
  assert.match(top, /<button role="radio" aria-checked=\{cur === m\.id\} aria-label=\{m\.label\}/);
  // 文字を出すときは吹き出しを出さず、アイコンだけのときは文字の吹き出し（マウスを乗せる・キーボードで移る。Tip）
  assert.match(top, /<Tip key=\{m\.id\} text=\{props\.labels \? null : m\.label\}[^>]*place="below"/);
  // 名前は aria-label で渡すので、吹き出しの文を読み上げ用（aria-describedby）には付けない（同じ名前を二重に読ませない）
  assert.match(top, /<Tip key=\{m\.id\}[^>]*describe=\{false\}>/);
  const tipSrc = read("src/components/Tip.tsx");
  assert.match(tipSrc, /const parts = props\.describe === false \? \{ \.\.\.full, childProps: \{\}, srText: null \} : full;/);
  assert.match(tipSrc, /\{full\.srText\}\s*<\/div>,/);
  assert.match(top, /\{props\.labels \? <span>\{m\.label\}<\/span> : null\}/);
  // 押したら pressViewMode の値を保存する（設定画面と同じ compactList・compactAuto）。v3.8.0：詳細を足したので pressMode を通す
  // （詳細でなければ pressViewMode と同じ。詳細のときは詳細を抜けたあとの幅で決める）
  assert.match(top, /ui: \{ \.\.\.x\.ui, \.\.\.pressMode\(x\.ui, listPaneWidth\.get\(\), listPaneExitWidth\.get\(\), detail, m\)\.settings \}/);
  // 吹き出しは下にも出せる（右に出すと画面の外にはみ出すため）。包み方は1つのまま（tipParts）
  const tip = read("src/components/Tip.tsx");
  assert.match(tip, /place\?: "right" \| "below"/);
  assert.match(rule(".tip-bubble.below"), /transform: none;/);
});

test("モード切替の文字を出す幅：base.css の寸法と同じ数え方。狭めていくと、ファイル・フォルダの文字より先に消える（一度消えた文字がまた出ない）", () => {
  // TOPBAR の値は base.css の値から（スキンで文字は ±2px まで大きくなるので 16px で数える）
  assert.match(rule(".topbar"), /gap: 10px;\s*padding: 0 14px;/);
  assert.match(rule(".topbar-actions"), /gap: 6px;/);
  assert.match(rule(".btn"), /gap: 6px;\s*height: 32px;\s*padding: 0 12px;/);
  assert.match(rule(".btn .icon"), /width: 16px;/);
  assert.match(rule(".seg.view-mode button"), /gap: 6px;\s*height: 100%;\s*padding: 0 10px;/);
  assert.match(rule(".seg.view-mode.icons button"), /width: 36px;\s*padding: 0;/);
  assert.match(rule(".seg.view-mode .icon"), /width: 16px;\s*height: 16px;/);
  assert.equal(TOPBAR.frame, 2 + 14 * 2);
  assert.equal(TOPBAR.fileFolderLabeled, 2 * (12 * 2 + 2 + 16 + 6 + 4 * 16) + 6);
  // v3.8.0：3つ目の「詳細」（2字）を足した
  assert.equal(TOPBAR.switchLabeled, 2 + (10 * 2 + 16 + 6 + 2 * 16) + (10 * 2 + 16 + 6 + 5 * 16) + (10 * 2 + 16 + 6 + 2 * 16));
  assert.equal(TOPBAR.switchIcons, 2 + 36 * 3);
  // 曲一覧の側の幅を広げていくと、文字は一度出たら消えない（単調）
  let seen = false;
  for (let w = 300; w <= 2600; w++) {
    const on = viewModeLabelsFit(w, listLayoutFor(w).topbarNarrow);
    if (seen) assert.ok(on, `labels disappear again at ${w}`);
    if (on) seen = true;
    if (listLayoutFor(w).topbarNarrow) assert.equal(on, false, String(w));
  }
  // 最小の窓（480px）ではアイコンだけ。広げたサイドバーの 1100px の窓（曲一覧の側 834px）では文字
  assert.equal(viewModeLabelsFit(mainWidth(480, true), true), false);
  assert.equal(viewModeLabelsFit(mainWidth(1100, false), false), true);
});

// v3.8.0：幅の下の端を 300 → 360px にした。モード切替が3つ（アイコンだけで 110px）になり 336px 未満には入らないが、
// 曲一覧の側がいちばん狭くなるのは詳細の左右の 360px（仕様 3.2。ふだんは最小の窓で 398px）なので、実際の幅の全部を当てている
test("上の帯の幅：曲一覧の側 360〜2600px のどこでも、取り込み中の知らせが出ていても、検索欄の最低幅とファイル・フォルダ・モード切替が入る（モード切替が切れない）", () => {
  // 取り込み中の知らせは縮んで 0 まで（.import-status の min-width: 0）。どの幅でも min-width: 140px を残していない
  const imp = rule(".import-status");
  assert.match(imp, /min-width: 0;/);
  assert.match(imp, /flex: 0 1 auto;/);
  assert.ok(!/\.import-status \{[^}]*min-width: 140px/.test(css), "min-width: 140px が残っていない");
  // 広いときは今までどおり 140px 以上（帯の最低幅。縮めたときは帯ごと切る）
  assert.match(rule(".import-status .progress-line"), /min-width: 140px;/);
  const searchMin = Number(rule(".search").match(/min-width: (\d+)px;/)![1]);
  const fileFolderIcons = 2 * (12 * 2 + 2 + 16) + TOPBAR.actionsGap;
  let worst = Infinity;
  for (let w = 360; w <= 2600; w++) {
    const narrow = listLayoutFor(w).topbarNarrow;
    const labels = viewModeLabelsFit(w, narrow);
    const actions = (narrow ? fileFolderIcons : TOPBAR.fileFolderLabeled) + TOPBAR.actionsGap + (labels ? TOPBAR.switchLabeled : TOPBAR.switchIcons);
    // 枠と余白・検索欄の最低幅・間（検索欄｜知らせ｜右端の並び）・知らせ（縮んで 0）・右端の並び
    const need = TOPBAR.frame + searchMin + TOPBAR.gap * 2 + 0 + actions;
    worst = Math.min(worst, w - need);
    assert.ok(need <= w, `${w}: ${need}`);
  }
  // いちばん余裕の少ない幅でも 0 以上（3.7.0 の min-width: 140px のままだと、674〜703px で 30px あふれていた）
  assert.ok(worst >= 0, String(worst));
  // （ときのモード切替は「通常／コンパクト」の2つで、文字つき 198px）
  const before = 674 - (TOPBAR.frame + searchMin + TOPBAR.gap * 2 + 140 + TOPBAR.fileFolderLabeled + TOPBAR.actionsGap + 198);
  assert.equal(before, -30);
});

// ---------- 再生バー ----------

const ALL: MovableButton[] = ["shuffle", "repeat", "volume", "lyricsRow", "lyrics"];

test("再生バーの寸法：playerLayout.ts の PLAYER と CSS（tokens.css の --player-center-w と base.css）が同じ値", () => {
  assert.match(tokens, /--player-center-w: clamp\(240px, 40%, 560px\);/);
  assert.deepEqual([PLAYER.centerMin, PLAYER.centerRatio, PLAYER.centerMax], [240, 0.4, 560]);
  const p = rule(".player");
  assert.match(p, /grid-template-columns: minmax\(0, 1fr\) var\(--player-center-w\) minmax\(0, 1fr\);/);
  assert.match(p, /gap: (\d+)px;\s*padding: 0 (\d+)px;/);
  assert.equal(Number(p.match(/gap: (\d+)px;/)![1]), PLAYER.gap3);
  assert.equal(Number(p.match(/padding: 0 (\d+)px;/)![1]), PLAYER.pad3);
  const one = rule(".player.one-row");
  assert.match(one, /grid-template-columns: minmax\(0, 1fr\) auto auto;/);
  assert.equal(Number(one.match(/gap: (\d+)px;/)![1]), PLAYER.gap1);
  assert.equal(Number(one.match(/padding: 0 (\d+)px;/)![1]), PLAYER.pad1);
  assert.match(rule(":root:has(.player.one-row)"), new RegExp(`--player-h: ${PLAYER.h1}px;`));
  assert.equal(px(tokens, "--player-h"), PLAYER.h3);
  assert.match(rule(".player-art"), new RegExp(`width: ${PLAYER.art3}px;`));
  assert.match(rule(".player.one-row .player-art"), new RegExp(`width: ${PLAYER.art1}px;`));
  assert.match(rule(".player-left"), new RegExp(`gap: ${PLAYER.artGap}px;`));
  assert.match(rule(".player.three-col .player-controls"), /grid-template-columns: 36px 36px 44px 36px 36px;/);
  assert.match(rule(".player-controls .icon-btn"), new RegExp(`width: ${PLAYER.btn}px;`));
  assert.match(rule(".player-controls .play-btn"), new RegExp(`width: ${PLAYER.play}px;`));
  assert.match(rule(".player-right"), new RegExp(`gap: ${PLAYER.btnGap}px;`));
  assert.match(rule(".volume"), /gap: 6px;/);
  assert.match(rule(".volume .range"), /width: 96px;/);
  assert.equal(PLAYER.volume, 36 + 6 + 96);
  // 再生バーの枠は .panel の 1px
  assert.match(rule(".panel"), /border: 1px solid/);
  assert.equal(PLAYER.border, 2);
  // 3列の高さに真ん中の列（ボタン 44 ＋ 間 4 ＋ シークバー 16）が入る
  assert.match(rule(".range"), /height: 16px;/);
  assert.ok(PLAYER.play + 4 + 16 <= PLAYER.h3 - 2);
  // 1段の高さに、ジャケット・再生ボタン・上の縁の線（12px）が重ならずに入る
  assert.match(rule(".seek-line.range"), /height: 12px;/);
  assert.ok((PLAYER.h1 - PLAYER.art1) / 2 >= 12, "上の縁の線の下にジャケットが来る");
});

test("再生バー：3列か1段かは、真ん中の列（40%・240〜560px）と左右の最低幅が入るかで決まる（窓の幅の決まった値で区切らない）", () => {
  assert.equal(playerLeftMin(), 60 + 12 + 120);
  assert.equal(playerRightMin(), 138 + 4 + 36);
  assert.equal(playerRightMin(80), 80 + 4 + 138 + 4 + 36, "出力ボタンの幅を渡すと最低幅に入る");
  assert.equal(playerCenterWidth(100), 240);
  assert.equal(playerCenterWidth(1000), 400);
  assert.equal(playerCenterWidth(5000), 560);
  // 幅を広げていくと、ある幅から先はずっと3列（単調。行ったり来たりしない）
  let firstThree = -1;
  for (let w = 300; w <= 3000; w++) {
    const L = playerLayout(w, () => true);
    if (L.mode === "three" && firstThree < 0) firstThree = w;
    if (firstThree >= 0) assert.equal(L.mode, "three", String(w));
  }
  // 今の値では再生バーの幅 728px（窓の幅で 748px）から3列。理由：左の最低幅 192px ×2 ＋ 間 16×2 ＋ 真ん中 40% が入る幅
  assert.equal(firstThree, 728);
  assert.equal(playerLayout(barWidth(747), () => true).mode, "one");
  assert.equal(playerLayout(barWidth(748), () => true).mode, "three");
  // v3.8.0：出力ボタンを足しても、入らないときは全体の音量を「···」へ移して3列を保つ。3列が始まる幅は同じ
  assert.equal(playerLayout(728, () => true, 120).mode, "three");
  assert.equal(playerLayout(727, () => true, 120).mode, "one");
});

test("再生バー（出力ボタン 112px あり。v3.8.0）：3列は出力ボタンが無いときと同じ再生バーの幅 728px（窓 748px）から。入らない幅では全体の音量を「···」へ移す（728〜1067px）。再生ボタンは中央のまま", () => {
  const out = PLAYER.output3;
  assert.equal(out, 112);
  const volInMore: number[] = [];
  const lyricsInMore: number[] = [];
  let firstThree = -1;
  for (let w = 480; w <= 2600; w++) {
    const L = playerLayout(w, () => true, out);
    if (L.mode === "one") {
      assert.equal(firstThree, -1, `one again at ${w}`);
      continue;
    }
    if (firstThree < 0) firstThree = w;
    // 再生ボタンは中央：左右の列が同じ幅で、枠＋余白＋左＋間＋真ん中＋間＋右 ＝ 再生バーの幅
    assert.ok(Math.abs(PLAYER.border + PLAYER.pad3 * 2 + L.sideW * 2 + PLAYER.gap3 * 2 + L.centerW - w) < 1e-9, String(w));
    // 右の列（出力ボタン・並べたもの・「···」）が列に入る
    const items = [out, ...L.right.map((b) => (b === "volume" ? PLAYER.volume : PLAYER.btn)), ...(L.more.length ? [PLAYER.more] : [])];
    assert.ok(items.reduce((a, b) => a + b, 0) + (items.length - 1) * PLAYER.btnGap <= L.sideW + 1e-9, String(w));
    // 全体の音量は、右の列か「···」の先頭のどちらか1か所。音量が「···」にあるときは、歌詞のボタンも「···」（歌詞だけが右に残る段は無い）
    if (L.more[0] === "volume") {
      volInMore.push(w);
      assert.ok(!L.right.includes("volume") && !L.right.includes("lyricsRow") && !L.right.includes("lyrics"), String(w));
    } else assert.ok(L.right[0] === "volume", String(w));
    if (L.more.includes("lyrics")) lyricsInMore.push(w);
    assert.equal(L.centerShuffle && L.centerRepeat, true);
  }
  assert.equal(firstThree, 728);
  assert.deepEqual([volInMore[0], volInMore[volInMore.length - 1], volInMore.length], [728, 1067, 1067 - 728 + 1]);
  // 歌詞のボタンが右の列に並ぶのは 1201px から（出力ボタン 112 ＋ 音量 138 ＋ 36 ＋ 36 ＋ 間 4×3 ＝ 334px が右の列に入る幅）
  assert.deepEqual([lyricsInMore[0], lyricsInMore[lyricsInMore.length - 1]], [728, 1200]);
  // 「···」の中の並びは1段のときと同じ（音量が先頭）
  assert.deepEqual(playerLayout(800, () => true, out).more, ["volume", "lyricsRow", "lyrics"]);
  assert.deepEqual(playerLayout(1100, () => true, out).right, ["volume"]);
  assert.deepEqual(playerLayout(1300, () => true, out).right, ["volume", "lyricsRow", "lyrics"]);
  // 音量を隠している人：歌詞のボタンだけで決まる（出力 112 ＋ 36 ＋ 36 ＋ 間 4×2 ＝ 192px は 3列の最低幅と同じなので、3列になった幅から右の列）
  assert.deepEqual(playerLayout(728, (b) => b !== "volume", out).right, ["lyricsRow", "lyrics"]);
  // 出力ボタンを隠していれば、と同じ（音量はいつも右の列）
  for (let w = 728; w <= 1300; w++) assert.equal(playerLayout(w, () => true, 0).right[0], "volume", String(w));
});

test("再生バー（3列）：左右は同じ幅で、再生ボタンは再生バーの中央。左右の中身は列からはみ出さない（出しているボタンの全部の組み合わせ・幅 728〜2600px）", () => {
  const combos: MovableButton[][] = [];
  for (let mask = 0; mask < 32; mask++) combos.push(ALL.filter((_, i) => mask & (1 << i)));
  for (let w = 728; w <= 2600; w += 3) {
    for (const on of combos) {
      const shown = (b: MovableButton) => on.includes(b);
      const L = playerLayout(w, shown);
      assert.equal(L.mode, "three");
      const inner = w - PLAYER.border - PLAYER.pad3 * 2;
      // 枠＋余白＋左＋間＋真ん中＋間＋右 ＝ 再生バーの幅。左右が同じなので、真ん中の列の中央（＝再生ボタン）は再生バーの中央
      assert.ok(Math.abs(PLAYER.border + PLAYER.pad3 * 2 + L.sideW * 2 + PLAYER.gap3 * 2 + L.centerW - w) < 1e-9);
      assert.ok(L.centerW >= PLAYER.centerMin && L.centerW <= PLAYER.centerMax && Math.abs(L.centerW - Math.min(560, Math.max(240, inner * 0.4))) < 1e-9);
      assert.ok(L.sideW >= playerLeftMin(), `left ${w}`);
      // 右の列：並べたものと「···」（中身があるときだけ）の幅が列に入る
      const items = [...L.right.map((b) => (b === "volume" ? PLAYER.volume : PLAYER.btn)), ...(L.more.length ? [PLAYER.more] : [])];
      const rightW = items.reduce((a, b) => a + b, 0) + Math.max(0, items.length - 1) * PLAYER.btnGap;
      assert.ok(rightW <= L.sideW + 1e-9, `right ${w} ${on}`);
      // 真ん中の列は 240px 以上なので、シャッフル・リピートはいつも両脇に入る（36×4＋44＋4×4＝204px）
      assert.equal(CENTER_FULL_W, 204);
      assert.equal(L.centerShuffle, shown("shuffle"));
      assert.equal(L.centerRepeat, shown("repeat"));
      // 出しているボタンは、真ん中・右・「···」のどれか1か所だけ。隠したボタンはどこにも出さない
      const placed = [...(L.centerShuffle ? ["shuffle"] : []), ...(L.centerRepeat ? ["repeat"] : []), ...L.right, ...L.more];
      assert.deepEqual([...placed].sort(), [...on].sort(), `${w} ${on}`);
      // 全体の音量は、出していればいつも右の列
      if (shown("volume")) assert.ok(L.right.includes("volume"));
      // 歌詞のボタン（流れる歌詞の列・歌詞）は、出しているものが全部右の列に入るか、まとめて「···」か（片方だけが残る段は無い）
      const lyr = (["lyricsRow", "lyrics"] as const).filter(shown);
      assert.ok(lyr.every((b) => L.right.includes(b)) || lyr.every((b) => L.more.includes(b)), `${w} ${on}`);
    }
  }
  // 広い窓では「···」は出ない（全部が右の列に入る）
  const wide = playerLayout(barWidth(1200), () => true);
  assert.deepEqual([wide.right, wide.more], [["volume", "lyricsRow", "lyrics"], []]);
  // 全部出しているとき：再生バーの幅 728〜813px は歌詞のボタンが2つとも「···」、814px から右の列（README の「再生バーの形」に書いた幅）。
  // 音量 138 ＋ 36 ＋ 36 ＋ 間 4×2 ＝ 218px が右の列に入る幅から
  for (let w = 728; w <= 1200; w++) {
    const L = playerLayout(w, () => true);
    const want = w < 814 ? [["volume"], ["lyricsRow", "lyrics"]] : [["volume", "lyricsRow", "lyrics"], []];
    assert.deepEqual([L.right, L.more], want, String(w));
  }
  // 流れる歌詞の列のボタンを隠していれば、3列になった幅から歌詞のボタンは右の列（音量＋歌詞＝178px は右の列の最低幅と同じ）
  const noRow = playerLayout(728, (b) => b !== "lyricsRow");
  assert.deepEqual([noRow.right, noRow.more], [["volume", "lyrics"], []]);
});

test("再生バー（1段）：中は前・再生・次、右は「···」（全体の音量・シャッフル・リピート・歌詞のボタン。隠したものは出さない）", () => {
  const L = playerLayout(barWidth(480), () => true);
  assert.equal(L.mode, "one");
  assert.deepEqual(L.more, ["volume", "shuffle", "repeat", "lyricsRow", "lyrics"]);
  assert.deepEqual(L.right, []);
  assert.equal(L.centerShuffle || L.centerRepeat, false);
  const hidden = playerLayout(barWidth(480), (b) => b !== "volume" && b !== "lyrics");
  assert.deepEqual(hidden.more, ["shuffle", "repeat", "lyricsRow"]);
  // 全部隠したら「···」も出さない（PlayerBar は more が空なら出さない）
  assert.deepEqual(playerLayout(barWidth(480), () => false).more, []);
  const pb = read("src/layout/PlayerBar.tsx");
  assert.match(pb, /\{L\.more\.length > 0 \? <PlayerMore items=\{L\.more\}/);
});

test("再生バーの画面（PlayerBar）：自分の幅を測って playerLayout で形を決める。1段ではシークバーを上の縁の線にし、アーティスト名と歌詞の1行の代わりに時間を出す", () => {
  const pb = read("src/layout/PlayerBar.tsx");
  assert.match(pb, /new ResizeObserver\(put\)/);
  // v3.8.0：出力ボタンを出していれば、その幅（PLAYER.output3）を渡す
  assert.match(pb, /const L = playerLayout\(barW, shown, outputsOn \? PLAYER\.output3 : 0\);/);
  assert.match(pb, /className=\{"player panel " \+ \(one \? "one-row" : "three-col"\)\}/);
  assert.match(pb, /\{one \? <SeekBar disabled=\{!t\} variant="line" \/> : null\}/);
  assert.match(pb, /\{one \? null : <SeekBar disabled=\{!t\} \/>\}/);
  assert.match(pb, /t \? <SeekTime \/> : null/);
  assert.match(pb, /\{!one && t && !rowOn \? <PlayerLyricLine trackId=\{t\.id\} \/> : null\}/);
  // 3列の5つの枠：出さないときも空の枠を残す（再生ボタンを動かさない）
  assert.match(pb, /\(on \? movable\(b, false\) : <span aria-hidden="true" \/>\)/);
  // 「···」は上に開き、Esc で閉じてボタンへ戻る。中の項目はボタンの組（読み上げで名前とオン・オフ）
  assert.match(pb, /aria-haspopup="dialog"/);
  assert.match(pb, /aria-expanded=\{open\}/);
  assert.match(pb, /role="dialog" aria-label="その他の操作"/);
  assert.match(pb, /if \(e\.key === "Escape"\)/);
  assert.match(pb, /<button className="menu-item" onClick=\{o\.onClick\} title=\{o\.title\} aria-pressed=\{o\.on\}>/);
  // 出力ボタンは、右の列の全体の音量の左
  const right = pb.slice(pb.indexOf('<div className="player-right">'));
  assert.ok(right.indexOf("<OutputButton") >= 0 && right.indexOf("<OutputButton") < right.indexOf("L.right.map"));
  // シークバーの線の形：時間の文字を持たず、読み上げに今の時間と長さを渡す
  const sb = read("src/components/SeekBar.tsx");
  assert.match(sb, /if \(props\.variant === "line"\) return input;/);
  assert.match(sb, /aria-valuetext=\{`\$\{formatTime\(cur\)\} \/ \$\{formatTime\(dur\)\}`\}/);
});

// ---------- 最小の窓・スキン ----------

test("最小の窓は 480×480（tauri.conf.json）", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
  const w = conf.app.windows[0];
  assert.deepEqual([w.minWidth, w.minHeight], [480, 480]);
});

test("480×480 の窓で、再生バー・上の帯・曲一覧がはみ出さない（幅の計算）", () => {
  const W = 480;
  // 再生バー：1段。左（残り）＝ 再生バーの中の幅 − 中（前・再生・次）− 右（出力ボタン 80 ＋ 間 4 ＋「···」）− 間×2
  // （v3.8.0 で出力ボタンを足した。では 266px、出力ボタンを隠せば今も 266px）
  const bar = barWidth(W);
  const L1 = playerLayout(bar, () => true, PLAYER.output3);
  assert.equal(L1.mode, "one");
  assert.equal(L1.output, PLAYER.output1);
  const inner1 = bar - PLAYER.border - PLAYER.pad1 * 2;
  const center1 = PLAYER.btn * 2 + PLAYER.play + PLAYER.btnGap * 2;
  assert.equal(inner1 - center1 - PLAYER.more - PLAYER.gap1 * 2, 266);
  const left1 = inner1 - center1 - (L1.output + PLAYER.btnGap) - PLAYER.more - PLAYER.gap1 * 2;
  assert.equal(left1, 182);
  assert.ok(left1 - PLAYER.art1 - 10 >= 120, "曲名と時間の文字に 120px 以上残る");
  // 上の帯：枠＋余白、検索欄の最低幅（base.css の .search の min-width）、ファイル・フォルダ（アイコンだけ）、切替（アイコンだけ）
  const main = mainWidth(W, true);
  assert.equal(main, 398);
  const searchMin = Number(rule(".search").match(/min-width: (\d+)px;/)![1]);
  const fileFolderIcons = 2 * (12 * 2 + 2 + 16) + TOPBAR.actionsGap;
  const need = TOPBAR.frame + searchMin + TOPBAR.gap + fileFolderIcons + TOPBAR.actionsGap + TOPBAR.switchIcons;
  assert.ok(need <= main, `${need} <= ${main}`);
  // 取り込み中の知らせは縮む（最低幅 0。検索欄と切替を押し出さない。どの幅でも。下の「上の帯の幅」のテスト）
  const imp = rule(".import-status");
  assert.ok(/min-width: 0;/.test(imp) && /flex: 0 1 auto;/.test(imp) && /overflow: hidden;/.test(imp), imp);
  // 曲一覧：曲一覧の側 398px。決まった幅の列（ジャケット 36・時間 52・お気に入り 68・ごみ箱 44・年間まとめの順位 40 と回数 44）と間を全部足しても入る（タイトルなどは minmax(0, …) で残りを分ける）
  const L = listLayoutFor(main);
  assert.equal(L.tight, true);
  const innerList = main - 2 - 10 * 2;
  const fixed = [L.artSize, 52, 68, 44, 40, 44];
  const cols = fixed.length + 1; // ＋タイトル
  assert.ok(innerList - fixed.reduce((a, b) => a + b, 0) - (cols - 1) * 10 >= 0);
  // 高さ：窓 480 − 外の余白 6×2 − 間 6 − 再生バー 76 − 流れる歌詞の列 48 ＝ 曲一覧の側 338px。上の帯 56 ＋ 見出し ＋ 列の見出し 34 ＋ 1行 48 が入る
  const mainH = 480 - appPadNarrow * 2 - appGapNarrow - PLAYER.h1 - px(tokens, "--lyrics-row-h");
  assert.equal(mainH, 338);
  assert.ok(mainH - px(tokens, "--topbar-h") - 60 - 34 >= 48 * 3, "見出しを 60px と見ても、3行以上見える");
});

test("スキンから変えさせない変数：Rust（skins\\tokens.rs）と画面（src\\skins\\tokens.ts）の LOCKED_VARS が同じ名前・同じ並び。--player-center-w を足した", () => {
  const rs = read("src-tauri/src/skins/tokens.rs");
  const m = rs.match(/pub const LOCKED_VARS: \[&str; (\d+)\] = \[([\s\S]*?)\];/);
  assert.ok(m, "Rust LOCKED_VARS");
  const rsNames = [...m![2].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  assert.equal(rsNames.length, Number(m![1]));
  const ts = read("src/skins/tokens.ts");
  const t = ts.match(/export const LOCKED_VARS = \[([\s\S]*?)\];/);
  assert.ok(t, "TS LOCKED_VARS");
  const tsNames = [...t![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  assert.deepEqual(tsNames, rsNames);
  assert.ok(rsNames.includes("--player-center-w"));
  // 既定の値はテーマの変数（tokens.css）にある。スキンの手引き（docs）にも、変えられない変数として書いてある
  for (const v of rsNames.filter((n) => !n.startsWith("--bg-") && n !== "--ui-alpha")) assert.ok(tokens.includes(`${v}:`), v);
  assert.ok(read("docs/着せ替え_仕様.md").includes("`--player-center-w`"));
});
