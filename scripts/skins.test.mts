// 着せ替え（v3.2.0）の画面側の決まりを確かめる：色と文字の値の確かめ・状態ごとのアイコンの選び方・立ち絵を出す／隠す条件・コントラストの計算。
// v3.9.0：立ち絵の欄の並び・使う立ち絵の値（Rust と同じ形）・画面が出す立ち絵は Rust が解いたもの。
// v3.10.0：手で選ぶ色（重なり順・値の確かめ・選び窓の値の作り方・読みやすさの警告・「既定の見た目に戻す」で外れる道）。
// v3.11.0：手で入れるアイコン（重なり順・枠が Rust と同じ・ドロップの振り分け・欄の作り・保存の道・ビルドした CSS での大きさ）。
// v3.11.1：ビルドした CSS の値で計算しても基準に届かない組み合わせがソースと同じこと・設定画面の短い文字を言葉の切れ目で折り返すこと。
// v3.12.0：「シャッフル再生」のボタンの枠（IconShufflePlay）。枠の場所・使うボタン・絵の順（手 → スキン → スキンの IconShuffle の default → 線）。
//          取り込んだスキンの立ち絵のカード（説明にスキンの名前・削除すると消えること・「スキン」の印・切らずに折り返す）。取り込みの数の上限の値が文と同じ。
// v3.12.1：立ち絵のカードの説明を「スキン『◯◯』の立ち絵」だけにし、欄の頭の説明は2文（同じ文を画面に2回出さない）。カードの高さを全部そろえ、ボタンを下の端に。
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
import { pickIconPath, pickLayeredIcon, repeatState, shuffleState, SKIN_FALLBACK, starState, volumeState, withRev } from "../src/skins/icons.ts";
import { wrapParts } from "../src/components/wrapParts.ts";
import { hasManualIcon, ICON_GROUPS, ICON_SLOTS, iconZoneKey, manualIconCount, parseIconZone, routeDrop, SIDEBAR_PX, slotPx } from "../src/skins/iconSlots.ts";
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
import {
  AVATAR_LEAD,
  AVATAR_NAME_MAX,
  avatarCardLabel,
  avatarChoice,
  avatarNameOk,
  avatarNameReady,
  avatarPreviewText,
  avatarRows,
  canCreateAvatar,
  cardClickSwitches,
  cardKeySwitches,
  CHOICE_NONE,
  CHOICE_SKIN,
  SKIN_CHARACTER_BADGE,
  skinCharacterChoice,
  skinCharacterNote,
} from "../src/skins/avatars.ts";
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

test("立ち絵を出す・隠す：スキン・設定・歌詞画面・曲一覧の側の幅。大きさは 75〜200%（v3.9.0。3.8.1 までは 125%）。専用の幅は窓の幅で決める", () => {
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
  // 曲一覧の側が足りないときは、隠す前にまず縮める。140px まで縮めても残す幅を割るときだけ隠す
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
    assert.equal(a.length, 132); // 場面3つ × 44 通り（v3.7.0 で輪郭2・お気に入りの星3、v3.8.0 でメーターの帯6・最大の線5・出力ボタンの縁1と「!」1、で小窓の文字3を足した）
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

test("立ち絵：「既定の見た目に戻す」の隣に「立ち絵を出さない」。立ち絵は残ることを書く。小窓を閉じたら選んだ画像を捨てる", () => {
  const skinsView = readFileSync(fileURLToPath(new URL("../src/views/SettingsSkins.tsx", import.meta.url)), "utf8");
  const reset = skinsView.indexOf("既定の見た目に戻す\n");
  const hide = skinsView.indexOf("立ち絵を出さない\n");
  assert.ok(reset > 0 && hide > reset, "the hide button sits right after the reset button");
  // 間に別の欄を挟まない（同じ行の中）
  assert.ok(!skinsView.slice(reset, hide).includes('className="field"'));
  assert.ok(skinsView.includes("onClick={() => void hideAvatar()}"));
  // v3.10.0：「既定の見た目に戻す」は手で選んだ色も戻す（立ち絵は残す）。v3.11.0 から手で入れたアイコンも
  assert.match(skinsView, /「既定の見た目に戻す」はスキンと手で選んだ色・手で入れたアイコンを戻し、立ち絵は残ります/);
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

// ---------------- v3.9.0 ----------------

test("大きさの上限 200%（下限 75%）。Rust の CHARACTER_SCALE_MIN・MAX と同じ値", () => {
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


test("立ち絵は高さに合わせて出し、横長の左右は欄で切る（幅で縮めない）。スキンの立ち絵も同じ", () => {
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

test("カード全体を押すと切り替わる。中のボタン（使う・変更・削除）からは二重に走らない。キーボードは Enter・Space", () => {
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
  const grid = view.indexOf('className="skin-grid avatar-grid"');
  assert.ok(grid > 0);
  const gridEnd = view.indexOf("立ち絵を足す");
  assert.ok(view.indexOf("<EditDialog") > gridEnd && view.indexOf("<CreateDialog") > gridEnd && grid < gridEnd);
});

test("「＋ 立ち絵を追加…」と小窓「立ち絵を追加」。名前は空でよい（Rust がファイル名から作る）。「変更」の小窓", () => {
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

// ---------------- v3.9.0 ----------------


test("古い呼び名と上限が残っていない（画面・型）", () => {
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

// ---------------- v3.9.0：専用の幅は絵に左右されない ----------------


test("欄の幅を決める CSS は固定値だけ（中身の幅に頼らない）。曲一覧の側は同じ値で空ける", () => {
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

// ---------------- v3.9.0：右の専用の幅（2026-11-19 に決めた。値は暫定） ----------------
// 窓の幅で決める（立ち絵ごとの幅を使わない）。詳細でも出す（残す幅 360）。
// 曲一覧の列は引いた後の幅で決める。足りないときは隠す前にまず縮める

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

test("定数（18%・140〜480px・暫定）と残す幅（ふだん 634＝3.8.1 の窓 920 で残っていた幅、詳細 360＝DETAIL.listMin。character.ts に写さない）", () => {
  assert.equal(RAIL_WINDOW_RATIO, 0.18);
  assert.equal(RAIL_MIN_PX, 140);
  assert.equal(RAIL_MAX_PX, 480);
  assert.equal(PAD, 12);
  assert.equal(MIN_LIST_SIDE_PX, centerWidth(920, true) - (180 + 12));
  assert.equal(MIN_LIST_SIDE_PX, 634);
  assert.equal(DETAIL.listMin, 360);
  const src = readFileSync(fileURLToPath(new URL("../src/skins/character.ts", import.meta.url)), "utf8");
  assert.ok(src.includes("2026-11-19 に決めた。値は暫定"));
  assert.doesNotMatch(src.replace(/\/\/.*$|\/\*[\s\S]*?\*\//gm, ""), /\b360\b/);
  const hook = readFileSync(fileURLToPath(new URL("../src/layout/useDetailLayout.ts", import.meta.url)), "utf8");
  assert.ok(hook.includes("return D ? DETAIL.listMin : MIN_LIST_SIDE_PX;"));
  assert.ok(hook.includes("return D ? D.listW : centerWidth(winW, collapsed);"));
  const shell = readFileSync(fileURLToPath(new URL("../src/layout/AppShell.tsx", import.meta.url)), "utf8");
  assert.ok(shell.includes("windowWidth: size.width,"));
  assert.ok(shell.includes("paneWidth: characterPaneOf(size.width, D, collapsed),"));
  assert.ok(shell.includes("minListPx: characterMinListOf(D),"));
});

test("専用の幅＝clamp(140, min(窓 × 18% × 大きさ, 曲一覧の側 − 残す幅 − 12), 480)。140 まで縮めても残す幅を割るときだけ隠す", () => {
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

test("(1) 窓を1pxずつ広げたとき、ふだんの表示では一度出たら隠れない。詳細で隠れ直すのは、曲一覧の側そのものが狭くなる所（詳細の配置・サイドバー・外の余白）だけ", () => {
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

test("前に隠れていた形（窓 約1220px・サイドバーを開いた詳細／約1060px・たたんだ詳細）と、窓 1400px・100%・詳細の左右で出る", () => {
  for (const [w, collapsed] of [[1220, false], [1060, true], [1400, false]] as const) {
    for (const slots of [0, 1, 2, 3]) {
      const D = detailOf(w, 800, slots, collapsed, true);
      assert.equal(D.side, true, `${w}`);
      const l = characterLayout({ character: chOf(220), visible: true, scale: 1, view: "all", windowWidth: w, paneWidth: D.listW, minListPx: DETAIL.listMin, playing: false, reducedMotion: false });
      assert.ok(l, `${w} slots=${slots}`);
      assert.ok(D.listW - l!.railPx >= DETAIL.listMin);
    }
  }
  // 1220px・開いた詳細：前は隠れていた（窓だけで決めた幅 220 + 12 では 360 残らない）。今は縮めて出す
  const D = detailOf(1220, 800, 2, false, true);
  assert.ok(D.listW - (idealRailWidth(1220, 1) + PAD) < DETAIL.listMin);
  assert.equal(characterFitState(1220, 1, D.listW, DETAIL.listMin), "shrunk");
});

test("(2)(3) 詳細の左右・上下 × 窓 480〜2560px × 大きさ 75〜200% × 出力先の数で、出すときは曲一覧に 360px 残り、曲一覧・ミキサー・立ち絵が重ならない", () => {
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

test("(4) 同じ窓・同じ大きさなら、どの立ち絵（スキンの幅 96〜280・高さ・窓の下限がばらばら）でも、専用の幅も曲一覧の列も同じ", () => {
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

test("曲一覧の列は引いた後の幅（.main − 専用の幅）で決め、出す判断と幅は引く前の幅で決める（引きずり合わない）", () => {
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

test("(5) 大きさのつまみの下の案内は、実際の出し方（いちばん大きく・縮めて・隠す）と食い違わない", () => {
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
  assert.ok(tray.includes("crate::skins::runtime::reset_look(&app)")); // 別のスレッドの中で呼ぶ
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

// ---------------- v3.10.0 ----------------

test("行に「手で選んだ色／スキンの色／既定の色」の札を出さない（手で変えたかは「戻す」が押せるかで分かる）", () => {
  const view = read("../src/views/SettingsColors.tsx");
  assert.ok(!view.includes("SOURCE_TEXT") && !view.includes("color-source"));
  assert.ok(!/"スキンの色"|"既定の色"|"手で選んだ色"/.test(view));
  assert.ok(view.includes('disabled={props.disabled || props.source !== "manual"}'));
  assert.ok(!read("../src/theme/base.css").includes(".color-source"));
});

test("濃さのつまみの刻みは保存する桁（0.1%）と同じ。どの濃さでも、つまみの位置と「濃さ N%」の表示がそろう", () => {
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

test("設定を書く道は全部「読む → 直す → 書く」を1つの鍵で一続きにする（全体を書き戻すだけの古い関数は残っていない）", () => {
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

// ---------------- v3.10.0 ----------------

test("設定の知らせは最新を読み直して1か所から送る・トレイの「既定の見た目に戻す」は別のスレッドへ渡す（メインのスレッドで書く鍵を待たない）", () => {
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

// ---------------- v3.10.0：ビルドした CSS の色を読む ----------------
// ソースの CSS（dark.css・light.css）だけを見るテストでは、ビルドで CSS を小さくする道具が rgba(…) を #rrggbbaa に書き換えることが
// 見えなかった（画面では半透明の5色が #000000・濃さ 100% と読まれ、読みやすさの警告が黙って0件になっていた）。
// ここでは vite で一時フォルダへビルドし、できた CSS のテーマの色を、画面と同じ読み取りに通す（npm ci 済みであること）

/** v3.11.0：ビルドした CSS の全文（アイコンの欄の大きさの決まりが、小さくする道具を通っても残ることを確かめる） */
let BUILT_CSS = "";

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
    BUILT_CSS = css;
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

test("ビルドした CSS のテーマの色（半透明の色は #rrggbbaa に書き換わっている）を、画面と同じ読み取りで全部読める", () => {
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
    // 基準に届くかどうかは、次のテスト（v3.11.1）でビルドした CSS の値そのもので確かめる
  }
});

test("v3.11.1：ビルドした CSS の値で計算しても、基準に届かない組み合わせはソースの値と同じ（Dark の指を乗せた面・入力欄の上の薄い文字が 4.5 以上）", () => {
  for (const [theme, file] of [["dark", dark], ["light", light]] as const) {
    const fails = (rows: { scene: string; name: string; ratio: number; min: number }[]) =>
      rows.filter((r) => !(r.ratio >= r.min)).map((r) => `${r.scene} ${r.name}`).sort();
    const src = fails(evaluate(readTheme(file), {}));
    // ビルドした CSS の書き換わったままの値（#rrggbbaa。画面に当たっている濃さそのもの）と、画面がそれを読んだ値（濃さは小数3桁）の両方で
    assert.deepEqual(fails(evaluate(BUILT[theme], {})), src, `${theme} built raw`);
    assert.deepEqual(fails(evaluate(readThemeColors(BUILT[theme]).colors, {})), src, `${theme} built read`);
  }
  // 3.11.0 で 4.498:1 だった組み合わせ。ビルドの値で 4.5 以上
  const D = readThemeColors(BUILT.dark).colors;
  for (const t of [BUILT.dark, D]) {
    for (const name of ["--c-secondary／指を乗せた行", "--c-secondary／入力欄"]) {
      const low = Math.min(...evaluate(t, {}).filter((r) => r.name === name).map((r) => r.ratio));
      assert.ok(low >= 4.5, `${name} ${low}`);
    }
  }
  // 0.067 はビルドで 17/255（#ffffff11 か、短くした #fff1）になり、画面は 0.067 と読む（0.07 のように 0.071 へ上がらない）
  for (const v of ["--c-hover", "--c-input"] as const) {
    const c = contrastColor(BUILT.dark[v]);
    assert.deepEqual(c.rgb, [255, 255, 255], v);
    assert.equal(Math.round(c.a * 255), 17, `${v} ${BUILT.dark[v]}`);
  }
  assert.equal(D["--c-hover"], "rgba(255, 255, 255, 0.067)");
  assert.equal(D["--c-input"], "rgba(255, 255, 255, 0.067)");
});

test("ビルドした CSS の既定を基準にしても、読みやすさの警告が出る（色の欄・スキンの取り込みの確認画面）", () => {
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

test("読むだけで、受け付ける形は広げない（手の色・スキンの値に #rrggbbaa は書けない。Rust の確かめも同じ）", () => {
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

// ---------------- v3.11.0：手で入れるアイコン ----------------

const manualOf = (path: string, still: string | null = null, rev = "r1") => ({ path, still, rev });

test("v3.11.0：重なり順は 今の SVG < スキンのアイコン < 手で入れたアイコン。手のアイコンは Dark・Light 共通・その状態の枠だけ・動きを減らすと最初の1コマ", () => {
  const skinIcons = {
    IconPlay: { default: { dark: { path: "skin/play-d.png", still: null }, light: { path: "skin/play-l.png", still: null } } },
    IconStar: { default: { dark: { path: "skin/star.png", still: null }, light: null }, filled: { dark: { path: "skin/star-f.png", still: null }, light: null } },
    IconRepeat: { default: { dark: { path: "skin/rep.png", still: null }, light: null } },
  };
  const manual = {
    IconPlay: { default: manualOf("icons/IconPlay/default.gif", "icons/IconPlay/derived/default.png", "abc") },
    IconStar: { outline: manualOf("icons/IconStar/outline.png") },
    IconNext: { default: manualOf("icons/IconNext/default.webp") },
  };
  // 手 > スキン（テーマに関係なく同じ手のアイコン）
  for (const theme of ["dark", "light"]) {
    assert.deepEqual(pickLayeredIcon(manual, skinIcons, "IconPlay", "default", theme, false), { path: "icons/IconPlay/default.gif", rev: "abc" });
  }
  assert.deepEqual(pickLayeredIcon(manual, skinIcons, "IconPlay", "default", "dark", true), { path: "icons/IconPlay/derived/default.png", rev: "abc" });
  // 手のアイコンが無い枠はスキンの選び方のまま（rev は付かない）
  assert.deepEqual(pickLayeredIcon(manual, skinIcons, "IconStar", "filled", "dark", false), { path: "skin/star-f.png", rev: null });
  assert.deepEqual(pickLayeredIcon(manual, skinIcons, "IconStar", "outline", "dark", false), { path: "icons/IconStar/outline.png", rev: "r1" });
  // 状態の分からないところ（default）は、手の状態の枠を使わずスキン → 今の SVG
  assert.deepEqual(pickLayeredIcon(manual, skinIcons, "IconRepeat", "default", "dark", false), { path: "skin/rep.png", rev: null });
  assert.equal(pickLayeredIcon(manual, skinIcons, "IconShuffle", "on", "dark", false), null);
  // スキンが無い（既定のスキン）でも手のアイコンは出る。どちらも無ければ null（今の SVG）
  assert.deepEqual(pickLayeredIcon(manual, null, "IconNext", "default", "light", false), { path: "icons/IconNext/default.webp", rev: "r1" });
  assert.equal(pickLayeredIcon(null, null, "IconNext", "default", "light", false), null);
  // 手のアイコンが無ければ 3.10.0 と同じ選び方
  for (const [n, st] of [["IconPlay", "default"], ["IconStar", "filled"], ["IconStar", "outline"], ["IconRepeat", "one"], ["IconX", "default"]] as const) {
    for (const theme of ["dark", "light"]) {
      const p = pickIconPath(skinIcons, n, st, theme, false);
      assert.deepEqual(pickLayeredIcon({}, skinIcons, n, st, theme, false), p ? { path: p, rev: null } : null, `${n} ${st} ${theme}`);
    }
  }
  // 入れ直しの印は URL の問い合わせの部分に付ける（asset プロトコルはパスだけを読む）
  assert.equal(withRev("http://asset.localhost/x.png", "1a-2b"), "http://asset.localhost/x.png?v=1a-2b");
  assert.equal(withRev("http://asset.localhost/x.png", null), "http://asset.localhost/x.png");
  assert.equal(withRev(null, "1"), null);
  // 画面：セーフモードの間は手のアイコンを当てない。URL に印を付ける
  const store = read("../src/stores/skin.ts");
  assert.ok(store.includes("const manual = st.safeMode ? null : st.manualIcons ?? null;"));
  assert.ok(store.includes("withRev(fileUrl(picked.path), picked.rev)"));
  // Rust：セーフモードの間は画面へ渡さない
  const rt = read("../src-tauri/src/skins/runtime.rs");
  assert.ok(/fn refresh_icons[\s\S]*?if safe \{\s*lock\(state\)\.manual_icons = icons::Resolved::new\(\);\s*return;/.test(rt));
});

/** Rust の manifest.rs の ICONS（名前と状態）を読む */
function rustIcons(): [string, string[]][] {
  const rs = read("../src-tauri/src/skins/manifest.rs");
  const block = rs.slice(rs.indexOf("pub const ICONS"), rs.indexOf("];", rs.indexOf("pub const ICONS")));
  return [...block.matchAll(/\("(Icon[A-Za-z]+)", &\[([^\]]*)\]\)/g)].map((m) => [m[1], [...m[2].matchAll(/"([a-z]+)"/g)].map((x) => x[1])]);
}

test("v3.11.0：枠は Rust（skins::icons）と同じ。ボタンのアイコン 35 個（状態のある4つは default を除いた状態ごと）＋ウィンドウ・トレイ。名前は日本語", () => {
  const icons = rustIcons();
  assert.equal(icons.length, 35);
  const want = new Set<string>();
  for (const [n, states] of icons) {
    const named = states.filter((s) => s !== "default");
    for (const s of named.length ? named : ["default"]) want.add(`${n}/${s}`);
  }
  want.add("AppWindow/default");
  want.add("AppTray/default");
  const got = ICON_SLOTS.map((x) => `${x.name}/${x.state}`);
  assert.equal(got.length, new Set(got).size, "no duplicates");
  assert.deepEqual([...got].sort(), [...want].sort());
  assert.equal(got.length, 44);
  // Rust の枠の名前（ウィンドウ・トレイ）と状態の決め方
  const rsIcons = read("../src-tauri/src/skins/icons.rs");
  assert.ok(rsIcons.includes('pub const APP_WINDOW: &str = "AppWindow";') && rsIcons.includes('pub const APP_TRAY: &str = "AppTray";'));
  assert.ok(rsIcons.includes("states.iter().copied().filter(|s| *s != DEFAULT_STATE)"));
  // 場所ごと・日本語の名前・状態の名前は状態のあるものだけ
  const groups = new Set(ICON_GROUPS.map((g) => g.id));
  for (const x of ICON_SLOTS) {
    assert.ok(groups.has(x.group), x.name);
    assert.ok(/[^\x00-\x7f]/.test(x.label), `${x.name} has a Japanese label`);
    assert.equal(x.stateLabel !== null, x.state !== "default", `${x.name}/${x.state}`);
    assert.ok(x.px === SIDEBAR_PX || (x.px >= 14 && x.px <= 32), `${x.name} ${x.px}`);
  }
  for (const g of ICON_GROUPS) assert.ok(ICON_SLOTS.some((x) => x.group === g.id), g.id);
  assert.deepEqual(ICON_SLOTS.filter((x) => x.group === "app").map((x) => x.name), ["AppWindow", "AppTray"]);
  assert.equal(slotPx(ICON_SLOTS.find((x) => x.name === "IconMusic")!, 26), 26);
  assert.equal(slotPx(ICON_SLOTS.find((x) => x.name === "IconPlay")!, 26), 22);
  // 見本は実物と同じ部品で描く：状態の無いボタンのアイコンは全部、部品の表にある
  const view = read("../src/views/SettingsIcons.tsx");
  for (const [n, states] of icons) {
    if (states.length === 1) assert.ok(new RegExp(`^  ${n},$`, "m").test(view), `${n} in PLAIN`);
  }
  for (const n of ["IconStar", "IconVolume", "IconRepeat", "IconShuffle"]) assert.ok(view.includes(`case "${n}":`), n);
  // 数え方
  assert.equal(manualIconCount({ IconPlay: { default: "png" }, IconStar: { filled: "gif", outline: "png" } }), 3);
  assert.equal(manualIconCount(undefined), 0);
  assert.ok(hasManualIcon({ IconStar: { filled: "gif" } }, "IconStar", "filled"));
  assert.ok(!hasManualIcon({ IconStar: { filled: "gif" } }, "IconStar", "outline"));
});

test("v3.12.0：「シャッフル再生」のボタンの枠（IconShufflePlay）。状態なし・「曲一覧」の先頭・16px。4か所のボタンが使い、再生バーの切・入は IconShuffle のまま", () => {
  // 枠：状態なし（default だけ）・名前は「シャッフル再生」・場所は曲一覧の先頭（上のボタンは行より先に見える）・16px
  const slot = ICON_SLOTS.filter((x) => x.name === "IconShufflePlay");
  assert.deepEqual(slot, [{ name: "IconShufflePlay", state: "default", group: "list", label: "シャッフル再生", stateLabel: null, px: 16 }]);
  assert.equal(ICON_SLOTS.filter((x) => x.group === "list")[0].name, "IconShufflePlay");
  assert.deepEqual(rustIcons().find(([n]) => n === "IconShufflePlay"), ["IconShufflePlay", ["default"]]);
  // シャッフルの枠は切・入だけのまま（default の枠は作らない）
  assert.deepEqual(ICON_SLOTS.filter((x) => x.name === "IconShuffle").map((x) => x.state), ["off", "on"]);
  // 部品：線は IconShuffle と同じ形・状態を渡さない（default）
  const comp = read("../src/components/Icons.tsx");
  assert.match(comp, /export const IconShufflePlay = \(p: P\) => \(\s*<Skinned name="IconShufflePlay" p=\{p\}>\s*<S \{\.\.\.p\}>\s*<path d=\{SHUFFLE_PATH\} \/>/);
  assert.match(comp, /<Skinned name="IconShuffle" state=\{shuffleState\(p\.on\)\} p=\{p\}>\s*<S \{\.\.\.p\}>\s*<path d=\{SHUFFLE_PATH\} \/>/);
  assert.ok(comp.includes('const SHUFFLE_PATH = "M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5";'));
  // 使うボタン：曲一覧・年間まとめ・プレイリスト・サイドバーのプレイリストの右クリック。状態の無い <IconShuffle /> はもう無い
  const uses: [string, RegExp][] = [
    ["../src/views/LibraryView.tsx", /<IconShufflePlay \/> シャッフル再生/],
    ["../src/views/YearlyView.tsx", /<IconShufflePlay \/> シャッフル再生/],
    ["../src/views/PlaylistView.tsx", /<IconShufflePlay \/> シャッフル\r?\n/],
    ["../src/layout/Sidebar.tsx", /\{ label: "シャッフル再生", icon: <IconShufflePlay \/>, onClick: \(\) => void api\.playlistPlay\(p\.id, null, true\) \}/],
  ];
  for (const [f, re] of uses) {
    const src = read(f);
    assert.match(src, re, f);
    assert.ok(!src.includes("<IconShuffle />"), f);
  }
  for (const f of ["../src/views/GroupView.tsx", "../src/views/QueueView.tsx", "../src/components/TrackTable.tsx", "../src/components/trackActions.tsx"]) {
    assert.ok(!read(f).includes("<IconShuffle />"), f);
  }
  // 再生バーの切・入は今のまま
  assert.ok(read("../src/layout/PlayerBar.tsx").includes("icon: <IconShuffle on={s.shuffle} />,"));
  // 見本の欄：状態の無い部品の表にある・説明文は今の仕組みのとおり
  const view = read("../src/views/SettingsIcons.tsx");
  assert.ok(/^  IconShufflePlay,$/m.test(view.slice(view.indexOf("const PLAIN"))));
  assert.ok(view.includes("曲一覧・年間まとめの「シャッフル再生」とプレイリストの「シャッフル」のボタン、サイドバーのプレイリストの右クリックの「シャッフル再生」は、「曲一覧」の「シャッフル再生」の枠で替わります（再生バーの「シャッフル」の切・入の枠では替わりません）。"));
  // 説明の呼び名は、それぞれのボタンに出ている文字と同じ（プレイリストは「シャッフル」、ほかは「シャッフル再生」。サイドバーは右クリックのメニュー）
  assert.match(read("../src/views/PlaylistView.tsx"), /<IconShufflePlay \/> シャッフル\n/);
  for (const f of ["../src/views/LibraryView.tsx", "../src/views/YearlyView.tsx"]) assert.match(read(f), /<IconShufflePlay \/> シャッフル再生\n/, f);
  const side = read("../src/layout/Sidebar.tsx");
  assert.ok(side.includes('{ label: "シャッフル再生", icon: <IconShufflePlay />') && side.includes("onContextMenu={(e) =>"));
  assert.ok(!view.includes("スキンのアイコンか元のアイコンのままです"));
  // 名前は「シャッフル」と「再生」の切れ目で折り返せる（文節で切る CSS。カタカナの言葉は 6 文字以内で、名前の 80px に入る）
  assert.deepEqual(wrapParts("シャッフル再生"), ["シャッフル再生"]);
  assert.ok("シャッフル".length * 12 <= 80 && "再生".length * 12 <= 80);
});

test("v3.12.0：「シャッフル再生」の絵の順は 手の IconShufflePlay → スキンの IconShufflePlay → スキンの IconShuffle の default → 線（null）。手の IconShuffle の切・入は見ない", () => {
  assert.deepEqual(SKIN_FALLBACK, { IconShufflePlay: "IconShuffle" });
  const t = (p: string) => ({ path: p, still: null });
  const full = {
    IconShufflePlay: { default: { dark: t("skin/sp-d.png"), light: t("skin/sp-l.png") } },
    IconShuffle: { default: { dark: t("skin/sh-d.png"), light: t("skin/sh-l.png") }, on: { dark: t("skin/sh-on.png"), light: null }, off: { dark: t("skin/sh-off.png"), light: null } },
  };
  const onlyShuffle = { IconShuffle: full.IconShuffle };
  const onlyStates = { IconShuffle: { on: full.IconShuffle.on, off: full.IconShuffle.off } };
  const manualPlay = { IconShufflePlay: { default: manualOf("icons/IconShufflePlay/default.gif", "icons/IconShufflePlay/derived/default.png", "q1") } };
  const manualShuffle = { IconShuffle: { off: manualOf("icons/IconShuffle/off.png"), on: manualOf("icons/IconShuffle/on.png") } };
  const pick = (m: any, k: any, theme = "dark", rm = false) => pickLayeredIcon(m, k, "IconShufflePlay", "default", theme, rm);
  // 1. 手で入れた IconShufflePlay（テーマに関係なく同じ。動きを減らすと最初の1コマ）
  for (const theme of ["dark", "light"]) assert.deepEqual(pick({ ...manualPlay, ...manualShuffle }, full, theme), { path: "icons/IconShufflePlay/default.gif", rev: "q1" });
  assert.deepEqual(pick(manualPlay, full, "dark", true), { path: "icons/IconShufflePlay/derived/default.png", rev: "q1" });
  // 2. スキンの IconShufflePlay（テーマの絵）
  assert.deepEqual(pick(manualShuffle, full, "dark"), { path: "skin/sp-d.png", rev: null });
  assert.deepEqual(pick(manualShuffle, full, "light"), { path: "skin/sp-l.png", rev: null });
  // 3. スキンの IconShuffle の default（3.11.1 までと同じ見え方）。切・入の絵は使わない
  assert.deepEqual(pick(manualShuffle, onlyShuffle, "dark"), { path: "skin/sh-d.png", rev: null });
  assert.deepEqual(pick(null, onlyShuffle, "light"), { path: "skin/sh-l.png", rev: null });
  assert.equal(pick(manualShuffle, onlyStates, "dark"), null);
  // 4. どれも無ければ線（null）。手の IconShuffle の切・入だけでは替わらない
  assert.equal(pick(manualShuffle, null), null);
  assert.equal(pick(manualShuffle, {}), null);
  assert.equal(pick(null, null), null);
  // 3.11.1 までの <IconShuffle />（状態なし）と同じ絵になる（手のアイコンが無いとき）
  for (const k of [full, onlyShuffle, onlyStates, {}, null]) {
    for (const theme of ["dark", "light"]) {
      const before = pickLayeredIcon({}, k as any, "IconShuffle", "default", theme, false);
      const kk = k && "IconShufflePlay" in k ? { ...k, IconShufflePlay: undefined } : k;
      assert.deepEqual(pickLayeredIcon({}, kk as any, "IconShufflePlay", "default", theme, false), before, `${theme}`);
    }
  }
  // 代わりを見るのは IconShufflePlay だけ。ほかのアイコンは今までどおり（スキンに無ければ null）
  assert.equal(pickLayeredIcon(null, { IconPlay: full.IconShuffle } as any, "IconPause", "default", "dark", false), null);
  // 再生バーの切・入は今までどおり（手の切・入 → スキンの切・入 → スキンの default）
  assert.deepEqual(pickLayeredIcon(manualShuffle, full, "IconShuffle", "on", "dark", false), { path: "icons/IconShuffle/on.png", rev: "r1" });
  assert.deepEqual(pickLayeredIcon(null, onlyShuffle, "IconShuffle", "off", "dark", false), { path: "skin/sh-off.png", rev: null });
});

test("v3.11.0：ドロップの振り分け。アイコンの枠の上はその枠へ（取り込みに回さない）。枠の外は今までどおり（受け手・取り込み・何もしない）", () => {
  const icon = iconZoneKey("IconStar", "filled");
  assert.equal(icon, "icon:IconStar:filled");
  assert.deepEqual(parseIconZone(icon), { name: "IconStar", state: "filled" });
  for (const bad of [null, "", "bg", "album", "icon:", "icon:IconStar", "icon:a:b:c", "xicon:IconStar:filled"]) assert.equal(parseIconZone(bad), null, String(bad));
  // 枠の上：ファイル1つでも、複数・何も無い（ファイルでないもの）でも枠へ（断るのは Rust）
  for (const n of [0, 1, 2, 30]) assert.equal(routeDrop(icon, n), "zone", String(n));
  // 枠の外：3.10.0 と同じ（パスが無ければ何もしない・ほかの受け手・取り込み）
  assert.equal(routeDrop(null, 0), "none");
  assert.equal(routeDrop(null, 1), "import");
  assert.equal(routeDrop(null, 5), "import");
  assert.equal(routeDrop("bg", 0), "none");
  assert.equal(routeDrop("bg", 1), "zone");
  assert.equal(routeDrop("album", 3), "zone");
  // 画面：FileDrop は routeDrop で振り分け、受け手に落とした位置を渡す。取り込みの呼び方は変えていない
  const fd = read("../src/components/FileDrop.tsx");
  assert.ok(fd.includes("const route = routeDrop(zone, paths.length);"));
  assert.ok(fd.includes('if (route === "none") return;'));
  assert.ok(fd.includes("zones.get(zone)?.(paths, { x: p.position.x, y: p.position.y });"));
  assert.equal((fd.match(/api\.libraryImport\(paths\)/g) ?? []).length, 1);
  // 枠の受け手は位置だけを Rust へ渡す（パスは渡さない）
  const view = read("../src/views/SettingsIcons.tsx");
  assert.ok(view.includes("void dropIcon(slot.name, slot.state, pos.x, pos.y);"));
  assert.ok(view.includes("data-drop={key}"));
  const cmds = read("../src/ipc/commands.ts");
  assert.ok(cmds.includes('iconsDrop: (name: string, state: string, x: number, y: number) => invoke<SkinState>("icons_drop", { name, state, x, y }),'));
  assert.ok(!/icons_(drop|pick)", \{[^}]*path/.test(cmds));
  // Rust：パスはウィンドウの知らせで控えたものだけ（窓口は位置を受ける。パスの引数は無い）
  const rsCmd = read("../src-tauri/src/commands/icons.rs");
  assert.ok(rsCmd.includes("pub async fn icons_drop(app: AppHandle, name: String, state: String, x: f64, y: f64)"));
  assert.ok(rsCmd.includes("let path = icons::take_drop(x, y).map_err(icons::for_screen)?;"));
  assert.ok(!/fn icons_\w+\([^)]*path: String/.test(rsCmd));
  const lib = read("../src-tauri/src/lib.rs");
  assert.ok(lib.includes("if let WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, position }) = event {"));
  assert.ok(lib.includes("skins::icons::record_drop(paths, position.x, position.y);"));
  for (const c of ["icons_pick", "icons_drop", "icons_reset", "icons_clear", "icons_app_preview"]) assert.ok(lib.includes(`commands::icons::${c},`), c);
});

test("v3.11.0：欄の作り（場所ごとの一覧・枠を押すとファイルを選ぶ・枠ごとの「戻す」・全部戻すは確認あり・セーフモード）。色の欄と立ち絵の欄の間", () => {
  const view = read("../src/views/SettingsIcons.tsx");
  // 枠はボタン（キーボードでも押せる）。押すと Rust がファイルを選ぶ画面を出す
  assert.ok(view.includes('className="icon-slot-frame"') && view.includes("onClick={() => void pickIcon(slot.name, slot.state)}"));
  assert.ok(view.includes("disabled={props.safe || !props.manual} onClick={() => void resetIcon(slot.name, slot.state)}"));
  assert.ok(view.includes('"手で入れたアイコンを全部戻す"') && view.includes("confirmDialog(") && view.includes("disabled={count === 0}"));
  assert.ok(view.includes("セーフモード") && view.includes('code: "icon_safe_mode"'));
  // 実際の大きさ：枠の中の見本は --slot-px で描く
  assert.ok(view.includes('style={{ ["--slot-px" as any]: `${props.px}px` }}'));
  const skinsView = read("../src/views/SettingsSkins.tsx");
  const c = skinsView.indexOf("<ColorsField />");
  const i = skinsView.indexOf("<IconsField />");
  const a = skinsView.indexOf("<AvatarsField />");
  assert.ok(c > 0 && c < i && i < a);
  // 「既定の見た目に戻す」の確認に、手で入れたアイコンの数を出す（無いときは 3.10.0 と同じ文）
  assert.ok(skinsView.includes("手で入れたアイコン（${manualIcons} 個。入れた画像も消えます）"));
  assert.ok(skinsView.includes("manualCount === 0 && manualIcons === 0"));
  // 外のライブラリを足していない
  const pkg = JSON.parse(read("../package.json"));
  assert.deepEqual(Object.keys(pkg.dependencies).sort(), ["@dnd-kit/core", "@dnd-kit/sortable", "@tanstack/react-virtual", "@tauri-apps/api", "@tauri-apps/plugin-global-shortcut", "react", "react-dom"]);
});

test("v3.11.0：保存は Rust の窓口だけ・画面から来た設定の保存では変わらない・「既定の見た目に戻す」は手のアイコンも外して画像を消す・置き場所と画面が読める範囲", () => {
  const rsSettings = read("../src-tauri/src/commands/settings.rs");
  assert.ok(rsSettings.includes("s.skin.icons = before.skin.icons.clone();"));
  const rt = read("../src-tauri/src/skins/runtime.rs");
  assert.ok(/pub\(crate\) fn reset_look_settings[\s\S]*?s\.skin\.icons\.clear\(\);/.test(rt));
  assert.ok(rt.includes("plain.skin.icons = reset.skin.icons;"));
  assert.ok(/pub fn reset_look[\s\S]*?let r = reset\(app\);\s*remove_icon_files_if_cleared\(&state\);/.test(rt));
  // ファイルを置くのと設定に書くのは files_lock の中で一続き
  assert.ok(/icons::write\(&dirs\(&state\), &prepared\)\.map_err\(icons::for_screen\)\?;\s*update_settings\(/.test(rt));
  // 設定のバックアップ：選択は入れて読み込む（項目の無い zip ではこの PC のまま）、画像は入れない
  const backup = read("../src-tauri/src/settings/backup.rs");
  assert.ok(backup.includes('skin_field_in_zip(entries, "icons")'));
  assert.ok(backup.includes("if let Some(i) = zip_icons {"));
  assert.ok(/fn wanted\(name: &str\) -> bool \{[\s\S]*?\}/.exec(backup)![0].indexOf("icons") < 0);
  // 置き場所は icons\<名前>\<状態>.<形式>。画面が読めるのは icons の下（icons-staging は読めない）
  const conf = JSON.parse(read("../src-tauri/tauri.conf.json"));
  assert.ok(conf.app.security.assetProtocol.scope.includes("$APPDATA/icons/**"));
  assert.ok(!JSON.stringify(conf.app.security.assetProtocol.scope).includes("icons-staging"));
  assert.equal(conf.identifier, "jp.local.plplayer3");
  const mod = read("../src-tauri/src/skins/mod.rs");
  assert.ok(mod.includes('icons: data_dir.join("icons"),') && mod.includes('icon_staging: data_dir.join("icons-staging"),'));
  const rsIcons = read("../src-tauri/src/skins/icons.rs");
  assert.ok(rsIcons.includes('format!("{name}/{state}.{ext}")'));
  // 画像の確かめはスキンと同じ images::check。アプリのアイコンは静止画だけ
  assert.ok(rsIcons.includes("images::check(&data, &inner)") && rsIcons.includes("an application icon must be a still image"));
  // トレイの「既定の見た目に戻す」は今までどおり別のスレッドで（reset_look の中で手のアイコンも外れる）
  const tray = read("../src-tauri/src/platform/windows/tray.rs");
  assert.equal((tray.match(/runtime::reset_look\(/g) ?? []).length, 1);
});

test("v3.11.0：ビルドした CSS でも、枠の中の見本はその場所の大きさ（--slot-px）で描く（スキン・手の画像 img.skin-icon の 20px より強い）", () => {
  assert.ok(BUILT_CSS.length > 0);
  // 小さくする道具は空白を詰めるので、空白を外して比べる（子孫の空白も消えるので、選び方は ".icon-slot-frame.icon" の形になる）
  const flat = BUILT_CSS.replace(/\s+/g, "");
  const rules = [...flat.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((r) => r[1].split(",").some((sel) => sel === ".icon-slot-frameimg.skin-icon"));
  assert.ok(rules.length > 0, "built CSS keeps the slot size rule for skin images");
  assert.ok(rules.some((r) => r[1].split(",").includes(".icon-slot-frame.icon") && r[2].includes("width:var(--slot-px,20px)") && r[2].includes("height:var(--slot-px,20px)")), rules.map((r) => r[0]).join(" | "));
});

test("v3.11.0：枠の名前は切らずに折り返し、状態（入・切・オン・オフ・全曲・1曲など）は別の行に必ず出す。ビルドした CSS でも省略記号にしない", () => {
  const view = read("../src/views/SettingsIcons.tsx");
  // v3.11.1：名前は WrapText で出す（「・」の後ろに <wbr> を入れるだけで、文字は変えない）
  assert.ok(/<span className="icon-slot-label">\s*<WrapText text=\{slot\.label\} \/>\s*<\/span>/.test(view));
  assert.ok(view.includes('{slot.stateLabel ? <span className="icon-slot-state">{slot.stateLabel}</span> : null}'));
  // 同じ名前の枠は、状態の名前で見分けられる（名前＋状態の組は重ならない・状態の名前は短い）
  const shown = ICON_SLOTS.map((x) => `${x.label}/${x.stateLabel ?? ""}`);
  assert.equal(new Set(shown).size, shown.length);
  for (const x of ICON_SLOTS) if (x.stateLabel) assert.ok(x.stateLabel.length <= 3, x.stateLabel);
  const rule = (css: string, sel: string) => {
    const flat = css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, "");
    return [...flat.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((r) => r[1].split(",").includes(sel)).map((r) => r[2]).join(";");
  };
  for (const css of [read("../src/theme/base.css"), BUILT_CSS]) {
    for (const sel of [".icon-slot-name", ".icon-slot-label", ".icon-slot-state"]) {
      const body = rule(css, sel);
      assert.ok(body.length > 0, sel);
      assert.ok(!body.includes("text-overflow:ellipsis") && !body.includes("overflow:hidden"), `${sel}: ${body}`);
    }
    assert.ok(rule(css, ".icon-slot-name").includes("flex-direction:column"));
    assert.ok(rule(css, ".icon-slot-label").includes("overflow-wrap:anywhere"));
  }
});

test("v3.11.1：設定画面の短い文字（見出し・項目名・チェックの名前・アイコンの場所と枠の名前・色の名前）は、言葉の切れ目で折り返し、はみ出さない", () => {
  // 「・」「／」の後ろで分け、つなぐと元の文字列に戻る（画面に出る文字は変えない）
  assert.deepEqual(wrapParts("ウィンドウ・タスクバー"), ["ウィンドウ・", "タスクバー"]);
  assert.deepEqual(wrapParts("そのほか（メニュー・ボタン・歌詞の画面）"), ["そのほか（メニュー・", "ボタン・", "歌詞の画面）"]);
  assert.deepEqual(wrapParts("シャッフル"), ["シャッフル"]);
  assert.deepEqual(wrapParts("戻す／消す"), ["戻す／", "消す"]);
  assert.deepEqual(wrapParts(""), []);
  for (const t of [...ICON_SLOTS.map((x) => x.label), ...ICON_GROUPS.map((g) => g.label), ...COLOR_ROWS.map((r) => r.label)]) {
    assert.equal(wrapParts(t).join(""), t, t);
  }
  // 名前は WrapText で出す（アイコンの枠・場所・色の行）
  const icons = read("../src/views/SettingsIcons.tsx");
  assert.ok(/<div className="icon-group-label">\s*<WrapText text=\{g\.label\} \/>\s*<\/div>/.test(icons));
  const colors = read("../src/views/SettingsColors.tsx");
  assert.ok(/<span className="flabel">\s*<WrapText text=\{props\.label\} \/>\s*<\/span>/.test(colors));
  const wrap = read("../src/components/WrapText.tsx");
  assert.ok(wrap.includes("<wbr />") && wrap.includes("wrapParts(props.text)"));
  // CSS：文節で切る（知らないブラウザでは keep-all）。言葉が幅に入らないときだけ途中で切る。ソースとビルドした CSS の両方
  const flat = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, "");
  const rulesFor = (css: string, sel: string) =>
    [...flat(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((r) => r[1].split(",").includes(sel)).map((r) => r[2]);
  for (const css of [read("../src/theme/base.css"), BUILT_CSS]) {
    for (const sel of [".settingsh3", ".settings.skin-subhead", ".settings.flabel", ".settings.field>label", ".settingslabel.row", ".settings.icon-group-label", ".settings.icon-slot-label"]) {
      const bodies = rulesFor(css, sel);
      assert.ok(
        bodies.some((b) => b.includes("word-break:auto-phrase") && b.includes("overflow-wrap:anywhere")),
        `${sel}: ${bodies.join(" | ")}`,
      );
      // keep-all は auto-phrase より前（auto-phrase を知るブラウザでは後ろが勝つ）
      const b = bodies.find((x) => x.includes("word-break:auto-phrase"))!;
      if (b.includes("word-break:keep-all")) assert.ok(b.indexOf("word-break:keep-all") < b.indexOf("word-break:auto-phrase"), b);
    }
    // 枠の最小の幅は、名前に 80px 残る 188px。欄がそれより狭くても横にはみ出さない（min(100%, …)）
    const slots = rulesFor(css, ".icon-slots").join(";");
    assert.ok(slots.includes("minmax(min(100%,188px),1fr)"), slots);
  }
  // 188px から、見本の枠 40・余白 6×2・横の内側 6×2・縁 1×2・「戻す」（12px の字 2つ＋内側 8×2＋縁 1×2＝42）を引くと、名前に 80px（12px の字で6文字＋余り）
  assert.equal(188 - 40 - 6 * 2 - 6 * 2 - 1 * 2 - (12 * 2 + 8 * 2 + 1 * 2), 80);
  // カタカナだけの名前（文節で切っても分かれない）は 6文字（12px の字で 72px）以内なので、名前の 80px に1行で入る
  const words = ICON_SLOTS.flatMap((x) => wrapParts(x.label)).filter((w) => /^[ァ-ヶー]+$/.test(w.replace(/・$/, "")));
  for (const w of words) assert.ok(w.replace(/・$/, "").length <= 6, w);
});

test("v3.11.0 見本スキン：くもくじら・ペンギンの両テーマの色は、ビルドした CSS の既定を基準にしても読みやすさの警告が出ない（取り込みの確認画面と同じ）", () => {
  for (const dir of ["kumokujira", "penguin"]) {
    const skin = JSON.parse(read(`../samples/${dir}/skin.json`));
    for (const theme of ["dark", "light"] as const) {
      const base = readThemeColors(BUILT[theme]).colors;
      const r = contrastWarningsOrError(base, skin.parts.appearance.themes[theme].tokens);
      assert.equal(r.error, null, `${dir} ${theme}`);
      assert.deepEqual(r.rows.map((x) => `${x.scene} ${x.name} ${x.ratio.toFixed(2)}`), [], `${dir} ${theme}`);
      // 色の値は手の色・スキンと同じ形の確かめに通る（取り込みで断られない）
      for (const [n, v] of Object.entries(skin.parts.appearance.themes[theme].tokens)) assert.equal(validColorToken(n, String(v)) !== null, true, `${dir} ${theme} ${n}`);
    }
  }
});

// ---------------- v3.12.0：取り込んだスキンの立ち絵のカード・取り込みの数の上限 ----------------

test("v3.12.0：取り込んだスキンの立ち絵のカードは、説明にスキンの名前を書き、「スキン」の印を付ける（v3.12.1 から共通の文は欄の頭に1回だけ）。ほかのカードは今のまま", () => {
  const list = {
    skins: [{ skinId: "kumokujira-sample", name: "くもくじら（見本）", preview: null, animated: false }],
    avatars: [{ id: "0123456789abcdef", name: "Pose", preview: null, hasStopped: true, animated: false, broken: null }],
    max: 30,
  };
  const rows = avatarRows(list, "くもくじら（見本）", true);
  const by = (k: string) => rows.find((r) => r.kind === k)!;
  const sc = by("skinCharacter");
  assert.equal(sc.name, "くもくじら（見本）");
  assert.equal(sc.note, skinCharacterNote("くもくじら（見本）"));
  assert.equal(sc.note, "スキン「くもくじら（見本）」の立ち絵");
  assert.equal(sc.badge, SKIN_CHARACTER_BADGE);
  assert.equal(SKIN_CHARACTER_BADGE, "スキン");
  // 選び方の「スキンの立ち絵」・「出さない」・登録した立ち絵は今のまま（印なし。文も 3.11.1 と同じ）
  assert.deepEqual([by("skin").name, by("skin").note, by("skin").badge], ["スキンの立ち絵", "いまは「くもくじら（見本）」の立ち絵", null]);
  assert.deepEqual([by("none").name, by("none").note, by("none").badge], ["出さない", "立ち絵を出しません", null]);
  assert.deepEqual([by("avatar").note, by("avatar").badge], ["登録した立ち絵", null]);
  // 句読点の崩れ（「。、」「。。」「、、」）が無い
  for (const r of rows) assert.ok(!/[。、][。、]/.test(r.note), r.note);
  // 読み上げ：スキンそのものと取り違えないよう「スキン『◯◯』の立ち絵」と読む。ほかは 3.11.1 と同じ
  assert.equal(avatarCardLabel(sc, false), "スキン「くもくじら（見本）」の立ち絵を使う");
  assert.equal(avatarCardLabel(sc, true), "スキン「くもくじら（見本）」の立ち絵（使用中）");
  assert.equal(avatarCardLabel(by("avatar"), false), "Poseを使う");
  assert.equal(avatarCardLabel(by("skin"), true), "スキンの立ち絵（使用中）");
  assert.equal(avatarCardLabel({ kind: "avatar", name: "", broken: "avatar_broken" }, false), "読めない立ち絵");
  // 画面：印は「スキン」の一覧の札と同じ部品（.skin-tag）で、v3.12.1 から名前と同じ行。説明はどのカードも wrap を付けて切らない（全文が見えるので title は付けない）
  const view = read("../src/views/SettingsAvatars.tsx");
  assert.ok(view.includes("aria-label={avatarCardLabel(r, on)}"));
  assert.ok(view.includes('<div className="skin-meta wrap">{r.note}</div>'));
  assert.ok(!view.includes("title={r.note}"));
  assert.match(view, /<div className="avatar-card-head">\s*<span className="skin-name">\{r\.name\}<\/span>\s*\{r\.badge \? <span className="skin-tag">\{r\.badge\}<\/span> : null\}\s*<\/div>/);
  assert.ok(read("../src/views/SettingsSkins.tsx").includes('<span className="skin-tag">削除不可</span>'));
  // 削除のボタンは登録した立ち絵だけ（スキンの立ち絵はスキンを削除すると一緒に消える）
  assert.match(view, /\{r\.kind === "avatar" \? \(\s*<button className="btn danger" onClick=\{\(\) => void del\(r\)\}>/);
});

test("v3.12.0：取り込んだスキンの立ち絵のカードの説明は、省略記号で切らずに言葉の切れ目で折り返す（ソースとビルドした CSS）", () => {
  const flat = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, "");
  const rulesFor = (css: string, sel: string) =>
    [...flat(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((r) => r[1].split(",").includes(sel)).map((r) => r[2]);
  for (const css of [read("../src/theme/base.css"), BUILT_CSS]) {
    const body = rulesFor(css, ".skin-meta.wrap").join(";");
    assert.ok(body.includes("white-space:normal"), body);
    assert.ok(!body.includes("text-overflow:ellipsis") && !body.includes("overflow:hidden"), body);
    // 切れ目は v3.11.1 の設定画面の短い文字と同じ決まり（文節で切る・知らないブラウザでは keep-all・入らない言葉だけ途中で切る）
    const phrase = rulesFor(css, ".settings.skin-meta.wrap");
    assert.ok(phrase.some((b) => b.includes("word-break:auto-phrase") && b.includes("overflow-wrap:anywhere")), phrase.join(" | "));
    // 札は1行のまま（一覧の札と同じ）
    assert.ok(rulesFor(css, ".skin-tag").join(";").includes("white-space:nowrap"));
  }
  // ふつうのカードの説明（.skin-meta）は今までどおり1行で、省略記号
  const meta = rulesFor(read("../src/theme/base.css"), ".skin-meta").join(";");
  assert.ok(meta.includes("white-space:nowrap") && meta.includes("text-overflow:ellipsis"), meta);
});

test("v3.12.0：スキンの取り込みの数の上限（ファイル 128・項目 160・中央ディレクトリ 512）が、断るときの文・README・手引き・仕様の表と同じ", () => {
  const mod = read("../src-tauri/src/skins/mod.rs");
  const num = (re: RegExp) => Number(re.exec(mod)![1]);
  const files = num(/pub const MAX_FILES: usize = (\d+);/);
  const entries = num(/pub const MAX_ENTRIES: usize = (\d+);/);
  const scanned = num(/pub const MAX_SCANNED_ENTRIES: usize = (\d+);/);
  assert.deepEqual([files, entries, scanned], [128, 160, 512]);
  // skin.json が指せる画像の数：アイコンの状態（default を含む）× Dark・Light ＋ ウィンドウ・トレイ・トレイの再生中 ＋ 立ち絵2枚 ＋ preview
  const manifest = read("../src-tauri/src/skins/manifest.rs");
  const table = manifest.slice(manifest.indexOf("pub const ICONS"), manifest.indexOf("];", manifest.indexOf("pub const ICONS")));
  const states = [...table.matchAll(/\("Icon[A-Za-z]+", &\[([^\]]*)\]\)/g)].map((m) => m[1].split(",").length);
  assert.equal(states.length, 35);
  const images = states.reduce((a, b) => a + b, 0) * 2 + 3 + 2 + 1;
  assert.equal(images, 98);
  assert.ok(images + 1 <= files, "every image the skin can use, plus skin.json, fits the file limit");
  assert.ok(read("../src/messages/ja.ts").includes(`ファイル ${files} 個・画像1つ 15MB まで`));
  assert.ok(read("../README.md").includes(`ファイル ${files} 個・画像1つ 15MB`));
  const guide = read("../docs/スキンの作り方.md");
  assert.ok(guide.includes(`ファイル ${files} 個（フォルダも数えて ${entries} 項目）`));
  assert.ok(guide.includes(`全部で ${images} 枚`));
  const spec = read("../docs/着せ替え_仕様.md");
  assert.ok(spec.includes(`| 項目の数（フォルダも数える） | ${entries} |`));
  assert.ok(spec.includes(`| 中央ディレクトリの項目の数（OS が足すファイルも含めた全部） | ${scanned} |`));
  assert.ok(spec.includes(`| ファイルの数 | ${files} |`));
  assert.ok(spec.includes(`${images} 枚`));
  assert.ok(spec.includes(`項目 ${entries + 1}・ファイル ${files + 1}・中央ディレクトリの項目 ${scanned + 1}`));
  // 3.11.1 までの値が、今の上限として残っていない
  for (const f of ["../src/messages/ja.ts", "../README.md", "../docs/スキンの作り方.md"]) assert.ok(!read(f).includes("ファイル 64 個・画像1つ"), f);
});

test("v3.12.0：「立ち絵」の欄のカードは左に絵・右に名前・説明・印・ボタン。「スキン」の一覧のカードは今のまま（上に絵・下に文字）", () => {
  const view = read("../src/views/SettingsAvatars.tsx");
  // 格子とカードの名前。カードの中は 絵（RowPreview）→ 右の列（名前と印・説明・読めない理由・ボタン）の順（v3.12.1 から印は名前の行）
  assert.ok(view.includes('<div className="skin-grid avatar-grid" role="group" aria-label="使う立ち絵（カードを押すと切り替わります）">'));
  assert.ok(view.includes('className={"skin-card avatar-card" + (on ? " active" : "")'));
  assert.ok(view.includes('<div className="skin-preview avatar-card-preview" aria-hidden="true">'));
  const card = view.slice(view.indexOf("<RowPreview row={r} skinStill={skinStill} />"), view.indexOf("        })}", view.indexOf("<RowPreview row={r}")));
  const order = ['<div className="avatar-card-body">', '<span className="skin-name">{r.name}</span>', '<span className="skin-tag">{r.badge}</span>', '<div className="skin-meta wrap">{r.note}</div>', 'className="error-text"', '<div className="skin-actions">'];
  let at = 0;
  for (const part of order) {
    const i = card.indexOf(part, at);
    assert.ok(i >= at, part);
    at = i;
  }
  // ボタン・カード全体を押す・キー・読み上げは今のまま
  for (const t of ["cardClickSwitches(r, on, !!(e.target as HTMLElement).closest(\"button\"))", "cardKeySwitches(e.key, r, on, e.target === e.currentTarget)", 'role="button"', "aria-pressed={on}", ">\n                      使用中"]) {
    assert.ok(view.includes(t), t);
  }
  // 「スキン」の一覧のカードは今のまま（avatar-card を付けない・説明は1行）
  const skinsView = read("../src/views/SettingsSkins.tsx");
  assert.ok(!skinsView.includes("avatar-card") && !skinsView.includes("avatar-grid"));
  assert.ok(skinsView.includes('<div key={x.id} className={"skin-card" + (on ? " active" : "") + (x.broken ? " broken" : "")}>'));
  const flat = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, "");
  const rulesFor = (css: string, sel: string) =>
    [...flat(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((r) => r[1].split(",").includes(sel)).map((r) => r[2]).join(";");
  for (const css of [read("../src/theme/base.css"), BUILT_CSS]) {
    // 格子は今と同じ auto-fill。1枚の最小の幅 280px、欄がそれより狭ければ1列（はみ出さない）
    assert.ok(rulesFor(css, ".avatar-grid").includes("grid-template-columns:repeat(auto-fill,minmax(min(100%,280px),1fr))"), rulesFor(css, ".avatar-grid"));
    const c = rulesFor(css, ".skin-card.avatar-card");
    assert.ok(c.includes("flex-direction:row"), c);
    const pv = rulesFor(css, ".avatar-card.avatar-card-preview");
    for (const d of ["flex:none", "width:72px", "height:96px"]) assert.ok(pv.includes(d), `${d}: ${pv}`);
    const body = rulesFor(css, ".avatar-card-body");
    for (const d of ["min-width:0", "flex-direction:column"]) assert.ok(body.includes(d), `${d}: ${body}`);
    // 名前も切らずに、言葉の切れ目で折り返す
    const name = rulesFor(css, ".avatar-card.skin-name");
    assert.ok(name.includes("white-space:normal") && !name.includes("ellipsis"), name);
    const phrase = rulesFor(css, ".settings.avatar-card.skin-name");
    assert.ok(phrase.includes("word-break:auto-phrase") && phrase.includes("overflow-wrap:anywhere"), phrase);
  }
  // 「スキン」の一覧のカードの決まり（上に絵・下に文字・格子 190px）は今のまま
  const base = read("../src/theme/base.css");
  assert.ok(rulesFor(base, ".skin-grid").includes("minmax(190px,1fr)"));
  assert.ok(rulesFor(base, ".skin-card").includes("flex-direction:column"));
  // 右の列に残る幅：280 − 内側 9×2 − 縁 1×2 − 絵 72 − あき 10 = 178px。「使う」「変更」「削除」は、ボタンの字（本文の 14px。スキンで +2px まで）
  // 2つ＋内側 9×2＋縁 1×2 を3つと、あき 6×2。16px でも 168px で1行に入る（入らなくても折り返す。.skin-actions は flex-wrap）
  assert.equal(280 - 9 * 2 - 1 * 2 - 72 - 10, 178);
  assert.ok(3 * (16 * 2 + 9 * 2 + 1 * 2) + 6 * 2 <= 178);
  assert.ok(rulesFor(base, ".skin-actions").includes("flex-wrap:wrap"));
});

test("v3.12.0：「スキンの立ち絵」の説明は、立ち絵の無いスキンのとき「（いまは無し）」まで全文出す（どのカードの説明も省略記号で切らない）", () => {
  const none = avatarRows({ skins: [], avatars: [], max: 30 }, "Shape Colors", false)[0];
  assert.equal(none.note, "当てているスキンに立ち絵があれば出します（いまは無し）");
  const has = avatarRows({ skins: [], avatars: [], max: 30 }, "ペンギン（見本）", true)[0];
  assert.equal(has.note, "いまは「ペンギン（見本）」の立ち絵");
  // 説明の要素はカードの種類によらず1つで、wrap が付く（種類で分けない）
  const view = read("../src/views/SettingsAvatars.tsx");
  assert.equal((view.match(/className="skin-meta/g) ?? []).length, 1);
  assert.ok(view.includes('<div className="skin-meta wrap">{r.note}</div>'));
});

test("v3.12.0：説明が空なら行を作らない。絵の無い「スキンの立ち絵」の枠は「出さない」と同じ「なし」", () => {
  const view = read("../src/views/SettingsAvatars.tsx");
  // 読めない立ち絵は説明が空 → 説明の要素を出さない（名前と理由の間に空の行を作らない）
  const rows = avatarRows({ skins: [], avatars: [{ id: "00000000000000aa", name: "", preview: null, hasStopped: false, animated: false, broken: "avatar_broken" as const }], max: 30 }, null, false);
  assert.equal(rows[2].note, "");
  assert.ok(view.includes('{r.note ? <div className="skin-meta wrap">{r.note}</div> : null}'));
  // 絵の枠の文字：「出さない」と、立ち絵の無いスキンを当てているときの「スキンの立ち絵」は「なし」。ほかは空
  assert.equal(avatarPreviewText("none"), "なし");
  assert.equal(avatarPreviewText("skin"), "なし");
  assert.equal(avatarPreviewText("skinCharacter"), "");
  assert.equal(avatarPreviewText("avatar"), "");
  assert.ok(view.includes('<span className="avatar-preview-text">{avatarPreviewText(r.kind)}</span>'));
  // 立ち絵のあるスキンのときは、今までどおりその絵（Rust が解いた停止中の静止画）を出す
  assert.ok(view.includes('const src = r.kind === "skin" ? props.skinStill : r.preview;'));
  assert.ok(view.includes("const skinStill = skinChar ? skinChar.stopped.still ?? skinChar.stopped.path : null;"));
});

// ---------------- v3.12.1：立ち絵のカードの説明を短く・高さをそろえる ----------------

test("v3.12.1：スキンの立ち絵のカードの説明は「スキン『◯◯』の立ち絵」だけ。欄の頭の説明は2文で、同じ文を画面に2回出さない", () => {
  const rows = avatarRows({ skins: [{ skinId: "penguin-sample", name: "ペンギン（見本）", preview: null, animated: false }], avatars: [], max: 30 }, null, false);
  const sc = rows.find((r) => r.kind === "skinCharacter")!;
  assert.equal(sc.note, "スキン「ペンギン（見本）」の立ち絵");
  assert.equal(sc.badge, "スキン");
  // カードの文には、どのカードでも同じ文を入れない
  for (const r of rows) assert.ok(!/スキンを替えても|削除すると消え/.test(r.note), r.note);
  // 欄の頭の説明：「スキン」の印のカードが何か・消すときはスキンの一覧からスキンを削除する、の2文だけ（480px で3行まで）
  assert.equal(AVATAR_LEAD, "「スキン」の印のカードは、取り込んだスキンに入っている立ち絵です。消すときは、上のスキンの一覧からそのスキンを削除してください。");
  assert.equal(AVATAR_LEAD.split("。").filter(Boolean).length, 2);
  assert.ok(!/[。、][。、]/.test(AVATAR_LEAD));
  // 3行に収まる長さの目安：1行を少なめに 27 字と見て 81 字まで（480px の窓の画面では1行 33 字前後で、2行に収まる）
  assert.ok([...AVATAR_LEAD].length <= 81, String([...AVATAR_LEAD].length));
  const view = read("../src/views/SettingsAvatars.tsx");
  // 見出しのすぐ下に1回だけ（色・アイコンの欄と同じ .hint-text の形）
  assert.ok(view.includes('<h4 className="skin-subhead">立ち絵</h4>\n      <p className="hint-text">{AVATAR_LEAD}</p>'));
  assert.equal((view.match(/\{AVATAR_LEAD\}/g) ?? []).length, 1);
  // 「スキンを替えても、選んだ立ち絵はそのままです。」は画面に1回だけ（「立ち絵を足す」の説明）。欄の頭の説明には書かない
  const lead = read("../src/skins/avatars.ts");
  const same = "スキンを替えても、選んだ立ち絵はそのままです。";
  assert.equal(view.split(same).length - 1, 1);
  assert.ok(!AVATAR_LEAD.includes(same) && !lead.includes(`"${same}`));
  // 操作の説明（カードを押すと切り替わる）は、欄の読み上げの名前に残す（目に見える説明からは外した）
  assert.ok(!AVATAR_LEAD.includes("カードを押"));
  assert.ok(view.includes('aria-label="使う立ち絵（カードを押すと切り替わります）"'));
});

test("v3.12.1：立ち絵のカードは全部同じ高さ（行は全部 1fr。ふつうは絵の枠 96px が基準）で、ボタンは下の端。印は名前と同じ行", () => {
  const flat = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, "");
  const rulesFor = (css: string, sel: string) =>
    [...flat(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((r) => r[1].split(",").includes(sel)).map((r) => r[2]).join(";");
  for (const css of [read("../src/theme/base.css"), BUILT_CSS]) {
    const g = rulesFor(css, ".avatar-grid");
    // 行は全部同じ高さ（いちばん高いカード）。中身の高さで止めない（3.12.0 の align-items: start はやめた）
    assert.ok(g.includes("grid-auto-rows:1fr"), g);
    assert.ok(!g.includes("align-items:start"), g);
    // カードは行の高さいっぱい（stretch）、右の列も高さいっぱいで、ボタンは下の端
    const c = rulesFor(css, ".skin-card.avatar-card");
    assert.ok(c.includes("align-items:stretch"), c);
    assert.ok(rulesFor(css, ".avatar-card-body").includes("flex:1"));
    assert.ok(rulesFor(css, ".skin-actions").includes("margin-top:auto"));
    // 名前と印は同じ行（入らなければ印を次の行へ）
    const head = rulesFor(css, ".avatar-card-head");
    for (const d of ["display:flex", "flex-wrap:wrap", "align-items:center"]) assert.ok(head.includes(d), `${d}: ${head}`);
    // 絵の枠は 96px（カードの基準の高さ：96 ＋ 内側 9×2 ＋ 縁 1×2 ＝ 116px）
    assert.ok(rulesFor(css, ".avatar-card.avatar-card-preview").includes("height:96px"));
  }
  assert.equal(96 + 9 * 2 + 1 * 2, 116);
  const view = read("../src/views/SettingsAvatars.tsx");
  assert.ok(!view.includes('<div className="skin-tags">'), "the badge sits on the name row");
});
