// 着せ替え（v3.2.0）の画面側の決まりを確かめる：色と文字の値の確かめ・状態ごとのアイコンの選び方・立ち絵を出す／隠す条件・コントラストの計算。
// v3.9.0：立ち絵の欄の並び・使う立ち絵の値（Rust と同じ形）・画面が出す立ち絵は Rust が解いたもの。
// v3.10.0：手で選ぶ色（重なり順・値の確かめ・選び窓の値の作り方・読みやすさの警告・「既定の見た目に戻す」で外れる道）。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/skins.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ALPHA_VARS,
  applyCssVars,
  COLOR_LABELS,
  COLOR_ROWS,
  COLOR_VARS,
  colorKind,
  currentColors,
  layeredCssVars,
  manualColorVars,
  pickedColor,
  readCurrentColor,
  readThemeColors,
  skinThemeColors,
  splitColor,
  warningLabel,
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
import {
  characterFits,
  characterFitState,
  characterFrameKey,
  characterImageStyle,
  characterLayout,
  characterStage,
  characterThresholds,
  characterWindowNote,
  clampScale,
  idealRailWidth,
  listSideWidth,
  MIN_LIST_SIDE_PX,
  RAIL_MAX_PX,
  RAIL_MIN_PX,
  RAIL_PADDING,
  RAIL_WINDOW_RATIO,
  railWidthFor,
  SCALE_MAX,
  SCALE_MIN,
} from "../src/skins/character.ts";
import { APP, centerWidth, DETAIL, detailLayout, type DetailInput } from "../src/layout/detailLayout.ts";
import { LIST_BELOW, listLayoutFor } from "../src/layout/density.ts";
import { color as contrastColor, contrastWarnings, contrastWarningsOrError, evaluate, ratio } from "../src/skins/contrast.ts";
import { warningRows } from "../src/skins/warnings.ts";
import { AVATAR_NAME_MAX, avatarChoice, avatarNameOk, avatarNameReady, avatarRows, canCreateAvatar, cardClickSwitches, cardKeySwitches, CHOICE_NONE, CHOICE_SKIN, skinCharacterChoice } from "../src/skins/avatars.ts";
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

test("立ち絵を出す・隠す：スキン・設定・歌詞画面・曲一覧の側の幅。大きさは 75〜200%（v3.9.0。3.8.1 までは 125%）。専用の幅は窓の幅で決める（直し5回目）", () => {
  const ch = {
    playing: { path: "playing.webp", still: "playing.first.png" },
    stopped: { path: "stopped.png", still: null },
    widthPx: 180,
    maxHeightPx: 420,
    minWindowWidthPx: 920,
  };
  const pane = centerWidth(1200, false); // 934
  const base = { character: ch, visible: true, scale: 1, view: "all", windowWidth: 1200, paneWidth: pane, playing: false, reducedMotion: false };
  const l = characterLayout(base)!;
  // 窓 1200 × 18% = 216
  assert.deepEqual(l, { state: "stopped", src: "stopped.png", widthPx: 216, maxHeightPx: 420, railPx: 228 });
  assert.equal(characterLayout({ ...base, playing: true })!.src, "playing.webp");
  // 動きを減らす：最初の1コマ。設定を途中で変えても、次の描画でそのまま反映される（入力だけで決まる）
  assert.equal(characterLayout({ ...base, playing: true, reducedMotion: true })!.src, "playing.first.png");
  // 隠す
  assert.equal(characterLayout({ ...base, character: null }), null);
  assert.equal(characterLayout({ ...base, visible: false }), null);
  assert.equal(characterLayout({ ...base, view: "lyrics" }), null);
  // 曲一覧の側が足りないときは、隠す前にまず縮める（直し8回目）。140px まで縮めても残す幅を割るときだけ隠す
  assert.equal(characterLayout({ ...base, paneWidth: 228 + MIN_LIST_SIDE_PX - 1 })!.widthPx, 215);
  assert.equal(characterLayout({ ...base, paneWidth: 152 + MIN_LIST_SIDE_PX })!.widthPx, 140);
  assert.equal(characterLayout({ ...base, paneWidth: 152 + MIN_LIST_SIDE_PX - 1 }), null);
  // スキンの minWindowWidthPx は使わない（大きくても小さくても同じ）
  for (const m of [760, 920, 1200, 5000]) assert.deepEqual(characterLayout({ ...base, character: { ...ch, minWindowWidthPx: m } }), l);
  // 設定画面などでは出す
  assert.ok(characterLayout({ ...base, view: "settings" }));
  // 大きさ（窓を広くして、隠れない形で見る）
  const wide = { ...base, windowWidth: 2000, paneWidth: centerWidth(2000, false) };
  assert.equal(characterLayout({ ...wide, scale: 1.25 })!.widthPx, 450);
  assert.equal(characterLayout({ ...wide, scale: 3 })!.widthPx, RAIL_MAX_PX);
  assert.equal(characterLayout({ ...wide, scale: 2 })!.railPx, RAIL_MAX_PX + 12);
  assert.equal(characterLayout({ ...wide, scale: 0.1 })!.maxHeightPx, 315);
  assert.equal(clampScale(Number.NaN), 1);
});

test("立ち絵（v3.3.0）：再生中と停止中の2枚を読み込んだまま、見せる方だけを切り替える（動く画像を頭に戻さない）", () => {
  const ch = {
    playing: { path: "playing.webp", still: "playing.first.png" },
    stopped: { path: "stopped.gif", still: "stopped.first.png" },
    widthPx: 180,
    maxHeightPx: 420,
    minWindowWidthPx: 920,
  };
  const base = { character: ch, visible: true, scale: 1, view: "all", windowWidth: 1200, paneWidth: centerWidth(1200, false), playing: false, reducedMotion: false };
  const stopped = characterStage(base)!;
  const playing = characterStage({ ...base, playing: true })!;
  // 2枚とも同じ src で置き（並びも同じ）、見せる方だけが変わる＝要素も src も変わらない
  const key = (s: typeof stopped) => s.frames.map((f) => `${f.state}:${f.src}:${f.key}`);
  assert.deepEqual(key(stopped), ["stopped:stopped.gif:stopped.gif", "playing:playing.webp:playing.webp"]);
  assert.deepEqual(key(playing), key(stopped));
  assert.deepEqual(stopped.frames.map((f) => f.shown), [true, false]);
  assert.deepEqual(playing.frames.map((f) => f.shown), [false, true]);
  assert.ok(stopped.layout && playing.layout);
  // 歌詞画面・狭い幅：読み込んだまま見えなくする（幅は取らない）。戻ったときも同じ要素・同じ src
  for (const hidden of [{ view: "lyrics" }, { windowWidth: 800, paneWidth: centerWidth(800, false) }]) {
    const h = characterStage({ ...base, ...hidden, playing: true })!;
    assert.equal(h.layout, null);
    assert.deepEqual(key(h), key(stopped));
    assert.ok(h.frames.every((f) => !f.shown));
  }
  // 使う人が切った・立ち絵の無いスキン：外す（画像を手放す。次は頭から）
  assert.equal(characterStage({ ...base, visible: false }), null);
  assert.equal(characterStage({ ...base, character: null }), null);
  // スキンを替えた：画像の場所が変わるので、別の要素になる（新しいスキンの頭から）
  const other = characterStage({ ...base, character: { ...ch, stopped: { path: "x/stopped.gif", still: null } } })!;
  assert.notDeepEqual(key(other), key(stopped));
  // 動きを減らす：今の状態の止まった絵だけ（もう1枚は読まない）
  const r1 = characterStage({ ...base, reducedMotion: true })!;
  assert.deepEqual(r1.frames, [{ key: "stopped.first.png", state: "stopped", src: "stopped.first.png", shown: true }]);
  const r2 = characterStage({ ...base, reducedMotion: true, playing: true })!;
  assert.deepEqual(r2.frames, [{ key: "playing.first.png", state: "playing", src: "playing.first.png", shown: true }]);
  // 止まった絵が無い（動かない画像）ならその絵
  const still = characterStage({ ...base, reducedMotion: true, character: { ...ch, stopped: { path: "stopped.png", still: null } } })!;
  assert.equal(still.frames[0].src, "stopped.png");
  // 再生中と停止中が同じ画像なら1枚だけ（同じ画像を2回読まない）
  const same = characterStage({ ...base, character: { ...ch, playing: { path: "one.webp", still: null }, stopped: { path: "one.webp", still: null } } })!;
  assert.equal(same.frames.length, 1);
  assert.equal(same.frames[0].shown, true);
  // 大きさは出していないときも決まる（戻ったときにずれない）
  assert.equal(characterStage({ ...base, view: "lyrics", scale: 1.25 })!.widthPx, idealRailWidth(1200, 1.25));
});

test("立ち絵（v3.3.0）：画面の要素の key は再生・一時停止で変わらない（同じ画像の1枚・2枚・動きを減らす）", () => {
  const ch = {
    playing: { path: "playing.webp", still: "playing.first.png" },
    stopped: { path: "stopped.gif", still: "stopped.first.png" },
    widthPx: 180,
    maxHeightPx: 420,
    minWindowWidthPx: 920,
  };
  const base = { character: ch, visible: true, scale: 1, view: "all", windowWidth: 1200, paneWidth: centerWidth(1200, false), playing: false, reducedMotion: false };
  // CharacterLayer.tsx が <img key> に使う値（React は key が変われば要素を作り直す＝動く画像が頭に戻る）
  const keys = (i: typeof base) => characterStage(i)!.frames.map((f) => f.key);
  const toggles = (i: typeof base) => {
    const a = keys({ ...i, playing: false });
    const b = keys({ ...i, playing: true });
    const c = keys({ ...i, playing: false });
    return [a, b, c];
  };
  // 再生中と停止中が同じ画像（1枚）：3.3.0 の最初の版は key に状態を入れていて、ここで key が変わっていた
  const one = { ...ch, playing: { path: "one.webp", still: null }, stopped: { path: "one.webp", still: null } };
  const [s1, p1, s1b] = toggles({ ...base, character: one });
  assert.deepEqual(s1, ["one.webp"]);
  assert.deepEqual(p1, s1);
  assert.deepEqual(s1b, s1);
  // 2枚：どちらの key も変わらない（並びも同じ）。2枚の key は重ならない
  const [s2, p2, s2b] = toggles(base);
  assert.deepEqual(s2, ["stopped.gif", "playing.webp"]);
  assert.deepEqual(p2, s2);
  assert.deepEqual(s2b, s2);
  assert.equal(new Set(s2).size, 2);
  // 一時的に隠す（歌詞画面・狭い幅）間も同じ key（戻ったときに作り直さない）
  assert.deepEqual(keys({ ...base, view: "lyrics", playing: true }), s2);
  assert.deepEqual(keys({ ...base, windowWidth: 800, paneWidth: centerWidth(800, false) }), s2);
  // 動きを減らす：止まった絵が同じなら同じ key。違う絵なら変わる（別の画像なので作り直してよい）
  const sameStill = { ...ch, playing: { path: "a.webp", still: "first.png" }, stopped: { path: "b.gif", still: "first.png" } };
  const [r1, r2] = toggles({ ...base, character: sameStill, reducedMotion: true });
  assert.deepEqual(r1, ["first.png"]);
  assert.deepEqual(r2, r1);
  const [d1, d2] = toggles({ ...base, reducedMotion: true });
  assert.notDeepEqual(d1, d2);
  // key は画像の場所だけで決まる（状態を混ぜない）
  for (const st of [characterStage(base)!, characterStage({ ...base, playing: true })!]) {
    for (const f of st.frames) assert.equal(f.key, characterFrameKey(f.src));
  }
  assert.equal(characterFrameKey("x.webp"), "x.webp");
  // 画面（CharacterLayer.tsx）が、この key をそのまま <img key> に使っていること（状態を足していないこと）
  const layer = readFileSync(fileURLToPath(new URL("../src/components/CharacterLayer.tsx", import.meta.url)), "utf8");
  const used = [...layer.matchAll(/\bkey=\{([^}]*)\}/g)].map((m) => m[1].trim());
  assert.deepEqual(used, ["f.key"]);
});

test("コントラストの計算は scripts/theme-contrast.mts と同じ（Light・Dark の全部の組み合わせ）", () => {
  for (const f of [light, dark]) {
    const a = evaluateFile(f);
    const b = evaluate(readTheme(f), {});
    assert.equal(a.length, 132); // 場面3つ × 44 通り（v3.7.0 で輪郭2・お気に入りの星3、v3.8.0 段2-2 でメーターの帯6・最大の線5・出力ボタンの縁1と「!」1、段3 で小窓の文字3を足した）
    assert.deepEqual(
      b.map((r) => [r.scene, r.name, r.ratio.toFixed(4)]),
      a.map((r) => [r.scene, r.name, r.ratio.toFixed(4)]),
    );
  }
  assert.equal(ratio([255, 255, 255], [0, 0, 0]).toFixed(2), "21.00");
});

test("v3.7.0：スキンから変えられる色は 15 個（--c-outline・--c-favorite を足した）。Rust の COLOR_VARS・テーマの CSS の --c-* と同じ名前", () => {
  assert.equal(COLOR_VARS.length, 15);
  const rs = readFileSync(fileURLToPath(new URL("../src-tauri/src/skins/tokens.rs", import.meta.url)), "utf8");
  const m = rs.match(/pub const COLOR_VARS: \[&str; (\d+)\] = \[([^\]]*)\];/);
  assert.ok(m, "Rust COLOR_VARS");
  assert.equal(Number(m![1]), COLOR_VARS.length);
  assert.deepEqual([...m![2].matchAll(/"([^"]+)"/g)].map((x) => x[1]), [...COLOR_VARS]);
  for (const f of [light, dark]) {
    assert.deepEqual(Object.keys(readTheme(f)).filter((k) => k.startsWith("--c-")).sort(), [...COLOR_VARS].sort(), f);
  }
  // 当てる・確かめる
  assert.equal(validColorToken("--c-outline", "rgba(255,255,255,.44)"), "rgba(255, 255, 255, 0.44)");
  assert.equal(validColorToken("--c-favorite", "#FFD23F"), "#ffd23f");
  assert.equal(validColorToken("--c-favorite", "gold"), null);
  const r = skinCssVars({ dark: { tokens: { "--c-outline": "#808080", "--c-favorite": "#ffcc00" }, shadow: null }, light: null, fontFamily: [], tokens: {} }, "dark");
  assert.deepEqual(r, { vars: { "--c-outline": "#808080", "--c-favorite": "#ffcc00" }, rejected: [] });
  assert.ok(SKIN_VARS.includes("--c-outline") && SKIN_VARS.includes("--c-favorite"));
});

test("v3.7.0：輪郭とお気に入りの星を読みにくい色にしたスキンは、その組み合わせだけ警告する", () => {
  const D = readTheme(dark);
  const L = readTheme(light);
  const o = contrastWarnings(D, { "--c-outline": "rgba(255, 255, 255, 0.1)" });
  assert.ok(o.length > 0 && o.every((r) => r.name.includes("--c-outline")), JSON.stringify(o.map((r) => r.name)));
  const f = contrastWarnings(L, { "--c-favorite": "#ffd23f" }); // Dark の黄色を Light に当てる
  assert.ok(f.length > 0 && f.every((r) => r.name.includes("--c-favorite")), JSON.stringify(f.map((r) => r.name)));
  assert.deepEqual(contrastWarnings(D, { "--c-favorite": "#ffd23f" }), []);
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

// ---------------- v3.9.0：立ち絵（スキンから切り離した立ち絵） ----------------

const rustAvatars = readFileSync(fileURLToPath(new URL("../src-tauri/src/skins/avatars.rs", import.meta.url)), "utf8");

test("立ち絵：使う立ち絵の値の形と名前の長さ・数の上限が Rust（skins::avatars）と同じ", () => {
  assert.ok(rustAvatars.includes(`pub const CHOICE_SKIN: &str = "${CHOICE_SKIN}";`));
  assert.ok(rustAvatars.includes(`pub const CHOICE_NONE: &str = "${CHOICE_NONE}";`));
  assert.ok(rustAvatars.includes(`const AVATAR_PREFIX: &str = "avatar:";`));
  assert.ok(rustAvatars.includes(`const SKIN_PREFIX: &str = "skin:";`));
  assert.equal(avatarChoice("0123456789abcdef"), "avatar:0123456789abcdef");
  assert.equal(skinCharacterChoice("quiet-paper"), "skin:quiet-paper");
  assert.ok(rustAvatars.includes(`pub const NAME_MAX: usize = ${AVATAR_NAME_MAX};`));
  assert.ok(rustAvatars.includes("pub const MAX_AVATARS: usize = 30;"));
  // 置き方は、スキンの立ち絵の既定の値（幅 180・高さの上限 420・出す窓の幅 920）
  for (const line of ["pub const WIDTH_PX: u32 = 180;", "pub const MAX_HEIGHT_PX: u32 = 420;", "pub const MIN_WINDOW_WIDTH_PX: u32 = 920;"]) {
    assert.ok(rustAvatars.includes(line), line);
  }
});

test("立ち絵の欄：頭に「スキンの立ち絵」「出さない」、続けて取り込んだスキンの立ち絵、登録した立ち絵", () => {
  const list = {
    skins: [{ skinId: "quiet-paper", name: "Quiet Paper", preview: "s/stopped.png", animated: false }],
    avatars: [
      { id: "0123456789abcdef", name: "Pose", preview: "a/stopped.png", hasStopped: true, animated: true, broken: null },
      { id: "fedcba9876543210", name: "One", preview: "b/playing.png", hasStopped: false, animated: false, broken: null },
      { id: "00000000000000aa", name: "", preview: null, hasStopped: false, animated: false, broken: "avatar_broken" as const },
    ],
    max: 30,
  };
  const rows = avatarRows(list, "Quiet Paper", true);
  assert.deepEqual(
    rows.map((r) => [r.kind, r.choice]),
    [
      ["skin", "skin"],
      ["none", "none"],
      ["skinCharacter", "skin:quiet-paper"],
      ["avatar", "avatar:0123456789abcdef"],
      ["avatar", "avatar:fedcba9876543210"],
      ["avatar", "avatar:00000000000000aa"],
    ],
  );
  assert.equal(rows[0].name, "スキンの立ち絵");
  assert.equal(rows[1].name, "出さない");
  assert.match(rows[0].note, /Quiet Paper/);
  assert.equal(rows[2].preview, "s/stopped.png");
  assert.equal(rows[3].avatarId, "0123456789abcdef");
  // 停止中の絵が無い立ち絵は、そう書く
  assert.match(rows[4].note, /停止中も同じ絵/);
  // 読めない立ち絵：使えない（見本も出さない）。名前を変える・削除のために id は持つ
  assert.equal(rows[5].broken, "avatar_broken");
  assert.equal(rows[5].preview, null);
  assert.equal(rows[5].avatarId, "00000000000000aa");
  // 一覧を読む前・スキンに立ち絵が無いときも、頭の2つは出す
  const empty = avatarRows(null, null, false);
  assert.deepEqual(empty.map((r) => r.choice), ["skin", "none"]);
  assert.match(empty[0].note, /いまは無し/);
});

test("立ち絵：作れる数（30 体まで。読めないものも数える）と名前の長さ（前後の空白を外して 1〜40 文字）", () => {
  const avatars = (n: number) => Array.from({ length: n }, (_, i) => ({ id: String(i).padStart(16, "0"), name: "x", preview: null, hasStopped: false, animated: false, broken: null }));
  assert.equal(canCreateAvatar(null), false);
  assert.equal(canCreateAvatar({ skins: [], avatars: avatars(29), max: 30 }), true);
  assert.equal(canCreateAvatar({ skins: [], avatars: avatars(30), max: 30 }), false);
  assert.equal(avatarNameReady(""), false);
  assert.equal(avatarNameReady("   "), false);
  assert.equal(avatarNameReady(" a "), true);
  assert.equal(avatarNameReady("あ".repeat(40)), true);
  assert.equal(avatarNameReady("あ".repeat(41)), false);
  // 文字の数え方は Rust と同じ（サロゲートペアの絵文字は1文字）
  assert.equal(avatarNameReady("😀".repeat(40)), true);
  assert.equal(avatarNameReady("😀".repeat(41)), false);
});

test("立ち絵：画面が出す絵は Rust が「使う立ち絵」から解いたもの（スキンの character を直接出さない）。停止中の無い立ち絵は1枚だけ読む", () => {
  const shell = readFileSync(fileURLToPath(new URL("../src/layout/AppShell.tsx", import.meta.url)), "utf8");
  assert.ok(shell.includes("useCharacter()"));
  assert.doesNotMatch(shell, /skin\?\.character(?![A-Za-z])/);
  // 「立ち絵を出す」のチェックは無くした（「出さない」は一覧で選び、Rust が character を null にする）
  const view = readFileSync(fileURLToPath(new URL("../src/views/SettingsAvatars.tsx", import.meta.url)), "utf8");
  assert.ok(!view.includes("characterVisible"));
  const skinsView = readFileSync(fileURLToPath(new URL("../src/views/SettingsSkins.tsx", import.meta.url)), "utf8");
  assert.ok(!skinsView.includes("characterVisible") && skinsView.includes("<AvatarsField />"));
  // 登録した立ち絵（停止中なし）は Rust が再生中の絵を両方に入れて渡す → 1枚だけ読み、再生・一時停止で key が変わらない
  const one = { path: "avatars/x/playing.gif", still: "avatars/x/derived/playing.png" };
  const ch = { playing: one, stopped: one, widthPx: 180, maxHeightPx: 420, minWindowWidthPx: 920 };
  const base = { character: ch, visible: true, scale: 1, view: "all", windowWidth: 1200, paneWidth: centerWidth(1200, false), playing: false, reducedMotion: false };
  const a = characterStage(base)!;
  const b = characterStage({ ...base, playing: true })!;
  assert.equal(a.frames.length, 1);
  assert.deepEqual(a.frames.map((f) => f.key), b.frames.map((f) => f.key));
  // 曲一覧の側に MIN_LIST_SIDE_PX 残らなければ隠す
  assert.equal(characterLayout({ ...base, paneWidth: RAIL_MIN_PX + RAIL_PADDING + MIN_LIST_SIDE_PX - 1 }), null);
  assert.ok(characterLayout({ ...base, paneWidth: RAIL_MIN_PX + RAIL_PADDING + MIN_LIST_SIDE_PX }));
  assert.equal(characterStage({ ...base, character: null }), null);
});

test("立ち絵（直し1回目）：「既定の見た目に戻す」の隣に「立ち絵を出さない」。立ち絵は残ることを書く。小窓を閉じたら選んだ画像を捨てる", () => {
  const skinsView = readFileSync(fileURLToPath(new URL("../src/views/SettingsSkins.tsx", import.meta.url)), "utf8");
  const reset = skinsView.indexOf("既定の見た目に戻す\n");
  const hide = skinsView.indexOf("立ち絵を出さない\n");
  assert.ok(reset > 0 && hide > reset, "the hide button sits right after the reset button");
  // 間に別の欄を挟まない（同じ行の中）
  assert.ok(!skinsView.slice(reset, hide).includes('className="field"'));
  assert.ok(skinsView.includes("onClick={() => void hideAvatar()}"));
  // v3.10.0：「既定の見た目に戻す」は手で選んだ色も戻す（立ち絵は残す）
  assert.match(skinsView, /「既定の見た目に戻す」はスキンと手で選んだ色を戻し、立ち絵は残ります/);
  assert.match(skinsView, /同じメニューの「立ち絵を出さない」/);
  const store = readFileSync(fileURLToPath(new URL("../src/stores/skin.ts", import.meta.url)), "utf8");
  assert.ok(store.includes("receive(await api.avatarsHide())"));
  const cmds = readFileSync(fileURLToPath(new URL("../src/ipc/commands.ts", import.meta.url)), "utf8");
  assert.ok(cmds.includes('invoke<SkinState>("avatars_hide")'));
  // Rust の窓口に登録してある
  const lib = readFileSync(fileURLToPath(new URL("../src-tauri/src/lib.rs", import.meta.url)), "utf8");
  assert.ok(lib.includes("commands::avatars::avatars_hide,"));
  // 小窓は、閉じるとき（作った・やめた・画面を離れた）に Rust が持つ選んだ画像を捨てる
  const view = readFileSync(fileURLToPath(new URL("../src/views/SettingsAvatars.tsx", import.meta.url)), "utf8");
  assert.ok(view.includes("useEffect(() => () => void cancelAvatarPick(), [])"));
});

// ---------------- v3.9.0 直し2回目 ----------------

test("直し2回目：大きさの上限 200%（下限 75%）。Rust の CHARACTER_SCALE_MIN・MAX と同じ値", () => {
  assert.equal(SCALE_MIN, 0.75);
  assert.equal(SCALE_MAX, 2);
  assert.equal(clampScale(2.5), 2);
  assert.equal(clampScale(1.6), 1.6);
  assert.equal(clampScale(0.5), 0.75);
  const store = readFileSync(fileURLToPath(new URL("../src-tauri/src/settings/store.rs", import.meta.url)), "utf8");
  assert.ok(store.includes(`pub const CHARACTER_SCALE_MIN: f32 = ${SCALE_MIN};`));
  assert.ok(store.includes(`pub const CHARACTER_SCALE_MAX: f32 = ${SCALE_MAX.toFixed(1)};`));
  // つまみは SCALE_MIN〜SCALE_MAX を 5% 刻み
  const view = readFileSync(fileURLToPath(new URL("../src/views/SettingsAvatars.tsx", import.meta.url)), "utf8");
  assert.ok(view.includes("min={SCALE_MIN}") && view.includes("max={SCALE_MAX}") && view.includes("step={0.05}"));
});


test("直し2回目：立ち絵は高さに合わせて出し、横長の左右は欄で切る（幅で縮めない）。スキンの立ち絵も同じ", () => {
  assert.deepEqual(characterImageStyle(420), { height: "min(420px, 100%)", width: "auto", maxWidth: "none", flex: "none" });
  assert.equal(characterImageStyle(840).height, "min(840px, 100%)");
  const layer = readFileSync(fileURLToPath(new URL("../src/components/CharacterLayer.tsx", import.meta.url)), "utf8");
  assert.ok(layer.includes("style={characterImageStyle(st.maxHeightPx)}"));
  assert.ok(!layer.includes("maxWidth: st.widthPx"));
  // 欄は中央揃え・下揃えで、はみ出しを切る（.character-rail）。画像は縮まない（flex: none）
  const css = readFileSync(fileURLToPath(new URL("../src/theme/base.css", import.meta.url)), "utf8");
  const rail = css.slice(css.indexOf(".character-rail {"), css.indexOf("}", css.indexOf(".character-rail {")));
  for (const d of ["overflow: hidden", "justify-content: center", "align-items: flex-end"]) assert.ok(rail.includes(d), d);
  const img = css.slice(css.indexOf(".character-img {"), css.indexOf("}", css.indexOf(".character-img {")));
  assert.ok(img.includes("flex: none"));
});

test("直し2回目：カード全体を押すと切り替わる。中のボタン（使う・変更・削除）からは二重に走らない。キーボードは Enter・Space", () => {
  const row = { broken: null };
  // クリック
  assert.equal(cardClickSwitches(row, false, false), true);
  assert.equal(cardClickSwitches(row, false, true), false); // 中のボタン：ボタンが処理する
  assert.equal(cardClickSwitches(row, true, false), false); // 使用中
  assert.equal(cardClickSwitches({ broken: "avatar_broken" }, false, false), false);
  // キー
  assert.equal(cardKeySwitches("Enter", row, false, true), true);
  assert.equal(cardKeySwitches(" ", row, false, true), true);
  assert.equal(cardKeySwitches("Enter", row, false, false), false); // 中のボタンでの Enter
  assert.equal(cardKeySwitches("a", row, false, true), false);
  assert.equal(cardKeySwitches("Enter", row, true, true), false);
  assert.equal(cardKeySwitches("Enter", { broken: "avatar_broken" }, false, true), false);
  const view = readFileSync(fileURLToPath(new URL("../src/views/SettingsAvatars.tsx", import.meta.url)), "utf8");
  // カードは押せる部品（キーボードで移れる）で、この2つの決まりを通す
  assert.ok(view.includes('role="button"') && view.includes("tabIndex={pickable ? 0 : -1}"));
  assert.ok(view.includes('cardClickSwitches(r, on, !!(e.target as HTMLElement).closest("button"))'));
  assert.ok(view.includes("cardKeySwitches(e.key, r, on, e.target === e.currentTarget)"));
  // ボタンは「使う」「変更」「削除」（「名前を変える」は「変更」の小窓へまとめた）
  for (const b of ["使う", "変更", "削除"]) assert.match(view, new RegExp(`>\\s*${b}\\s*</button>`), b);
  assert.doesNotMatch(view, />\s*名前を変える\s*<\/button>/);
  // 小窓はカードの外（小窓の中のクリックがカードへ伝わらない）
  const grid = view.indexOf('className="skin-grid"');
  const gridEnd = view.indexOf("立ち絵を足す");
  assert.ok(view.indexOf("<EditDialog") > gridEnd && view.indexOf("<CreateDialog") > gridEnd && grid < gridEnd);
});

test("直し2回目：「＋ 立ち絵を追加…」と小窓「立ち絵を追加」。名前は空でよい（Rust がファイル名から作る）。「変更」の小窓", () => {
  const view = readFileSync(fileURLToPath(new URL("../src/views/SettingsAvatars.tsx", import.meta.url)), "utf8");
  assert.ok(view.includes("＋ 立ち絵を追加…"));
  assert.ok(view.includes('title="立ち絵を追加"') && view.includes('title="立ち絵を変更"'));
  for (const old of ["画像から作る", "アップロード"]) assert.ok(!view.includes(old), old);
  // 名前：空でも押せる・入れたなら 1〜40 文字
  assert.equal(avatarNameOk(""), true);
  assert.equal(avatarNameOk("   "), true);
  assert.equal(avatarNameOk("Pose"), true);
  assert.equal(avatarNameOk("あ".repeat(41)), false);
  // 「変更」：差し替え（追加と同じ pick）・停止中を外す・名前。保存は avatars_update
  for (const t of ["差し替える…", "外す", "元に戻す", "保存"]) assert.ok(view.includes(t), t);
  assert.ok(view.includes("updateAvatar(a.id, name.trim(), playing?.token ?? null, stopped?.token ?? null, removeStopped && !stopped)"));
  const cmds = readFileSync(fileURLToPath(new URL("../src/ipc/commands.ts", import.meta.url)), "utf8");
  assert.ok(cmds.includes('invoke<AvatarSummary>("avatars_update", { id, name, playing, stopped, removeStopped })'));
  assert.ok(!cmds.includes("avatars_rename"));
  const lib = readFileSync(fileURLToPath(new URL("../src-tauri/src/lib.rs", import.meta.url)), "utf8");
  assert.ok(lib.includes("commands::avatars::avatars_update,") && !lib.includes("avatars_rename"));
  // 追加と変更は同じ選び方（閉じたら選んだ画像を捨てる）
  assert.ok(view.includes("function usePick()") && (view.match(/usePick\(\)/g) ?? []).length === 3);
  // 名前の既定の決まりは Rust（name_from_file）。「立ち絵」が最後の既定
  assert.ok(rustAvatars.includes('pub const DEFAULT_NAME: &str = "立ち絵";'));
});

// ---------------- v3.9.0 直し3回目 ----------------


test("直し3回目：古い呼び名と上限が残っていない（画面・型）", () => {
  const skinsView = readFileSync(fileURLToPath(new URL("../src/views/SettingsSkins.tsx", import.meta.url)), "utf8");
  assert.ok(!skinsView.includes("画像から作る"));
  assert.ok(skinsView.includes("「＋ 立ち絵を追加…」"));
  const types = readFileSync(fileURLToPath(new URL("../src/ipc/types.ts", import.meta.url)), "utf8");
  assert.ok(!types.includes("/** 0.75〜1.25 */"));
  assert.ok(types.includes("0.75〜2"));
  for (const f of ["../src/views/SettingsAvatars.tsx", "../src/stores/skin.ts", "../src/skins/avatars.ts", "../src/messages/ja.ts"]) {
    assert.ok(!readFileSync(fileURLToPath(new URL(f, import.meta.url)), "utf8").includes("画像から作る"), f);
  }
});

// ---------------- v3.9.0 直し4回目：専用の幅は絵に左右されない ----------------


test("直し4回目：欄の幅を決める CSS は固定値だけ（中身の幅に頼らない）。曲一覧の側は同じ値で空ける", () => {
  const css = readFileSync(fileURLToPath(new URL("../src/theme/base.css", import.meta.url)), "utf8");
  const block = (sel: string) => {
    const i = css.indexOf(`${sel} {`);
    assert.ok(i >= 0, sel);
    return css.slice(i, css.indexOf("}", i));
  };
  const rail = block(".character-rail");
  // 流れの外（周りの大きさに効かない）・幅は3つとも同じ値・中身で大きさが変わらない（contain: strict）・はみ出しは切る
  for (const d of [
    "position: absolute",
    "width: var(--character-rail-w)",
    "min-width: var(--character-rail-w)",
    "max-width: var(--character-rail-w)",
    "contain: strict",
    "overflow: hidden",
    "justify-content: center",
    "align-items: flex-end",
  ]) {
    assert.ok(rail.includes(d), d);
  }
  // 幅を中身で決める書き方をしていない
  for (const bad of ["min-width: auto", "fit-content", "max-content", "min-content", "width: auto"]) assert.ok(!rail.includes(bad), bad);
  // 曲一覧などは同じ値で右を空ける（中身の幅は見ない）
  assert.ok(block(".main.has-character-rail > :not(.topbar):not(.character-rail)").includes("margin-right: var(--character-rail-w)"));
  // 中央の画面の列は、格子で中身に押し広げられない（minmax(0, 1fr)・min-width: 0）
  assert.ok(block(".app").includes("grid-template-columns: var(--sidebar-w) minmax(0, 1fr)"));
  const main = block(".main");
  assert.ok(main.includes("min-width: 0") && main.includes("overflow: hidden"));
  // AppShell が入れる値は characterLayout の railPx だけ（絵の大きさは渡していない）
  const shell = readFileSync(fileURLToPath(new URL("../src/layout/AppShell.tsx", import.meta.url)), "utf8");
  assert.ok(shell.includes('["--character-rail-w" as any]: `${character.railPx}px`'));
  assert.equal((shell.match(/--character-rail-w/g) ?? []).length, 1);
});

// ---------------- v3.9.0 直し5〜8回目：右の専用の幅（2026-11-19 作者の決定。値は暫定） ----------------
// 直し5回目：窓の幅で決める（立ち絵ごとの幅を使わない）。直し6回目：詳細でも出す（残す幅 360）。
// 直し7回目：曲一覧の列は引いた後の幅で決める。直し8回目：足りないときは隠す前にまず縮める

const SKIN_WIDTHS = [96, 140, 180, 220, 280]; // スキンの widthPx の範囲（96〜280）と、追加した立ち絵の 180
const imgOf = (tag: string) => ({ path: `${tag}.png`, still: null });
const chOf = (widthPx: number, minWindowWidthPx = 920, maxHeightPx = 420) => ({ playing: imgOf(`p${widthPx}`), stopped: imgOf(`s${widthPx}`), widthPx, maxHeightPx, minWindowWidthPx });
const PAD = RAIL_PADDING;

/** 詳細の配置（AppShell と同じ形。行の高さ・再生バーの高さは、曲一覧の幅で変わる値の代わりに決まった値を渡す） */
const detailOf = (winW: number, winH: number, slots: number, collapsed: boolean, lyricsRow: boolean, rowH = 60, playerH = 80) => {
  const inp: DetailInput = { winW, winH, sidebarCollapsed: collapsed || winW < APP.sidebarAutoBelow, slots, lyricsRow, rowHeight: () => rowH, playerH };
  return detailLayout(inp);
};

/** 表示の形ごとの「窓の幅 → 曲一覧の側（引く前）」と残す幅（AppShell・useDetailLayout と同じ） */
const shapes = () => {
  const out: { key: string; paneFor: (w: number) => number; minList: number; detail: boolean; winH: number; slots: number; collapsed: boolean }[] = [];
  for (const collapsed of [false, true]) {
    out.push({ key: `normal collapsed=${collapsed}`, paneFor: (w) => centerWidth(w, collapsed), minList: MIN_LIST_SIDE_PX, detail: false, winH: 900, slots: 0, collapsed });
    for (const winH of [480, 760, 1080, 1440]) {
      for (const slots of [0, 1, 2, 3]) {
        out.push({ key: `detail H=${winH} slots=${slots} collapsed=${collapsed}`, paneFor: (w) => detailOf(w, winH, slots, collapsed, true).listW, minList: DETAIL.listMin, detail: true, winH, slots, collapsed });
      }
    }
  }
  return out;
};

test("直し5〜8回目：定数（18%・140〜480px・暫定）と残す幅（ふだん 634＝3.8.1 の窓 920 で残っていた幅、詳細 360＝DETAIL.listMin。character.ts に写さない）", () => {
  assert.equal(RAIL_WINDOW_RATIO, 0.18);
  assert.equal(RAIL_MIN_PX, 140);
  assert.equal(RAIL_MAX_PX, 480);
  assert.equal(PAD, 12);
  assert.equal(MIN_LIST_SIDE_PX, centerWidth(920, true) - (180 + 12));
  assert.equal(MIN_LIST_SIDE_PX, 634);
  assert.equal(DETAIL.listMin, 360);
  const src = readFileSync(fileURLToPath(new URL("../src/skins/character.ts", import.meta.url)), "utf8");
  assert.ok(src.includes("2026-11-19 作者の決定。値は暫定"));
  assert.doesNotMatch(src.replace(/\/\/.*$|\/\*[\s\S]*?\*\//gm, ""), /\b360\b/);
  const hook = readFileSync(fileURLToPath(new URL("../src/layout/useDetailLayout.ts", import.meta.url)), "utf8");
  assert.ok(hook.includes("return D ? DETAIL.listMin : MIN_LIST_SIDE_PX;"));
  assert.ok(hook.includes("return D ? D.listW : centerWidth(winW, collapsed);"));
  const shell = readFileSync(fileURLToPath(new URL("../src/layout/AppShell.tsx", import.meta.url)), "utf8");
  assert.ok(shell.includes("windowWidth: size.width,"));
  assert.ok(shell.includes("paneWidth: characterPaneOf(size.width, D, collapsed),"));
  assert.ok(shell.includes("minListPx: characterMinListOf(D),"));
});

test("直し8回目：専用の幅＝clamp(140, min(窓 × 18% × 大きさ, 曲一覧の側 − 残す幅 − 12), 480)。140 まで縮めても残す幅を割るときだけ隠す", () => {
  for (const minList of [MIN_LIST_SIDE_PX, DETAIL.listMin]) {
    for (let pane = 300; pane <= 2600; pane += 7) {
      for (let w = 480; w <= 2560; w += 53) {
        for (let pct = 75; pct <= 200; pct += 5) {
          const sc = pct / 100;
          const r = railWidthFor(w, sc, pane, minList);
          const ideal = Math.round(Math.min(480, Math.max(140, w * 0.18 * sc)));
          assert.equal(idealRailWidth(w, sc), ideal);
          if (pane - (140 + PAD) < minList) {
            assert.equal(r, null);
            assert.equal(characterFits(pane, minList), false);
            assert.equal(characterFitState(w, sc, pane, minList), "hidden");
          } else {
            assert.equal(r, Math.min(ideal, Math.floor(pane - minList - PAD)));
            assert.ok(r! >= 140 && r! <= 480);
            // 出すときは残す幅が守られる
            assert.ok(pane - (r! + PAD) >= minList);
            assert.equal(characterFits(pane, minList), true);
            assert.equal(characterFitState(w, sc, pane, minList), r! < ideal ? "shrunk" : "full");
          }
        }
      }
    }
  }
});

test("直し8回目：(1) 窓を1pxずつ広げたとき、ふだんの表示では一度出たら隠れない。詳細で隠れ直すのは、曲一覧の側そのものが狭くなる所（詳細の配置・サイドバー・外の余白）だけ", () => {
  const found = new Set<string>();
  for (const sh of shapes()) {
    let seen = false;
    let start = 0;
    let maxPaneSoFar = 0;
    for (let w = 480; w <= 2560; w++) {
      const pane = sh.paneFor(w);
      const shown = characterFits(pane, sh.minList);
      if (shown) {
        if (start) found.add(`${sh.detail ? "detail" : "normal"} collapsed=${sh.collapsed} ${start}-${w - 1}`);
        start = 0;
        seen = true;
      } else if (seen && !start) start = w;
      if (seen && !shown) {
        // 隠れ直すのは、それより狭い窓のときより曲一覧の側が狭くなったときだけ（立ち絵の決め方が谷を作っていない）
        assert.ok(pane < maxPaneSoFar, `${sh.key} ${w}: pane ${pane} max ${maxPaneSoFar}`);
      }
      // 大きさに関係しない（出す・隠すは引く前の幅だけで決まる）
      for (const sc of [0.75, 1, 2]) assert.equal(characterFitState(w, sc, pane, sh.minList) !== "hidden", shown);
      maxPaneSoFar = Math.max(maxPaneSoFar, pane);
    }
    if (start) found.add(`${sh.detail ? "detail" : "normal"} collapsed=${sh.collapsed} ${start}-2560`);
  }
  // ふだんの表示：谷は無い。詳細：谷はこの3つだけ（どれも曲一覧の側が狭くなる所）
  //   600〜605px（6px）：外の余白と間が 6px → 10px に増え、曲一覧の側（ミキサーは下）が 512px を切る
  //   844〜995px（152px）：窓が横長のとき、詳細がミキサーを横に並べ、曲一覧の側が 360px から始まる（140 + 12 を足せない）
  //   1100〜1167px（68px）：サイドバーを開く設定で、窓 1100px でサイドバーが開き、曲一覧の側が 172px 狭くなる
  assert.deepEqual([...found].sort(), [
    "detail collapsed=false 1100-1167",
    "detail collapsed=false 600-605",
    "detail collapsed=false 844-995",
    "detail collapsed=true 600-605",
    "detail collapsed=true 844-995",
  ]);
});

test("直し8回目：前に隠れていた形（窓 約1220px・サイドバーを開いた詳細／約1060px・たたんだ詳細）と、窓 1400px・100%・詳細の左右で出る", () => {
  for (const [w, collapsed] of [[1220, false], [1060, true], [1400, false]] as const) {
    for (const slots of [0, 1, 2, 3]) {
      const D = detailOf(w, 800, slots, collapsed, true);
      assert.equal(D.side, true, `${w}`);
      const l = characterLayout({ character: chOf(220), visible: true, scale: 1, view: "all", windowWidth: w, paneWidth: D.listW, minListPx: DETAIL.listMin, playing: false, reducedMotion: false });
      assert.ok(l, `${w} slots=${slots}`);
      assert.ok(D.listW - l!.railPx >= DETAIL.listMin);
    }
  }
  // 1220px・開いた詳細：直し7回目までは隠れていた（窓だけで決めた幅 220 + 12 では 360 残らない）。今は縮めて出す
  const D = detailOf(1220, 800, 2, false, true);
  assert.ok(D.listW - (idealRailWidth(1220, 1) + PAD) < DETAIL.listMin);
  assert.equal(characterFitState(1220, 1, D.listW, DETAIL.listMin), "shrunk");
});

test("直し6・8回目：(2)(3) 詳細の左右・上下 × 窓 480〜2560px × 大きさ 75〜200% × 出力先の数で、出すときは曲一覧に 360px 残り、曲一覧・ミキサー・立ち絵が重ならない", () => {
  let shownSide = 0;
  let shownStack = 0;
  for (let winW = 480; winW <= 2560; winW += 20) {
    for (const winH of [480, 760, 1080, 1440, 2600]) {
      for (const slots of [0, 1, 2, 3]) {
        for (const collapsed of [false, true]) {
          for (const lyricsRow of [false, true]) {
            const D = detailOf(winW, winH, slots, collapsed, lyricsRow);
            const { pad, gap } = winW < APP.narrowBelow ? { pad: APP.padNarrow, gap: APP.gapNarrow } : { pad: APP.pad, gap: APP.gap };
            const sideW = collapsed || winW < APP.sidebarAutoBelow ? APP.sidebarCollapsedW : APP.sidebarW;
            const x0 = pad + sideW + gap; // 中央の画面（.main）の左
            for (let pct = 75; pct <= 200; pct += 25) {
              const sc = pct / 100;
              const l = characterLayout({ character: chOf(220), visible: true, scale: sc, view: "all", windowWidth: winW, paneWidth: D.listW, minListPx: DETAIL.listMin, playing: false, reducedMotion: false });
              const key = `${winW}x${winH} slots=${slots} collapsed=${collapsed} lyrics=${lyricsRow} ${pct}%`;
              assert.equal(!!l, D.listW - (RAIL_MIN_PX + PAD) >= DETAIL.listMin, key);
              if (!l) continue;
              const railL = x0 + D.listW - l.railPx;
              const railR = x0 + D.listW;
              assert.ok(railL - x0 >= DETAIL.listMin, key);
              assert.ok(l.widthPx >= RAIL_MIN_PX && l.widthPx <= RAIL_MAX_PX, key);
              if (D.side) {
                shownSide++;
                const mixerL = x0 + D.listW + gap;
                assert.ok(railR < mixerL, key);
                assert.equal(mixerL + D.mixerW, x0 + D.centerW, key);
              } else {
                shownStack++;
                assert.equal(D.listW, D.centerW, key);
                const mainBottom = pad + D.listH;
                assert.ok(mainBottom < mainBottom + gap && D.mixerH > 0, key);
              }
            }
          }
        }
      }
    }
  }
  assert.ok(shownSide > 100 && shownStack > 100, `${shownSide} ${shownStack}`);
  // ふだんの表示も、出すときは 634px 残る
  for (let w = 480; w <= 2560; w += 3) {
    for (const collapsed of [false, true]) {
      for (const sc of [0.75, 1, 2]) {
        const pane = centerWidth(w, collapsed);
        const l = characterLayout({ character: chOf(180), visible: true, scale: sc, view: "all", windowWidth: w, paneWidth: pane, playing: false, reducedMotion: false });
        if (l) assert.ok(pane - l.railPx >= MIN_LIST_SIDE_PX, `${w}`);
      }
    }
  }
  const css = readFileSync(fileURLToPath(new URL("../src/theme/base.css", import.meta.url)), "utf8");
  const i = css.indexOf(".main.has-character-rail {");
  assert.ok(css.slice(i, css.indexOf("}", i)).includes("position: relative"));
  const shell = readFileSync(fileURLToPath(new URL("../src/layout/AppShell.tsx", import.meta.url)), "utf8");
  assert.ok(shell.indexOf("<CharacterLayer") < shell.indexOf("</main>") && shell.indexOf("</main>") < shell.indexOf("<Mixer"));
});

test("直し5・7・8回目：(4) 同じ窓・同じ大きさなら、どの立ち絵（スキンの幅 96〜280・高さ・窓の下限がばらばら）でも、専用の幅も曲一覧の列も同じ", () => {
  for (const sh of shapes().filter((x) => x.slots <= 1 && (x.winH === 760 || !x.detail))) {
    for (let w = 480; w <= 2560; w += 20) {
      const pane = sh.paneFor(w);
      for (let pct = 75; pct <= 200; pct += 25) {
        const sc = pct / 100;
        const seen = new Set<string>();
        for (const wp of SKIN_WIDTHS) {
          for (const mw of [760, 920, 1200]) {
            const l = characterLayout({ character: chOf(wp, mw, 160 + wp), visible: true, scale: sc, view: "all", windowWidth: w, paneWidth: pane, minListPx: sh.minList, playing: false, reducedMotion: false });
            seen.add(`${l ? `${l.railPx}/${l.widthPx}` : "hidden"} ${JSON.stringify(listLayoutFor(listSideWidth(pane, l)))}`);
          }
        }
        assert.equal(seen.size, 1, `${sh.key} ${w} ${pct}%`);
      }
    }
  }
});

test("直し7・8回目：曲一覧の列は引いた後の幅（.main − 専用の幅）で決め、出す判断と幅は引く前の幅で決める（引きずり合わない）", () => {
  let shown = 0;
  for (const sh of shapes().filter((x) => x.slots === 2 && (x.winH === 760 || !x.detail))) {
    for (let w = 480; w <= 2560; w += 10) {
      const pane = sh.paneFor(w);
      for (const sc of [0.75, 1, 1.5, 2]) {
        const input = { character: chOf(180), visible: true, scale: sc, view: "all", windowWidth: w, paneWidth: pane, minListPx: sh.minList, playing: false, reducedMotion: false };
        const l = characterLayout(input);
        const listW = listSideWidth(pane, l);
        if (l) {
          shown++;
          assert.equal(listW, pane - l.railPx);
          assert.ok(listW >= sh.minList);
          const lay = listLayoutFor(listW);
          assert.equal(lay.hideGenre, listW < LIST_BELOW.genre);
          assert.equal(lay.hideAlbum, listW < LIST_BELOW.album);
          assert.equal(lay.rowH, listW < LIST_BELOW.row ? 48 : 60);
        } else assert.equal(listW, pane);
        // 引いた後の幅は、出す判断・幅の入力に入らない（入力は引く前の幅だけ。同じ入力なら同じ答え）
        assert.deepEqual(characterLayout(input), l);
      }
    }
  }
  assert.ok(shown > 1000);
  // 画面で確かめた形（窓 1600px・詳細の左右・150%・100%）：アルバムとジャンルを隠す
  const D = detailOf(1600, 1000, 2, false, true);
  for (const sc of [1.5, 1]) {
    const l = characterLayout({ character: chOf(220), visible: true, scale: sc, view: "all", windowWidth: 1600, paneWidth: D.listW, minListPx: DETAIL.listMin, playing: false, reducedMotion: false })!;
    const listW = listSideWidth(D.listW, l);
    assert.ok(listW >= DETAIL.listMin && listW < LIST_BELOW.album, `${sc}: ${listW}`);
    assert.equal(listLayoutFor(listW).hideGenre, true);
  }
  const shell = readFileSync(fileURLToPath(new URL("../src/layout/AppShell.tsx", import.meta.url)), "utf8");
  const input = shell.slice(shell.indexOf("const characterInput = {"), shell.indexOf("const character = characterLayout(characterInput);"));
  assert.ok(input.length > 0 && !input.includes("listPaneWidth") && !input.includes("mainPaneWidth"));
  assert.ok(shell.indexOf("const character = characterLayout(characterInput);") < shell.indexOf("listPaneWidth.set(listSideWidth(w, railRef.current))"));
  assert.ok(shell.includes("mainPaneWidth.set(w);") && shell.includes("}, [railPx]);"));
  const top = readFileSync(fileURLToPath(new URL("../src/layout/TopBar.tsx", import.meta.url)), "utf8");
  assert.ok(top.includes("const narrow = listLayoutFor(mainW).topbarNarrow;"));
  assert.ok(top.includes("<ViewModeSwitch width={listW} labels={viewModeLabelsFit(mainW, narrow)} />"));
  const sizeSrc = readFileSync(fileURLToPath(new URL("../src/components/useLayoutSize.ts", import.meta.url)), "utf8");
  assert.ok(sizeSrc.includes("return listLayoutFor(useStore(listPaneWidth));"));
});

test("直し8回目：(5) 大きさのつまみの下の案内は、実際の出し方（いちばん大きく・縮めて・隠す）と食い違わない", () => {
  for (const sh of shapes().filter((x) => x.slots === 1 || !x.detail)) {
    for (const pct of [75, 100, 150, 200]) {
      const sc = pct / 100;
      const th = characterThresholds(sc, sh.paneFor, sh.minList);
      assert.ok(th.showFrom !== null && th.fullFrom !== null, sh.key);
      assert.ok(th.fullFrom! >= th.showFrom!);
      for (let w = 480; w <= 2560; w += 3) {
        const pane = sh.paneFor(w);
        const st = characterFitState(w, sc, pane, sh.minList);
        if (w >= th.fullFrom!) assert.equal(st, "full", `${sh.key} ${pct}% ${w}`);
        if (w >= th.showFrom!) assert.notEqual(st, "hidden", `${sh.key} ${pct}% ${w}`);
        if (th.fullOnlyAbove && w < th.fullFrom!) assert.notEqual(st, "full", `${sh.key} ${pct}% ${w}`);
        if (th.hiddenBelow && w < th.showFrom!) assert.equal(st, "hidden", `${sh.key} ${pct}% ${w}`);
        if (w % 51 !== 0) continue;
        const n = characterWindowNote(true, sc, w, pane, sh.paneFor, sh.minList, sh.detail)!;
        const l = characterLayout({ character: chOf(96), visible: true, scale: sc, view: "all", windowWidth: w, paneWidth: pane, minListPx: sh.minList, playing: false, reducedMotion: false });
        assert.equal(n.fits, !!l);
        assert.equal(n.state, st);
        assert.ok(n.text.startsWith(sh.detail ? "詳細の表示で、" : "この大きさ"), n.text);
        assert.ok(n.text.includes(`窓の幅がおよそ ${th.fullFrom}px 以上でいちばん大きく出ます。`), n.text);
        if (th.fullFrom! > th.showFrom!) {
          assert.ok(n.text.includes(th.fullOnlyAbove ? "それより狭いと縮めて出し、" : "それより狭いと縮めることがあり、"));
          assert.ok(n.text.includes(th.hiddenBelow ? `およそ ${th.showFrom}px より狭いと隠します。` : `およそ ${th.showFrom}px より狭いと隠すことがあります。`));
        } else {
          assert.ok(n.text.includes(th.hiddenBelow ? "それより狭いと隠します。" : "それより狭いと隠すことがあります。"));
        }
        assert.ok(n.text.includes({ full: "いちばん大きく出ています。", shrunk: "縮めて出しています。", hidden: "立ち絵を隠しています。" }[st]), n.text);
      }
    }
  }
  // 例：ふだんの表示・サイドバーを開く設定・100%
  const open = (w: number) => centerWidth(w, false);
  const th = characterThresholds(1, open, MIN_LIST_SIDE_PX);
  assert.deepEqual([th.showFrom, th.hiddenBelow], [880, true]);
  // 出す立ち絵が無いときは案内を出さない
  assert.equal(characterWindowNote(false, 1, 1200, open(1200), open), null);
  // 画面：AppShell と同じ詳細の配置・残す幅で計算する。隠しているときは【隠しています】を付ける
  const view = readFileSync(fileURLToPath(new URL("../src/views/SettingsAvatars.tsx", import.meta.url)), "utf8");
  assert.ok(view.includes("characterWindowNote(!!st?.character, s.skin?.characterScale ?? 1, size.width, pane, paneFor, characterMinListOf(D), !!D)"));
  assert.ok(view.includes("useDetailLayout(size, s, s.ui.sidebarCollapsed || size.sidebarAuto, lyricsRowOn(s))"));
  assert.ok(view.includes("useCharacterPaneFor(size, s, lyricsRowOn(s))"));
  assert.ok(view.indexOf("<ScaleSlider />") < view.indexOf("<WindowNote />"));
  assert.ok(view.includes("【隠しています】"));
});

// ---------------- v3.10.0 手で選ぶ色 ----------------

const read = (rel: string) => readFileSync(fileURLToPath(new URL(rel, import.meta.url)), "utf8");
const skinWith = (dark: Record<string, string> | null, light: Record<string, string> | null) => ({
  dark: dark ? { tokens: dark, shadow: null } : null,
  light: light ? { tokens: light, shadow: null } : null,
  fontFamily: [],
  tokens: {},
});

test("v3.10.0：重なり順は テーマの既定 < スキンのそのテーマの色 < 手の色。手の色は Dark・Light 共通。手の色が無ければ 3.9.1 と同じ", () => {
  const skin = skinWith({ "--c-primary": "#111111", "--c-accent": "#222222" }, { "--c-bg": "#eeeeee" });
  const manual = { "--c-accent": "#333333", "--c-ok": "#00AA00" };
  // Dark：スキンの文字はそのまま、アクセントは手の色が勝つ
  const d = layeredCssVars(skin, "dark", manual);
  assert.deepEqual(d.vars, { "--c-primary": "#111111", "--c-accent": "#333333", "--c-ok": "#00aa00" });
  assert.deepEqual([d.rejected, d.manualRejected], [[], []]);
  // Light：スキンの Light の色の上に、同じ手の色
  const l = layeredCssVars(skin, "light", manual);
  assert.deepEqual(l.vars, { "--c-bg": "#eeeeee", "--c-accent": "#333333", "--c-ok": "#00aa00" });
  // 既定のスキン（スキン無し）でも手の色だけ当たる
  assert.deepEqual(layeredCssVars(null, "light", manual).vars, { "--c-accent": "#333333", "--c-ok": "#00aa00" });
  // 手の色が無い（3.9.1 から上げた人・全部戻したあと）：スキンだけのときと同じ。既定のスキンなら何も当てない
  for (const none of [{}, undefined, null]) {
    assert.deepEqual(layeredCssVars(skin, "dark", none), { ...skinCssVars(skin, "dark"), manualRejected: [] });
    assert.deepEqual(layeredCssVars(null, "dark", none).vars, {});
  }
  // 当て方：外してから当てる（手の色を外すと、スキンの値か既定に戻る）
  const set = new Map<string, string>();
  const style = { setProperty: (n: string, v: string) => void set.set(n, v), removeProperty: (n: string) => set.delete(n) };
  applyCssVars(style, d.vars);
  assert.equal(set.get("--c-accent"), "#333333");
  applyCssVars(style, layeredCssVars(skin, "dark", {}).vars);
  assert.equal(set.get("--c-accent"), "#222222");
  assert.equal(set.has("--c-ok"), false);
});

test("v3.10.0：手の色は 15 個の名前とスキンの色と同じ値の形だけ（当てる前にもう一度確かめる）。合わない色だけ当てずに名前を返す", () => {
  const r = manualColorVars({
    "--c-bg": "#ABC",
    "--c-panel-rgb": "1,2,3",
    "--c-hover": "rgba(0,0,0,.5)",
    "--c-primary": "red",
    "--c-secondary": "#000; background: url(https://example.invalid/x)",
    "--c-accent": "var(--c-bg)",
    "--row-h": "#000000",
    "--ui-alpha": "#000000",
    "--c-overlay-rgb": "#000000",
    "--c-ok": 5,
  });
  assert.deepEqual(r.vars, { "--c-bg": "#aabbcc", "--c-panel-rgb": "1, 2, 3", "--c-hover": "rgba(0, 0, 0, 0.5)" });
  assert.deepEqual(r.rejected.sort(), ["--c-accent", "--c-ok", "--c-overlay-rgb", "--c-primary", "--c-secondary", "--row-h", "--ui-alpha"].sort());
  for (const bad of [null, undefined, "x", 5, ["--c-bg", "#000000"]]) assert.deepEqual(manualColorVars(bad), { vars: {}, rejected: [] }, String(bad));
  // スキンの値が壊れていれば rejected（スキンを既定に戻す）、手の色が壊れていれば manualRejected（その色だけ当てない）。別々に返す
  const x = layeredCssVars(skinWith({ "--c-bg": "nope" }, null), "dark", { "--c-ok": "nope", "--c-danger": "#ff0000" });
  assert.deepEqual([x.rejected, x.manualRejected, x.vars], [["--c-bg"], ["--c-ok"], { "--c-danger": "#ff0000" }]);
});

test("v3.10.0：色の欄の 15 行は COLOR_VARS と同じ並び（新しい表を作らない）。半透明の5色は色＋濃さ、*-rgb の2色は色だけ", () => {
  assert.deepEqual(COLOR_ROWS.map((r) => r.name), [...COLOR_VARS]);
  assert.deepEqual(Object.keys(COLOR_LABELS).sort(), [...COLOR_VARS].sort());
  assert.ok(COLOR_ROWS.every((r) => r.label.length > 0 && !r.label.includes("--")));
  assert.deepEqual(COLOR_ROWS.filter((r) => r.kind === "alpha").map((r) => r.name), ["--c-hover", "--c-selected", "--c-border", "--c-outline", "--c-input"]);
  assert.deepEqual([...ALPHA_VARS], ["--c-hover", "--c-selected", "--c-border", "--c-outline", "--c-input"]);
  assert.deepEqual(COLOR_ROWS.filter((r) => r.kind === "rgb").map((r) => r.name), ["--c-panel-rgb", "--c-overlay-rgb"]);
  assert.equal(COLOR_ROWS.filter((r) => r.kind === "solid").length, 8);
  // 半透明の5色は、どちらのテーマでも既定が rgba（濃さのつまみの初期値になる）
  for (const f of [light, dark]) {
    const t = readTheme(f);
    for (const n of ALPHA_VARS) assert.match(t[n], /^rgba\(/, `${f} ${n}`);
  }
  // Rust の手の色の確かめは、スキンの色と同じ表と同じ確かめ（新しい表を作っていない）
  const rs = read("../src-tauri/src/skins/colors.rs");
  assert.ok(rs.includes("use super::tokens::{color_token, COLOR_VARS};"));
  assert.ok(!/const [A-Z_]+: \[&str; 15\]/.test(rs));
});

test("v3.10.0：選び窓の値（#rrggbb）と濃さから保存する値を作る。どの行の値も、その色の確かめに通る", () => {
  assert.deepEqual(splitColor("#AABBCC"), { hex: "#aabbcc", alpha: 1 });
  assert.deepEqual(splitColor("rgba(255, 255, 255, 0.07)"), { hex: "#ffffff", alpha: 0.07 });
  assert.deepEqual(splitColor("15, 15, 22"), { hex: "#0f0f16", alpha: 1 });
  assert.deepEqual(splitColor("rgb(1, 2, 3)"), { hex: "#010203", alpha: 1 });
  assert.equal(splitColor("red"), null);
  assert.equal(pickedColor("solid", "#ABCDEF"), "#abcdef");
  assert.equal(pickedColor("rgb", "#0f0f16"), "15, 15, 22");
  assert.equal(pickedColor("alpha", "#7c6cff", 0.22), "rgba(124, 108, 255, 0.22)");
  // 濃さは小数3桁まで（色だけ選んだとき、Light の既定 0.045 などの濃さを変えない）
  assert.equal(pickedColor("alpha", "#000000", 0.045), "rgba(0, 0, 0, 0.045)");
  assert.equal(pickedColor("alpha", "#000000", 0.12345), "rgba(0, 0, 0, 0.123)");
  assert.equal(pickedColor("alpha", "#000000", 2), "rgba(0, 0, 0, 1)");
  assert.equal(pickedColor("alpha", "#000000", -1), "rgba(0, 0, 0, 0)");
  assert.equal(pickedColor("alpha", "#000000", NaN), "rgba(0, 0, 0, 1)");
  // 選び窓の値は #rrggbb。rgb() は色の確かめと同じく #rrggbb にそろえて受け、濃さのある rgba() は選び窓の値として受けない
  assert.equal(pickedColor("solid", "rgb(1,2,3)"), "#010203");
  assert.equal(pickedColor("solid", "rgba(1,2,3,0.5)"), null);
  assert.equal(pickedColor("solid", "javascript:x"), null);
  // 全部の行・どちらのテーマの既定からでも：分けて作り直した値が、その名前の確かめに通り、濃さが残る
  for (const f of [light, dark]) {
    const t = readTheme(f);
    for (const r of COLOR_ROWS) {
      const p = splitColor(t[r.name]);
      assert.ok(p, `${f} ${r.name}`);
      const v = pickedColor(r.kind, p!.hex, p!.alpha)!;
      assert.equal(validColorToken(r.name, v), v, `${r.name} ${v}`);
      assert.equal(colorKind(r.name), r.kind);
      if (r.kind === "alpha") assert.equal(splitColor(v)!.alpha, p!.alpha, r.name);
    }
  }
});

test("v3.10.0：今の色の見本と初期値は、手の色 → スキンのそのテーマの色 → テーマの既定 の順で決まる", () => {
  const D = readTheme(dark);
  const skin = skinThemeColors(skinWith({ "--c-accent": "#123456", "--c-bg": "bad" }, null), "dark");
  assert.deepEqual(skin, { "--c-accent": "#123456" });
  assert.deepEqual(skinThemeColors(skinWith({ "--c-accent": "#123456" }, null), "light"), {});
  const c = currentColors(D, skin, { "--c-ok": "#00ff00", "--c-accent": "#654321", "--c-bg": "nope" });
  assert.deepEqual(c["--c-ok"], { value: "#00ff00", source: "manual" });
  assert.deepEqual(c["--c-accent"], { value: "#654321", source: "manual" });
  assert.deepEqual(c["--c-bg"], { value: D["--c-bg"], source: "theme" });
  assert.deepEqual(currentColors(D, skin, {})["--c-accent"], { value: "#123456", source: "skin" });
  assert.equal(Object.keys(c).length, 15);
});

test("v3.10.0：読みやすさの警告は今のテーマで、手の色のせいで新しく届かなくなった組み合わせだけ（基準は既定＋スキンの色）", () => {
  const D = readTheme(dark);
  const L = readTheme(light);
  // 欄が渡すもの：基準＝既定＋スキンの色、重ねる色＝確かめた手の色（SettingsColors.tsx と同じ組み立て）
  const warn = (base: Record<string, string>, skin: Record<string, string>, manual: Record<string, string>) =>
    contrastWarnings({ ...base, ...skin }, manualColorVars(manual).vars);
  assert.deepEqual(warn(D, {}, {}), []);
  // 1組だけなので、Dark で文字を白にしたまま Light に切り替えると、Light で警告が出る（Dark では出ない）
  const white = { "--c-primary": "#ffffff" };
  assert.deepEqual(warn(D, {}, white), []);
  const w = warn(L, {}, white);
  assert.ok(w.length > 0 && w.every((r) => r.name.includes("--c-primary") || r.name.includes("--c-bg")), JSON.stringify(w.map((r) => r.name)));
  // スキンがもう読みにくくしている色を、手でも同じ色にしただけなら、手の色のせいにしない
  const skinBad = { "--c-secondary": "#bbbbbb" };
  assert.ok(contrastWarnings(L, skinBad).length > 0);
  assert.deepEqual(warn(L, skinBad, { "--c-secondary": "#bbbbbb" }), []);
  // 確かめに通らない手の色は計算にも入れない
  assert.deepEqual(warn(L, {}, { "--c-primary": "white" }), []);
  // 画面に出す名前は、変数の名前を色の名前に置き換える
  assert.equal(warningLabel("--c-bg／--c-primary（再生ボタン）"), "「背景」／「文字」（再生ボタン）");
  assert.equal(warningLabel("線 --c-outline（操作部の輪郭）／パネル"), "線 「操作部の輪郭」（操作部の輪郭）／パネル");
  const view = read("../src/views/SettingsColors.tsx");
  assert.ok(view.includes("contrastWarningsOrError({ ...defaults, ...skinColors }, manualColorVars(manual).vars)"));
});

test("v3.10.0：色の欄の作り（選び窓は <input type=\"color\">・保存は選び窓を閉じたとき・行の「戻す」・全部戻すは確認あり・セーフモード）", () => {
  const view = read("../src/views/SettingsColors.tsx");
  assert.ok(view.includes('type="color"'));
  // 値を文字で打つ欄は作らない
  assert.ok(!/type="text"|className="input"/.test(view));
  // 動かしている間は当てるだけ、選び窓の本来の change で保存
  assert.ok(view.includes('el.addEventListener("change", onChange)'));
  assert.ok(view.includes("previewColor(props.name, v)"));
  assert.ok(view.includes("void commitColor(props.name, null)"));
  assert.ok(view.includes('disabled={props.disabled || props.source !== "manual"}'));
  assert.ok(view.includes('"手で選んだ色を全部戻す"') && view.includes("confirmDialog("));
  assert.ok(view.includes("セーフモード"));
  // 欄は着せ替えの中、立ち絵の欄の上
  const skinsView = read("../src/views/SettingsSkins.tsx");
  assert.ok(skinsView.indexOf("<ColorsField />") > 0 && skinsView.indexOf("<ColorsField />") < skinsView.indexOf("<AvatarsField />"));
  // 外のライブラリを足していない
  const pkg = JSON.parse(read("../package.json"));
  assert.deepEqual(Object.keys(pkg.dependencies).sort(), ["@dnd-kit/core", "@dnd-kit/sortable", "@tanstack/react-virtual", "@tauri-apps/api", "@tauri-apps/plugin-global-shortcut", "react", "react-dom"]);
});

test("v3.10.0：手の色の保存は Rust の窓口だけ・「既定の見た目に戻す」（設定とトレイ）は手の色も外す・スキンが読めないときの戻しは手の色を残す・セーフモードでは当てない", () => {
  const cmds = read("../src/ipc/commands.ts");
  assert.ok(cmds.includes('invoke<AppSettings>("skins_color_set", { name, value })'));
  assert.ok(cmds.includes('invoke<AppSettings>("skins_colors_clear")'));
  const lib = read("../src-tauri/src/lib.rs");
  assert.ok(lib.includes("commands::skins::skins_color_set,") && lib.includes("commands::skins::skins_colors_clear,"));
  const rsCmd = read("../src-tauri/src/commands/skins.rs");
  assert.match(rsCmd, /pub async fn skins_reset\(app: AppHandle\) -> AppResult<SkinState> \{\r?\n\s+runtime::reset_look\(&app\)/);
  const tray = read("../src-tauri/src/platform/windows/tray.rs");
  assert.ok(tray.includes("crate::skins::runtime::reset_look(&app)")); // 直し2回目：別のスレッドの中で呼ぶ
  // 画面から来た設定の保存では、手の色は変わらない
  const rsSettings = read("../src-tauri/src/commands/settings.rs");
  assert.ok(rsSettings.includes("s.skin.colors = before.skin.colors.clone();"));
  // 画面の当て方：スキンの値が読めないときは、スキンだけを既定にする（手の色を消す skins_reset を呼ばない）
  const store = read("../src/stores/skin.ts");
  assert.ok(store.includes('receive(await api.skinsApply("builtin:default"))'));
  assert.ok(!/rejected\.length > 0\) \{[^}]*resetSkin\(\)/.test(store));
  assert.ok(store.includes("if (skinState.get()?.safeMode) return {};"));
  // 「既定の見た目に戻す」の確認の文と案内
  const skinsView = read("../src/views/SettingsSkins.tsx");
  assert.ok(skinsView.includes("手で選んだ色（${manualCount} 個）も戻ります"));
  assert.ok(skinsView.includes("manualCount === 0"));
});

// ---------------- v3.10.0 直し1回目 ----------------

test("直し1回目：行に「手で選んだ色／スキンの色／既定の色」の札を出さない（手で変えたかは「戻す」が押せるかで分かる）", () => {
  const view = read("../src/views/SettingsColors.tsx");
  assert.ok(!view.includes("SOURCE_TEXT") && !view.includes("color-source"));
  assert.ok(!/"スキンの色"|"既定の色"|"手で選んだ色"/.test(view));
  assert.ok(view.includes('disabled={props.disabled || props.source !== "manual"}'));
  assert.ok(!read("../src/theme/base.css").includes(".color-source"));
});

test("直し1回目：濃さのつまみの刻みは保存する桁（0.1%）と同じ。どの濃さでも、つまみの位置と「濃さ N%」の表示がそろう", () => {
  const view = read("../src/views/SettingsColors.tsx");
  assert.ok(view.includes("const ALPHA_STEP_PCT = 0.1;"));
  assert.ok(view.includes("step={ALPHA_STEP_PCT}") && view.includes("value={pctOf(alpha)}") && view.includes("濃さ {pctOf(alpha)}%"));
  assert.ok(view.includes("const pctOf = (a: number) => Math.round(a * 1000) / 10;"));
  const pctOf = (a: number) => Math.round(a * 1000) / 10;
  // 保存できる濃さ（小数3桁）は全部、0.1% の刻みにちょうど乗る（つまみが別の位置へ丸めない）
  for (let i = 0; i <= 1000; i++) {
    const a = i / 1000;
    const v = pctOf(a);
    assert.ok(Math.abs(v * 10 - Math.round(v * 10)) < 1e-9, String(a));
    assert.equal(splitColor(pickedColor("alpha", "#000000", v / 100)!)!.alpha, a, String(a));
  }
  // 両方のテーマの既定の濃さ（Light の 0.045 など）も同じ
  for (const f of [light, dark]) {
    const t = readTheme(f);
    for (const n of ALPHA_VARS) assert.equal(pctOf(splitColor(t[n])!.alpha) / 100, splitColor(t[n])!.alpha, `${f} ${n}`);
  }
});

test("直し1回目：設定を書く道は全部「読む → 直す → 書く」を1つの鍵で一続きにする（全体を書き戻すだけの古い関数は残っていない）", () => {
  const settingsRs = read("../src-tauri/src/commands/settings.rs");
  const runtimeRs = read("../src-tauri/src/skins/runtime.rs");
  for (const src of [settingsRs, runtimeRs]) assert.ok(!src.includes("store_and_emit"));
  assert.ok(settingsRs.includes("let _w = write.lock()"));
  assert.ok(settingsRs.includes("update_and_emit(&app, &state, |before| Ok(Some(from_screen(settings, before, &bg_dir))))"));
  assert.ok(runtimeRs.includes("crate::commands::settings::update_and_emit(app, &state, |cur| {"));
  // 写しを直接置き換えるのは、鍵の中の2か所（write_locked と設定の zip の読み込み）だけ
  const direct = [settingsRs, runtimeRs].join("\n").match(/\*state\.settings\.lock\(\)|\*current\.lock\(\)/g) ?? [];
  assert.equal(direct.length, 2);
  assert.ok(read("../src-tauri/src/commands/mod.rs").includes("pub settings_write: Mutex<()>,"));
});

// ---------------- v3.10.0 直し2回目 ----------------

test("直し2回目：設定の知らせは最新を読み直して1か所から送る・トレイの「既定の見た目に戻す」は別のスレッドへ渡す（メインのスレッドで書く鍵を待たない）", () => {
  const settingsRs = read("../src-tauri/src/commands/settings.rs");
  // 設定の知らせを送るのは emit_latest の中の1か所だけ（読み直してから送る）
  assert.equal((settingsRs.match(/app\.emit\(ev::SETTINGS/g) ?? []).length, 1);
  assert.ok(settingsRs.includes("emit_latest(&state.settings_emit, &state.settings, |s| {"));
  assert.ok(read("../src-tauri/src/commands/mod.rs").includes("pub settings_emit: Mutex<()>,"));
  // トレイ：reset_look は「立ち絵を出さない」と同じく別のスレッドで
  const tray = read("../src-tauri/src/platform/windows/tray.rs");
  assert.ok(tray.includes('"reset-skin" => {\n                reset_look_off_main(app);'));
  assert.ok(tray.includes('name("tray-reset-look".into()).spawn(move || {\n        if let Err(e) = crate::skins::runtime::reset_look(&app)'));
  assert.equal((tray.match(/runtime::reset_look\(/g) ?? []).length, 1);
});

// ---------------- v3.10.0 直し3回目：ビルドした CSS の色を読む ----------------
// ソースの CSS（dark.css・light.css）だけを見るテストでは、ビルドで CSS を小さくする道具が rgba(…) を #rrggbbaa に書き換えることが
// 見えなかった（画面では半透明の5色が #000000・濃さ 100% と読まれ、読みやすさの警告が黙って0件になっていた）。
// ここでは vite で一時フォルダへビルドし、できた CSS のテーマの色を、画面と同じ読み取りに通す（npm ci 済みであること）

/** vite で一時フォルダへビルドし、できた CSS のテーマごとの --c-* の値を返す（一時フォルダは消す） */
function builtThemeColors(): Record<"dark" | "light", Record<string, string>> {
  const projectRoot = fileURLToPath(new URL("..", import.meta.url));
  const out = mkdtempSync(join(tmpdir(), "plp-css-"));
  try {
    execFileSync(process.execPath, [join(projectRoot, "node_modules", "vite", "bin", "vite.js"), "build", "--outDir", out, "--emptyOutDir", "--logLevel", "error"], {
      cwd: projectRoot,
      stdio: "pipe",
    });
    const assets = join(out, "assets");
    const css = readdirSync(assets).filter((f) => f.endsWith(".css")).map((f) => readFileSync(join(assets, f), "utf8")).join("\n");
    const block = (theme: string) => {
      const m = new RegExp(`\\[data-theme="?${theme}"?\\]\\s*\\{([^}]*)\\}`).exec(css);
      assert.ok(m, `built CSS has [data-theme=${theme}]`);
      const o: Record<string, string> = {};
      for (const d of m![1].matchAll(/(--c-[a-z-]+)\s*:\s*([^;]+)/g)) o[d[1]] = d[2].trim();
      return o;
    };
    return { dark: block("dark"), light: block("light") };
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}

const BUILT = builtThemeColors();

test("直し3回目：ビルドした CSS のテーマの色（半透明の色は #rrggbbaa に書き換わっている）を、画面と同じ読み取りで全部読める", () => {
  for (const [theme, file] of [["dark", dark], ["light", light]] as const) {
    const raw = BUILT[theme];
    assert.deepEqual(Object.keys(raw).sort(), [...COLOR_VARS].sort(), theme);
    const { colors, unreadable } = readThemeColors(raw);
    assert.deepEqual(unreadable, [], `${theme}: ${JSON.stringify(raw)}`);
    const src = readTheme(file);
    for (const r of COLOR_ROWS) {
      // 読んだ値は、手の色・スキンと同じ形（validColorToken に通る）にそろっている
      assert.equal(validColorToken(r.name, colors[r.name]), colors[r.name], `${theme} ${r.name} ${raw[r.name]}`);
      // 選び窓の色と濃さは、ソースの値と同じ（濃さは 8桁の16進の刻み 1/255 の中）
      const a = splitColor(colors[r.name])!;
      const b = splitColor(src[r.name])!;
      assert.ok(a && b, `${theme} ${r.name}`);
      assert.equal(a.hex, b.hex, `${theme} ${r.name}`);
      assert.ok(Math.abs(a.alpha - b.alpha) <= 1 / 255, `${theme} ${r.name} ${a.alpha} ${b.alpha}`);
      // 半透明の5色は、#000000・濃さ 100% と読まれない
      if (r.kind === "alpha") assert.ok(a.alpha < 1, `${theme} ${r.name}`);
      // 読みやすさの計算の読み取りも、書き換わったままの値で投げない
      assert.doesNotThrow(() => contrastColor(raw[r.name]), `${theme} ${r.name}`);
    }
    // 計算の結果も、ソースの値で計算したものとほぼ同じ（濃さの丸めの分だけ。比の差は 1% 未満）
    const e1 = evaluate(colors, {});
    const e2 = evaluate(src, {});
    assert.equal(e1.length, e2.length);
    e1.forEach((x, i) => assert.ok(Math.abs(x.ratio - e2[i].ratio) / e2[i].ratio < 0.01, `${theme} ${x.scene} ${x.name} ${x.ratio} ${e2[i].ratio}`));
    // 注：基準に届くかどうかまでは比べない。Dark の --c-hover・--c-input（0.07）はビルドで 0x12（0.0706）になり、明るい背景画像の上の
    // 薄い文字が 4.512 → 4.498:1 と基準 4.5 をわずかに割る（ビルドの書き換えによるもので、読む側の直しの範囲の外）
  }
});

test("直し3回目：ビルドした CSS の既定を基準にしても、読みやすさの警告が出る（色の欄・スキンの取り込みの確認画面）", () => {
  const L = readThemeColors(BUILT.light).colors;
  const D = readThemeColors(BUILT.dark).colors;
  // 何も変えなければ警告なし（既定で元から届かない組み合わせは出さない）
  assert.deepEqual(contrastWarningsOrError(L, {}), { rows: [], error: null });
  assert.deepEqual(contrastWarningsOrError(D, {}), { rows: [], error: null });
  // 色の欄：Light で文字を #eeeeee にすると警告が出る
  const manual = contrastWarningsOrError(L, manualColorVars({ "--c-primary": "#eeeeee" }).vars);
  assert.equal(manual.error, null);
  assert.ok(manual.rows.length > 0 && manual.rows.every((r) => r.name.includes("--c-primary") || r.name.includes("--c-bg")));
  // スキンの取り込みの確認画面：読みにくい色のスキンは警告が出る（3.9.1 以前は、ビルドした画面ではここが黙って0件だった）
  const skinRows = contrastWarningsOrError(L, { "--c-secondary": "#bbbbbb" });
  assert.equal(skinRows.error, null);
  assert.ok(skinRows.rows.length > 0);
  // 書き換わったままの値を渡しても投げない（計算の読み取りも #rrggbbaa を読む）
  assert.ok(contrastWarningsOrError(BUILT.light, { "--c-secondary": "#bbbbbb" }).rows.length > 0);
  // 読めない値は黙って0件にせず、理由を返す
  const bad = contrastWarningsOrError({ ...L, "--c-hover": "nope" }, { "--c-primary": "#eeeeee" });
  assert.ok(bad.error && bad.rows.length === 0);
});

test("直し3回目：読むだけで、受け付ける形は広げない（手の色・スキンの値に #rrggbbaa は書けない。Rust の確かめも同じ）", () => {
  assert.equal(readCurrentColor("--c-hover", "#ffffff12"), "rgba(255, 255, 255, 0.071)");
  assert.equal(readCurrentColor("--c-hover", "#0000000b"), "rgba(0, 0, 0, 0.043)");
  assert.equal(readCurrentColor("--c-hover", "#fff8"), "rgba(255, 255, 255, 0.533)");
  assert.equal(readCurrentColor("--c-bg", "#123456ff"), "#123456");
  assert.equal(readCurrentColor("--c-bg", "#fff"), "#ffffff");
  for (const [n, v] of [["--c-panel-rgb", "#ffffff12"], ["--c-hover", "#fffff"], ["--c-hover", "red"], ["--row-h", "#ffffff12"], ["--c-hover", 5]] as const) {
    assert.equal(readCurrentColor(n, v), null, `${n} ${v}`);
  }
  assert.equal(validColorToken("--c-hover", "#ffffff12"), null);
  assert.equal(validColor("#fff8"), null);
  assert.deepEqual(manualColorVars({ "--c-hover": "#ffffff12" }).rejected, ["--c-hover"]);
  assert.deepEqual(skinCssVars(skinWith({ "--c-hover": "#ffffff12" }, null), "dark").rejected, ["--c-hover"]);
  const rs = read("../src-tauri/src/skins/tokens.rs");
  assert.ok(rs.includes('"#1234567",'), "Rust still refuses 7+ hex digits");
  // 画面：既定の色は readThemeColors を通し、計算できなかったときは理由を出す（例外を捨てて0件にしない）
  const td = read("../src/skins/themeDefaults.ts");
  assert.ok(td.includes("readThemeColors(raw)"));
  const view = read("../src/views/SettingsColors.tsx");
  assert.ok(!/catch\s*\{\s*return \[\];/.test(view));
  assert.ok(view.includes("読みやすさを計算できませんでした"));
  const review = read("../src/views/SkinImportReview.tsx");
  assert.ok(review.includes("contrastWarningsOrError(d.colors, t.tokens)") && review.includes("読みやすさを計算できませんでした"));
  assert.ok(!/catch\s*\{\s*out\.push\(\{ theme, rows: \[\] \}\);/.test(review));
});
