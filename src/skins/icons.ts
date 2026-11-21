// 着せ替え（v3.2.0）：ボタンのアイコンの状態と、スキンの画像の選び方。
// 状態の切り替え点は、今の SVG（src\components\Icons.tsx）と同じ。画像が無ければ null（呼び出し側は今の SVG を出す）。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

/** 音量：消音・0・小・大（IconVolume の線の数と同じ。0.01 と 0.5 で切り替える） */
export function volumeState(muted: boolean | undefined, level: number | undefined): "muted" | "zero" | "low" | "high" {
  if (muted) return "muted";
  const l = level ?? 1;
  if (l <= 0.01) return "zero";
  if (l <= 0.5) return "low";
  return "high";
}

/** お気に入り：塗り・線 */
export const starState = (filled: boolean | undefined): "filled" | "outline" => (filled ? "filled" : "outline");

/** リピート：切・全曲・1曲。mode が無い（一覧の見出しのボタンなど）ときは、1曲かどうかだけで決める */
export function repeatState(mode: string | undefined, one: boolean | undefined): string {
  if (mode === "off" || mode === "all" || mode === "one") return mode;
  return one ? "one" : "default";
}

/** シャッフル：入・切。分からないときは default */
export const shuffleState = (on: boolean | undefined): string => (on === undefined ? "default" : on ? "on" : "off");

export interface AssetLike {
  path: string;
  still: string | null;
}
export type IconTable = Record<string, Record<string, { dark: AssetLike | null; light: AssetLike | null }>>;

/**
 * 出す画像のパス。状態の画像 → default の画像 → 無し（null）の順。
 * テーマの画像は今のテーマ → もう片方。「動きを減らす」のときは、動く画像の最初の1コマ（still）だけを使う（動く本体は読まない）
 */
export function pickIconPath(icons: IconTable | null | undefined, name: string, state: string, theme: string, reducedMotion: boolean): string | null {
  const states = icons?.[name];
  if (!states) return null;
  const t = states[state] ?? states.default;
  if (!t) return null;
  const a = theme === "light" ? t.light ?? t.dark : t.dark ?? t.light;
  if (!a) return null;
  return reducedMotion && a.still ? a.still : a.path;
}

/** v3.11.0：手で入れたアイコン（Rust が確かめたもの。Dark・Light 共通） */
export interface ManualIconLike {
  path: string;
  still: string | null;
  rev: string;
}
export type ManualIconTable = Record<string, Record<string, ManualIconLike>>;

/**
 * v3.11.0：出す画像。重なり順は 今の SVG（null）< スキンのアイコン < 手で入れたアイコン。
 * 手のアイコンはその状態の枠だけを見る（状態のあるアイコンに default の枠は無い）。無ければスキンの選び方（pickIconPath）。
 * rev は手のアイコンのときだけ（ファイルの名前が同じまま入れ直すので、URL に付けて読み直す）。
 * セーフモードの間は、呼び出し側が手のアイコンもスキンも渡さない
 */
export function pickLayeredIcon(
  manual: ManualIconTable | null | undefined,
  icons: IconTable | null | undefined,
  name: string,
  state: string,
  theme: string,
  reducedMotion: boolean,
): { path: string; rev: string | null } | null {
  const m = manual?.[name]?.[state];
  if (m) return { path: reducedMotion && m.still ? m.still : m.path, rev: m.rev };
  const p = pickIconPath(icons, name, state, theme, reducedMotion);
  return p ? { path: p, rev: null } : null;
}

/** URL に入れ直しの印を付ける（asset プロトコルは問い合わせの部分を読まないので、ファイルはそのまま。画面の読み直しだけが起きる） */
export function withRev(url: string | null, rev: string | null): string | null {
  if (!url) return null;
  return rev ? `${url}?v=${encodeURIComponent(rev)}` : url;
}
