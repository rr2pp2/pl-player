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
