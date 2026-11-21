// 既定のテーマの色を、画面の CSS から読む（dark.css・light.css が正。値をここに写さない）。
// スキンの取り込みの確認画面（v3.2.0）と「色」の欄（v3.10.0）の読みやすさの警告・今の色の見本に使う。
// 一時の要素に data-theme を付けて読むので、<html> に当てたスキン・手の色の値は混ざらない（その要素のテーマの値が勝つ）。
// v3.10.0：ビルドした CSS では半透明の色が `#ffffff12` のような8桁の16進に書き換わっている（CSS を小さくする道具のため）。
// そのまま渡すと色の読み取りが投げ、警告が黙って0件になっていた（3.9.1 以前の取り込みの確認画面も）。readThemeColors で今までの形にそろえる
import { COLOR_VARS, readThemeColors } from "./tokens";

export function themeDefaults(theme: string): { colors: Record<string, string>; unreadable: string[] } {
  const el = document.createElement("div");
  el.dataset.theme = theme;
  el.style.display = "none";
  document.body.appendChild(el);
  const cs = getComputedStyle(el);
  const raw: Record<string, string> = {};
  for (const v of COLOR_VARS) raw[v] = cs.getPropertyValue(v).trim();
  el.remove();
  const r = readThemeColors(raw);
  if (r.unreadable.length > 0) console.warn("theme colors could not be read:", r.unreadable.map((n) => `${n}=${raw[n]}`).join(", "));
  return r;
}
