// Light テーマの色が、文字と背景の濃さの差（WCAG のコントラスト比）の基準を満たすかを確かめる（v3.1.2）。
// 走らせ方（プロジェクトのフォルダで。依存は要らない）：
//   node --test scripts/theme-contrast.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { color, evaluate, minUiOpacity, ratio, readTheme, DEFAULT_DIM, DEFAULT_UI } from "./theme-contrast.mts";

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
  // 場面3つ × 組み合わせ。組み合わせが減っていないこと（数え漏れで通らないように）
  assert.equal(rows.length, 3 * 23);
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

test("Dark は変えていない（3.1.1 の値のまま）", () => {
  assert.deepEqual(readTheme(dark), {
    "--c-bg": "#08080c",
    "--c-panel-rgb": "15, 15, 22",
    "--c-overlay-rgb": "8, 8, 12",
    "--c-primary": "#ffffff",
    "--c-secondary": "#b0b0b8",
    "--c-accent": "#7c6cff",
    "--c-accent-contrast": "#ffffff",
    "--c-hover": "rgba(255, 255, 255, 0.07)",
    "--c-selected": "rgba(124, 108, 255, 0.22)",
    "--c-border": "rgba(255, 255, 255, 0.09)",
    "--c-input": "rgba(255, 255, 255, 0.07)",
    "--c-danger": "#ff6b7a",
    "--c-ok": "#5fd39a",
  });
});
