// 着せ替え（v3.2.0）の画面側の決まりを確かめる：色と文字の値の確かめ・状態ごとのアイコンの選び方・立ち絵を出す／隠す条件・コントラストの計算。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/skins.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import {
  applyCssVars,
  COLOR_VARS,
  DEFAULT_FONT_STACK,
  LOCKED_VARS,
  SKIN_VARS,
  shadowValues,
  skinCssVars,
  validColor,
  validColorToken,
  validCommonToken,
  validFontName,
  validRgbTriple,
} from "../src/skins/tokens.ts";
import { pickIconPath, repeatState, shuffleState, starState, volumeState } from "../src/skins/icons.ts";
import { characterLayout, clampScale, MIN_WINDOW_FOR_CHARACTER } from "../src/skins/character.ts";
import { contrastWarnings, evaluate, ratio } from "../src/skins/contrast.ts";
import { warningRows } from "../src/skins/warnings.ts";
import { evaluate as evaluateFile, readTheme } from "./theme-contrast.mts";

const light = fileURLToPath(new URL("../src/theme/light.css", import.meta.url));
const dark = fileURLToPath(new URL("../src/theme/dark.css", import.meta.url));

test("色の値：#rgb・#rrggbb・rgb()・rgba() だけ。形をそろえる", () => {
  assert.equal(validColor("#ABC"), "#aabbcc");
  assert.equal(validColor(" #7c6cff "), "#7c6cff");
  assert.equal(validColor("rgb(1, 2, 255)"), "#0102ff");
  assert.equal(validColor("rgba(255,255,255,0.07)"), "rgba(255, 255, 255, 0.07)");
  assert.equal(validColor("rgba(0, 0, 0, .5)"), "rgba(0, 0, 0, 0.5)");
  for (const bad of [
    "red",
    "#12345",
    "#ggg",
    "rgb(256, 0, 0)",
    "rgba(0, 0, 0, 1.5)",
    "rgba(0, 0, 0, 0.1234)",
    "rgb(0, 0, 0); background: url(x)",
    "url(https://example.invalid/x.png)",
    "var(--c-bg)",
    "expression(alert(1))",
    "",
    42,
    null,
  ]) {
    assert.equal(validColor(bad), null, String(bad));
  }
  assert.equal(validRgbTriple("15,15,22"), "15, 15, 22");
  assert.equal(validRgbTriple("#000000"), null);
  assert.equal(validColorToken("--c-panel-rgb", "1, 2, 3"), "1, 2, 3");
  assert.equal(validColorToken("--c-bg", "1, 2, 3"), null);
  // 許していない名前（寸法など）は、値が正しくても受けない
  assert.equal(validColorToken("--row-h", "#000000"), null);
  assert.equal(validColorToken("--anything", "#000000"), null);
});

test("文字の大きさは既定 ±2px・角丸 0〜24px・動き 100〜250ms・寸法と背景の設定は受けない", () => {
  assert.equal(validCommonToken("--fs-body", 15), "15px");
  assert.equal(validCommonToken("--fs-body", "16px"), "16px");
  assert.equal(validCommonToken("--fs-body", "17px"), null);
  assert.equal(validCommonToken("--fs-title", 28), null);
  assert.equal(validCommonToken("--fs-small", 10), "10px");
  assert.equal(validCommonToken("--fs-small", "1em"), null);
  assert.equal(validCommonToken("--fs-small", 10.5), null);
  assert.equal(validCommonToken("--radius", 0), "0px");
  assert.equal(validCommonToken("--radius", 25), null);
  assert.equal(validCommonToken("--dur-hover", "150ms"), "150ms");
  assert.equal(validCommonToken("--dur-hover", 50), null);
  assert.equal(validCommonToken("--ease", "linear"), "linear");
  assert.equal(validCommonToken("--ease", "cubic-bezier(0,0,0,0)"), null);
  for (const v of LOCKED_VARS) assert.equal(validCommonToken(v, 1), null, v);
  assert.ok(validFontName("Yu Gothic UI"));
  assert.ok(validFontName("BIZ UDPゴシック"));
  for (const bad of ["", " Meiryo", 'a"b', "x;y", "url(x)", "a,b", "a".repeat(65)]) assert.ok(!validFontName(bad), bad);
});

test("当てる変数：今のテーマの色・影・共通の値・フォント。確かめに通らない値は入れず名前を返す", () => {
  const a = {
    dark: { tokens: { "--c-accent": "#d08a55", "--c-panel-rgb": "28, 30, 38" }, shadow: "soft" },
    light: { tokens: { "--c-accent": "#87502b" }, shadow: null },
    fontFamily: ["Yu Gothic UI"],
    tokens: { "--fs-body": "15px", "--radius": "6px" },
  };
  const d = skinCssVars(a, "dark");
  assert.deepEqual(d.rejected, []);
  assert.equal(d.vars["--c-accent"], "#d08a55");
  assert.equal(d.vars["--c-panel-rgb"], "28, 30, 38");
  assert.equal(d.vars["--shadow-panel"], shadowValues("dark", "soft")!.panel);
  assert.equal(d.vars["--fs-body"], "15px");
  assert.equal(d.vars["--font-ui"], `"Yu Gothic UI", ${DEFAULT_FONT_STACK}`);
  const l = skinCssVars(a, "light");
  assert.equal(l.vars["--c-accent"], "#87502b");
  assert.equal(l.vars["--shadow-panel"], undefined);
  // Light の色が無いスキンは、Light では色を当てない（今の Light のまま）
  assert.deepEqual(Object.keys(skinCssVars({ ...a, light: null }, "light").vars).filter((k) => k.startsWith("--c-")), []);
  // 細工した値（取り込みの確かめを通らなかったもの）は当てない
  const evil = skinCssVars({ ...a, dark: { tokens: { "--c-bg": "red; background: url(https://example.invalid)", "--row-h": "#000000" }, shadow: "x" }, fontFamily: ['a";}'], tokens: { "--sidebar-w": "9px" } }, "dark");
  assert.deepEqual(evil.rejected.sort(), ["--c-bg", "--row-h", "--sidebar-w", "fontFamily", "shadow"].sort());
  for (const k of Object.keys(evil.vars)) assert.ok(!LOCKED_VARS.includes(k), k);
  // スキンが無い（既定のスキン）なら何も当てない
  assert.deepEqual(skinCssVars(null, "dark"), { vars: {}, rejected: [] });
});

test("当て方：先に全部外す（既定のスキンに戻すと何も残らない）。許した名前のほかは当てない", () => {
  const m = new Map<string, string>([["--bg-dim", "0.45"], ["--c-accent", "#000000"]]);
  const style = { setProperty: (n: string, v: string) => void m.set(n, v), removeProperty: (n: string) => m.delete(n) };
  applyCssVars(style, { "--c-accent": "#123456", "--row-h": "999px" });
  assert.equal(m.get("--c-accent"), "#123456");
  assert.equal(m.get("--row-h"), undefined);
  // 背景の設定（BackgroundLayer が当てる）には触らない
  assert.equal(m.get("--bg-dim"), "0.45");
  applyCssVars(style, {});
  assert.deepEqual([...m.keys()], ["--bg-dim"]);
  for (const v of COLOR_VARS) assert.ok(SKIN_VARS.includes(v));
  for (const v of LOCKED_VARS) assert.ok(!SKIN_VARS.includes(v), v);
});

test("状態の切り替え点は今の SVG と同じ（音量 0.01・0.5／お気に入り／リピート／シャッフル）", () => {
  assert.equal(volumeState(true, 0.9), "muted");
  assert.equal(volumeState(false, 0), "zero");
  assert.equal(volumeState(false, 0.01), "zero");
  assert.equal(volumeState(false, 0.02), "low");
  assert.equal(volumeState(false, 0.5), "low");
  assert.equal(volumeState(false, 0.51), "high");
  assert.equal(volumeState(false, undefined), "high");
  assert.equal(starState(true), "filled");
  assert.equal(starState(undefined), "outline");
  assert.equal(repeatState("off", false), "off");
  assert.equal(repeatState("all", false), "all");
  assert.equal(repeatState("one", true), "one");
  assert.equal(repeatState(undefined, true), "one");
  assert.equal(repeatState(undefined, false), "default");
  assert.equal(shuffleState(true), "on");
  assert.equal(shuffleState(false), "off");
  assert.equal(shuffleState(undefined), "default");
});

test("アイコンの画像の選び方：状態 → default → 無し、テーマ → もう片方、動きを減らす → 最初の1コマ", () => {
  const A = (p: string, still: string | null = null) => ({ path: p, still });
  const icons = {
    IconVolume: { muted: { dark: A("m-d"), light: A("m-l") }, default: { dark: A("v"), light: null } },
    IconPlay: { default: { dark: null, light: A("play-l") } },
    IconStar: { filled: { dark: A("star.gif", "star.first.png"), light: A("star.gif", "star.first.png") } },
  };
  assert.equal(pickIconPath(icons, "IconVolume", "muted", "dark", false), "m-d");
  assert.equal(pickIconPath(icons, "IconVolume", "muted", "light", false), "m-l");
  // 状態の画像が無ければ default
  assert.equal(pickIconPath(icons, "IconVolume", "low", "dark", false), "v");
  // テーマの画像が無ければ、もう片方
  assert.equal(pickIconPath(icons, "IconVolume", "low", "light", false), "v");
  assert.equal(pickIconPath(icons, "IconPlay", "default", "dark", false), "play-l");
  // 状態の画像も default も無ければ null（今の SVG）
  assert.equal(pickIconPath(icons, "IconStar", "outline", "dark", false), null);
  assert.equal(pickIconPath(icons, "IconPause", "default", "dark", false), null);
  assert.equal(pickIconPath(null, "IconPlay", "default", "dark", false), null);
  // 動きを減らす：動く画像は最初の1コマだけを読む。静止画はそのまま
  assert.equal(pickIconPath(icons, "IconStar", "filled", "dark", false), "star.gif");
  assert.equal(pickIconPath(icons, "IconStar", "filled", "dark", true), "star.first.png");
  assert.equal(pickIconPath(icons, "IconVolume", "muted", "dark", true), "m-d");
});

test("立ち絵を出す・隠す：スキン・設定・歌詞画面・ウィンドウの幅。大きさは 75〜125%", () => {
  const ch = {
    playing: { path: "playing.webp", still: "playing.first.png" },
    stopped: { path: "stopped.png", still: null },
    widthPx: 180,
    maxHeightPx: 420,
    minWindowWidthPx: 920,
  };
  const base = { character: ch, visible: true, scale: 1, view: "all", windowWidth: 1200, playing: false, reducedMotion: false };
  const l = characterLayout(base)!;
  assert.deepEqual(l, { state: "stopped", src: "stopped.png", widthPx: 180, maxHeightPx: 420, railPx: 192 });
  assert.equal(characterLayout({ ...base, playing: true })!.src, "playing.webp");
  // 動きを減らす：最初の1コマ。設定を途中で変えても、次の描画でそのまま反映される（入力だけで決まる）
  assert.equal(characterLayout({ ...base, playing: true, reducedMotion: true })!.src, "playing.first.png");
  // 隠す
  assert.equal(characterLayout({ ...base, character: null }), null);
  assert.equal(characterLayout({ ...base, visible: false }), null);
  assert.equal(characterLayout({ ...base, view: "lyrics" }), null);
  assert.equal(characterLayout({ ...base, windowWidth: 919 }), null);
  assert.ok(characterLayout({ ...base, windowWidth: 920 }));
  // スキンが 760 未満を指定しても、760 未満では出さない
  assert.equal(characterLayout({ ...base, character: { ...ch, minWindowWidthPx: 700 }, windowWidth: MIN_WINDOW_FOR_CHARACTER - 1 }), null);
  // 設定画面などでは出す
  assert.ok(characterLayout({ ...base, view: "settings" }));
  // 大きさ
  assert.equal(characterLayout({ ...base, scale: 1.25 })!.widthPx, 225);
  assert.equal(characterLayout({ ...base, scale: 3 })!.widthPx, 225);
  assert.equal(characterLayout({ ...base, scale: 0.1 })!.maxHeightPx, 315);
  assert.equal(clampScale(Number.NaN), 1);
});

test("コントラストの計算は scripts/theme-contrast.mts と同じ（Light・Dark の全部の組み合わせ）", () => {
  for (const f of [light, dark]) {
    const a = evaluateFile(f);
    const b = evaluate(readTheme(f), {});
    assert.equal(a.length, 69);
    assert.deepEqual(
      b.map((r) => [r.scene, r.name, r.ratio.toFixed(4)]),
      a.map((r) => [r.scene, r.name, r.ratio.toFixed(4)]),
    );
  }
  assert.equal(ratio([255, 255, 255], [0, 0, 0]).toFixed(2), "21.00");
});

test("コントラストの警告：スキンで新しく届かなくなった組み合わせだけ。色を変えないスキンは警告なし", () => {
  const L = readTheme(light);
  const D = readTheme(dark);
  assert.deepEqual(contrastWarnings(L, {}), []);
  // Light の既定は全部届く。読みにくい色を足すと警告が出る
  const w = contrastWarnings(L, { "--c-secondary": "#bbbbbb" });
  assert.ok(w.length > 0);
  assert.ok(w.every((r) => r.name.includes("--c-secondary")));
  // #rgb の形でも計算できる
  assert.ok(contrastWarnings(L, { "--c-secondary": "#bbb" }).length === w.length);
  // Dark で元から届かない組み合わせ（線など）は、関係の無い色を変えただけでは出さない
  assert.deepEqual(contrastWarnings(D, { "--c-ok": "#5fd39a" }), []);
  // 読みやすい色に変えるスキンも警告なし
  assert.deepEqual(contrastWarnings(L, { "--c-primary": "#000000" }), []);
});

test("取り込みの警告：種類ごとにまとめて出た順に並べ、上限を超えた分はその種類の最後に「ほか N 件」", () => {
  const w = (code: string, detail: string) => ({ code, detail });
  const rows = warningRows(
    [w("unknown_field", "a"), w("unknown_token_ignored", "parts.appearance.tokens.x"), w("unknown_field", "b"), w("unknown_field", "c"), w("min_app_version_newer", "9.0.0")],
    [{ code: "unknown_field", count: 4997 }],
  );
  assert.deepEqual(rows, [
    w("unknown_field", "a"),
    w("unknown_field", "b"),
    w("unknown_field", "c"),
    w("unknown_field", "ほか 4997 件"),
    w("unknown_token_ignored", "parts.appearance.tokens.x"),
    w("min_app_version_newer", "9.0.0"),
  ]);
  // 上限に届かなければ、そのまま
  assert.deepEqual(warningRows([w("extensions_ignored", "customCss")], []), [w("extensions_ignored", "customCss")]);
  // 数だけ届いた種類も、行を出す
  assert.deepEqual(warningRows([], [{ code: "unknown_field", count: 2 }]), [w("unknown_field", "ほか 2 件")]);
  assert.deepEqual(warningRows([], []), []);
});
