// 着せ替え（v3.2.0）：取り込みの確認画面に出す skin.json の警告の並べ方。
// Rust（skins/manifest.rs の Ctx::warn）が、種類（code）ごとに WARNINGS_PER_CODE 件までにして、超えた分は種類ごとの数だけ送る。
// ここは、種類ごとにまとめて出た順に並べ、上限を超えた分を、その種類の最後に「ほか N 件」の1行で出すだけ（上限の数はここに写さない）。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。

export interface WarningLike {
  code: string;
  detail: string;
}

export interface OmittedLike {
  code: string;
  count: number;
}

/** 種類ごとにまとめた行（出た順）。上限を超えた分は `detail` が「ほか N 件」の行 */
export function warningRows(warnings: WarningLike[], omitted: OmittedLike[]): WarningLike[] {
  const order: string[] = [];
  const byCode = new Map<string, WarningLike[]>();
  for (const w of warnings) {
    if (!byCode.has(w.code)) {
      byCode.set(w.code, []);
      order.push(w.code);
    }
    byCode.get(w.code)!.push(w);
  }
  for (const o of omitted) {
    if (!byCode.has(o.code)) {
      byCode.set(o.code, []);
      order.push(o.code);
    }
  }
  const out: WarningLike[] = [];
  for (const code of order) {
    out.push(...byCode.get(code)!);
    const more = omitted.filter((o) => o.code === code).reduce((n, o) => n + o.count, 0);
    if (more > 0) out.push({ code, detail: `ほか ${more} 件` });
  }
  return out;
}
