// Light テーマの色が、文字と背景の濃さの差（WCAG のコントラスト比）の基準を満たすかを確かめる（v3.1.2）。
// v3.7.0：Dark のアクセント・アクセントの上の文字・危険の色を変え、操作部の輪郭（--c-outline）とお気に入りの星（--c-favorite）を両方のテーマに足した。
// v3.8.0：メーターの帯（緑 --c-ok・黄 --c-favorite・赤 --c-danger。新しい色の変数は足さない）を、ミキサー（パネル）とプルダウン（メニュー）の面で足した。
// 走らせ方（プロジェクトのフォルダで。依存は要らない）：
//   node --test scripts/theme-contrast.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { checks, color, evaluate, minUiOpacity, ratio, readTheme, DEFAULT_DIM, DEFAULT_UI } from "./theme-contrast.mts";

const light = fileURLToPath(new URL("../src/theme/light.css", import.meta.url));
const dark = fileURLToPath(new URL("../src/theme/dark.css", import.meta.url));

test("比の計算：白と黒は 21、同じ色は 1、#777777 と白は 4.48", () => {
  assert.equal(ratio([255, 255, 255], [0, 0, 0]).toFixed(2), "21.00");
  assert.equal(ratio([120, 120, 120], [120, 120, 120]), 1);
  assert.equal(ratio([0x77, 0x77, 0x77], [255, 255, 255]).toFixed(2), "4.48");
  // 向きによらない
  assert.equal(ratio([0, 0, 0], [255, 255, 255]), ratio([255, 255, 255], [0, 0, 0]));
});

test("色の読み方：#rrggbb・rgba()・r, g, b", () => {
  assert.deepEqual(color("#4232c2"), { rgb: [0x42, 0x32, 0xc2], a: 1 });
  assert.deepEqual(color("rgba(0, 0, 0, 0.48)"), { rgb: [0, 0, 0], a: 0.48 });
  assert.deepEqual(color("252, 252, 250"), { rgb: [252, 252, 250], a: 1 });
  assert.throws(() => color("red"));
});

test("Light：既定の設定（背景の暗さ 45%・UI の不透明度 70%）で、白・黒の背景画像と画像なしのどれでも基準を満たす", () => {
  const rows = evaluate(light, DEFAULT_DIM, DEFAULT_UI);
  // 場面3つ × 組み合わせ。組み合わせが減っていないこと（数え漏れで通らないように）。v3.8.0 でメーターの帯 6 を足した
  assert.equal(rows.length, 3 * 44); // v3.8.0 で最大の線5・出力ボタンの縁1・出力ボタンの「!」1・遅れを測る小窓の文字3を足した
  const fails = rows.filter((r) => !(r.ratio >= r.min)).map((r) => `${r.scene} ${r.name} ${r.ratio.toFixed(2)} < ${r.min}`);
  assert.deepEqual(fails, []);
});

test("Light：既定の設定より濃い覆い・不透明なパネルなら、どれも満たす（設定を上げて読みにくくならない）", () => {
  for (const dim of [0.45, 0.6, 0.75, 0.95]) {
    for (const ui of [0.7, 0.8, 0.9, 1]) {
      const fails = evaluate(light, dim, ui).filter((r) => !(r.ratio >= r.min));
      assert.equal(fails.length, 0, `dim ${dim} ui ${ui}: ${fails.map((f) => f.name).join(", ")}`);
    }
  }
  // 既定の暗さでは、UI の不透明度 70% より下げても 67% までは満たす
  assert.ok((minUiOpacity(light, DEFAULT_DIM) ?? 1) <= DEFAULT_UI);
});

test("変数の名前は Dark と同じ（足し引きしていない）", () => {
  const names = (p: string) => Object.keys(readTheme(p)).filter((k) => k.startsWith("--c-")).sort();
  assert.deepEqual(names(light), names(dark));
});

test("Dark（v3.7.0）：アクセント #b3aaff・アクセントの上の文字 #08080c・危険 #ff8591。輪郭とお気に入りの星を足した。ほかは 3.1.1 の値のまま", () => {
  assert.deepEqual(readTheme(dark), {
    "--c-bg": "#08080c",
    "--c-panel-rgb": "15, 15, 22",
    "--c-overlay-rgb": "8, 8, 12",
    "--c-primary": "#ffffff",
    "--c-secondary": "#b0b0b8",
    "--c-accent": "#b3aaff",
    "--c-accent-contrast": "#08080c",
    "--c-hover": "rgba(255, 255, 255, 0.07)",
    "--c-selected": "rgba(124, 108, 255, 0.22)",
    "--c-border": "rgba(255, 255, 255, 0.09)",
    "--c-outline": "rgba(255, 255, 255, 0.44)",
    "--c-input": "rgba(255, 255, 255, 0.07)",
    "--c-danger": "#ff8591",
    "--c-ok": "#5fd39a",
    "--c-favorite": "#ffd23f",
  });
});

test("Light（v3.7.0）：足したのは輪郭とお気に入りの星だけ。輪郭は今までの --c-border と同じ値", () => {
  const t = readTheme(light);
  assert.equal(t["--c-outline"], "rgba(0, 0, 0, 0.48)");
  assert.equal(t["--c-outline"], t["--c-border"]);
  assert.equal(t["--c-favorite"], "#755400");
  assert.equal(t["--c-accent"], "#4232c2");
  assert.equal(t["--c-danger"], "#a3132b");
  assert.equal(t["--c-accent-contrast"], "#ffffff");
  // 星の色は、設定の幅を狭めない濃さ（3.6.0 と同じく、暗さ 45% で 67%・30% で 74% まで下げられる。docs\テーマの色の根拠.md 6章）
  assert.equal(minUiOpacity(light, 0.45), 0.67);
  assert.equal(minUiOpacity(light, 0.3), 0.74);
});

test("Dark（v3.7.0）：既定の設定で、届かないのは飾りの区切り線（--c-border）と、3.6.0 から変えていない補助文字／選んだ行（白い背景画像）だけ", () => {
  const fails = evaluate(dark, DEFAULT_DIM, DEFAULT_UI)
    .filter((r) => !(r.ratio >= r.min))
    .map((r) => `${r.scene} ${r.name}`);
  assert.deepEqual(fails.sort(), [
    "明るい背景画像（白） --c-secondary／選んだ行",
    "明るい背景画像（白） 線 --c-border／パネル",
    "明るい背景画像（白） 線 --c-border／入力欄の中",
    "暗い背景画像（黒） 線 --c-border／パネル",
    "暗い背景画像（黒） 線 --c-border／入力欄の中",
    "背景画像なし 線 --c-border／パネル",
    "背景画像なし 線 --c-border／入力欄の中",
  ].sort());
});

test("v3.7.0 で足した決まり：アクセントの文字 4.5・危険 4.5・アクセントの上の文字 4.5・操作部の輪郭 3・お気に入りの星 3 を、両方のテーマの3つの場面で満たす", () => {
  for (const f of [dark, light]) {
    const rows = evaluate(f, DEFAULT_DIM, DEFAULT_UI).filter((r) => /--c-accent|--c-danger|--c-outline|--c-favorite/.test(r.name));
    // 名前で拾った数（場面3つ × アクセント文字5・線1・上の文字1・危険2・輪郭2・星3 ＋ v3.8.0 メーターの黄と赤の帯 4）
    // （最大の線の縁と黄・赤の帯 2、出力ボタンの危険の色の縁 1 を足した。で小窓の危険の文字 1 を足した）
    assert.equal(rows.length, 3 * 22, f);
    const fails = rows.filter((r) => !(r.ratio >= r.min)).map((r) => `${r.scene} ${r.name} ${r.ratio.toFixed(2)}`);
    assert.deepEqual(fails, [], f);
    // 星と輪郭は 3:1、文字は 4.5:1 で判定している（基準を取り違えていない）
    for (const r of rows) assert.equal(r.min, /--c-outline|--c-favorite|線 --c-accent|線 --c-danger|^メーター/.test(r.name) ? 3 : 4.5, r.name);
  }
});

test("v3.7.0：3.6.0 の Dark（#7c6cff・#ffffff・#ff6b7a）では、明るい背景画像の上でアクセントの文字と危険の文字が 4.5 に届かなかった（変えた理由）", () => {
  const old = { ...readTheme(dark), "--c-accent": "#7c6cff", "--c-accent-contrast": "#ffffff", "--c-danger": "#ff6b7a" };
  const r = (name: string) => {
    const t = { ...old };
    return Math.min(
      ...[0, 1, 2].map((i) => {
        const s = { name: "", image: [[255, 255, 255], [0, 0, 0], null][i] as any, dim: i === 2 ? 0 : DEFAULT_DIM, ui: DEFAULT_UI };
        const c = checks(t, s).find((x) => x.name === name)!;
        return ratio(c.fg, c.bg);
      }),
    );
  };
  assert.ok(r("--c-accent（文字）／パネル") < 4.5);
  assert.ok(r("--c-danger／パネル") < 4.5);
  assert.ok(r("--c-accent-contrast／--c-accent") < 4.5);
});
