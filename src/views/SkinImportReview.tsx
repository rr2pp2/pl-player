// 着せ替え（v3.2.0）：zip の取り込みの確認画面。Rust が一時の場所で全部確かめた結果を見せる。
// 「このスキンを取り込む」を押すまで、正式の場所には入らない。取り込んでも当てない（当てるのは一覧の「適用」）。
// 色の読みやすさ（WCAG のコントラスト比）は、ここで計算して警告だけ出す（警告があっても取り込める）。
import { useMemo } from "react";
import type { SkinAppearance, SkinInspection } from "../ipc/types";
import { errorText, skinPartLabels, skinWarning } from "../messages/ja";
import { contrastWarnings, type Result } from "../skins/contrast";
import { COLOR_VARS } from "../skins/tokens";
import { warningRows } from "../skins/warnings";
import { cancelSkinImport, commitSkinImport, skinImport } from "../stores/skin";
import { useStore } from "../stores/store";
import { Modal } from "../components/Modal";

const mb = (n: number) => (n >= 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);

/** 既定のテーマの色を、画面の CSS から読む（dark.css・light.css が正。値をここに写さない） */
function themeDefaults(theme: string): Record<string, string> {
  const el = document.createElement("div");
  el.dataset.theme = theme;
  el.style.display = "none";
  document.body.appendChild(el);
  const cs = getComputedStyle(el);
  const out: Record<string, string> = {};
  for (const v of COLOR_VARS) out[v] = cs.getPropertyValue(v).trim();
  el.remove();
  return out;
}

function contrastByTheme(a: SkinAppearance | null): { theme: string; rows: Result[] }[] {
  if (!a) return [];
  const out: { theme: string; rows: Result[] }[] = [];
  for (const theme of ["dark", "light"] as const) {
    const t = a[theme];
    if (!t || Object.keys(t.tokens).length === 0) continue;
    try {
      out.push({ theme, rows: contrastWarnings(themeDefaults(theme), t.tokens) });
    } catch {
      out.push({ theme, rows: [] });
    }
  }
  return out;
}

function Body(props: { r: SkinInspection }) {
  const r = props.r;
  const contrast = useMemo(() => contrastByTheme(r.appearance), [r]);
  const images = r.files.filter((f) => f.width > 0);
  const largest = images.reduce((m, f) => (f.width * f.height > m.width * m.height ? f : m), { width: 0, height: 0 } as { width: number; height: number });
  const anims = images.filter((f) => f.animated);
  const dirs: [string, string][] = [
    ["icons/", "アイコン"],
    ["app/", "アプリのアイコン"],
    ["character/", "立ち絵"],
  ];
  const parts = skinPartLabels(r.parts);
  return (
    <div className="skin-review">
      <div className="skin-steps" aria-label="手順">
        <span className="skin-step">1 zip を選ぶ</span>
        <span className="skin-step active">2 中身を確かめる</span>
        <span className="skin-step">3 取り込み</span>
      </div>
      <div className="skin-review-summary">
        <div className="skin-review-preview">{r.previewDataUrl ? <img src={r.previewDataUrl} alt="" draggable={false} /> : <span className="hint-text">見本なし</span>}</div>
        <div className="skin-review-head">
          <div className="skin-review-name">{r.name}</div>
          <div className="hint-text">
            {[r.author ? `作者：${r.author}` : "", `版：${r.version}`, `id：${r.id}`].filter(Boolean).join("　/　")}
          </div>
          {r.description ? <p className="skin-review-desc">{r.description}</p> : null}
          <div className="ok-text">skin.json・画像・動く画像の確かめに合格しました</div>
          {r.unwrapped ? <div className="hint-text">zip の中のフォルダ1つを外して読みました（フォルダごと圧縮した zip）</div> : null}
          <div className="skin-tags">
            {parts.length ? parts.map((p) => <span key={p} className="skin-tag">{p}</span>) : <span className="skin-tag">中身なし</span>}
          </div>
        </div>
      </div>
      <div className="skin-review-cols">
        <div>
          <h4>ファイル</h4>
          <div className="skin-file-list">
            <div className="skin-file-row">
              <span>skin.json</span>
              <span>確かめ済み</span>
            </div>
            {r.files.some((f) => f.path.startsWith("preview.")) ? (
              <div className="skin-file-row">
                <span>見本（preview）</span>
                <span>1 枚</span>
              </div>
            ) : null}
            {dirs.map(([d, label]) => {
              const n = r.files.filter((f) => f.path.startsWith(d));
              if (n.length === 0) return null;
              const a = n.filter((f) => f.animated).length;
              return (
                <div className="skin-file-row" key={d}>
                  <span>{label}（{d}）</span>
                  <span>
                    {n.length} 枚{a ? `・動く画像 ${a}` : ""}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
        <div>
          <h4>大きさ・確かめた結果</h4>
          <div className="skin-file-list">
            <div className="skin-file-row">
              <span>zip</span>
              <strong>{mb(r.zipBytes)}</strong>
            </div>
            <div className="skin-file-row">
              <span>展開後</span>
              <strong>{mb(r.unpackedBytes)}</strong>
            </div>
            <div className="skin-file-row">
              <span>ファイルの数</span>
              <strong>
                {r.fileCount} / {r.maxFiles}
              </strong>
            </div>
            {largest.width ? (
              <div className="skin-file-row">
                <span>いちばん大きい画像</span>
                <strong>
                  {largest.width}×{largest.height}
                </strong>
              </div>
            ) : null}
            {anims.length ? (
              <div className="skin-file-row">
                <span>動く画像</span>
                <strong>
                  最大 {Math.max(...anims.map((f) => f.frames))} コマ / {(Math.max(...anims.map((f) => f.durationMs)) / 1000).toFixed(1)} 秒
                </strong>
              </div>
            ) : null}
            {r.ignored.length ? (
              <div className="skin-file-row">
                <span>無視したファイル（OS が足すもの。取り込みません）</span>
                <span>{r.ignored.map((g) => `${g.name} ${g.count}`).join("・")}</span>
              </div>
            ) : null}
            <div className="skin-file-row">
              <span>実行ファイル・外への通信</span>
              <strong className="ok-text">なし</strong>
            </div>
          </div>
        </div>
      </div>
      {r.conflict ? <div className="skin-callout bad">{errorText.skin_id_exists}（id：{r.id}）</div> : null}
      {r.warnings.length || r.warningsOmitted.length ? (
        <div className="skin-callout warn">
          <div>知らせること</div>
          <ul>
            {warningRows(r.warnings, r.warningsOmitted).map((w, i) => (
              <li key={i}>
                {skinWarning(w.code)}
                {w.detail ? <span className="hint-text">（{w.detail}）</span> : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {contrast.map((c) =>
        c.rows.length ? (
          <div className="skin-callout warn" key={c.theme}>
            <div>
              {c.theme === "light" ? "Light" : "Dark"} テーマの色で、読みにくくなる組み合わせが {c.rows.length} 件あります（本文 4.5:1・線 3:1 に届かない。背景の暗さ 45%・UI の不透明度 70% で計算）。取り込むことはできます。
            </div>
            <ul>
              {c.rows.slice(0, 5).map((x, i) => (
                <li key={i}>
                  {x.scene}：{x.name} {x.ratio.toFixed(2)}:1
                </li>
              ))}
              {c.rows.length > 5 ? <li>ほか {c.rows.length - 5} 件</li> : null}
            </ul>
          </div>
        ) : (
          <div className="ok-text" key={c.theme}>
            {c.theme === "light" ? "Light" : "Dark"} テーマの色：読みやすさの基準を満たしています（既定の設定で計算）
          </div>
        ),
      )}
      <p className="hint-text">取り込んでも、すぐには当てません。当てるときは、一覧のスキンの「適用」を押してください。</p>
    </div>
  );
}

export function SkinImportReview() {
  const r = useStore(skinImport);
  if (!r) return null;
  return (
    <Modal
      title="スキンを取り込む"
      onClose={() => void cancelSkinImport()}
      footer={
        <>
          <button className="btn" onClick={() => void cancelSkinImport()}>
            キャンセル
          </button>
          <button className="btn primary" disabled={r.conflict} onClick={() => void commitSkinImport()}>
            このスキンを取り込む
          </button>
        </>
      }
    >
      <Body r={r} />
    </Modal>
  );
}
