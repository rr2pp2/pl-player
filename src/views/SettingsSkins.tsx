// 設定の「着せ替え」の欄（v3.2.0）：スキンの一覧・見本・適用・取り込み・書き出し・削除・既定に戻す。
// v3.9.0：立ち絵（使う立ち絵の一覧・「＋ 立ち絵を追加…」・変更・大きさ）は SettingsAvatars.tsx の「立ち絵」の欄（この欄の下に出す）。
import { useEffect, useState } from "react";
import { api } from "../ipc/commands";
import { fileUrl } from "../ipc/events";
import type { SkinSummary } from "../ipc/types";
import { errorText, skinPartLabels } from "../messages/ja";
import { notify } from "../stores/notices";
import { settings, updateSettings } from "../stores/settings";
import { applySkin, deleteSkin, exitSafeMode, exportSkin, hideAvatar, pickSkinZip, resetSkin, skinListVersion, skinPicking, skinState } from "../stores/skin";
import { useStore } from "../stores/store";
import { confirmDialog } from "../components/Modal";
import { SkinImportReview } from "./SkinImportReview";
import { AvatarsField } from "./SettingsAvatars";

const BUILTIN = "builtin:default";

function Preview(props: { x: SkinSummary }) {
  const url = fileUrl(props.x.preview);
  const [bad, setBad] = useState(false);
  return (
    <div className={"skin-preview" + (props.x.builtin ? " default" : "")} aria-hidden="true">
      {url && !bad ? <img src={url} alt="" draggable={false} onError={() => setBad(true)} /> : null}
    </div>
  );
}

export function SkinsSection() {
  const st = useStore(skinState);
  const s = useStore(settings);
  const ver = useStore(skinListVersion);
  const picking = useStore(skinPicking);
  const [list, setList] = useState<SkinSummary[]>([]);
  const effective = st?.effectiveId ?? BUILTIN;
  const active = st?.activeId ?? BUILTIN;
  useEffect(() => {
    api.skinsList().then(setList).catch((e) => notify({ level: "error", error: e }));
  }, [ver, effective, active]);
  const current = list.find((x) => x.id === effective);
  const saved = list.find((x) => x.id === active);
  const name = (x: SkinSummary | undefined) => (!x || x.builtin ? "既定のスキン" : x.name);
  const nextDefault = s.skin?.safeModeNextStart === true;

  const del = async (x: SkinSummary) => {
    const using = x.id === active || x.id === effective;
    const ok = await confirmDialog(
      "スキンを削除",
      `「${x.name}」を削除します。${using ? "使っているスキンなので、先に既定の見た目に戻します。" : ""}元に戻せません（zip が手元にあれば、取り込み直せます）。`,
      "削除",
      true,
    );
    if (ok) await deleteSkin(x.id);
  };

  return (
    <section>
      <h3>着せ替え</h3>
      <p className="hint-text skin-lead">アプリの見た目（色と文字・ボタンのアイコン・アプリのアイコン・立ち絵）を変えます。立ち絵は下の「立ち絵」の欄で、スキンとは別に選べます（「＋ 立ち絵を追加…」で画像を選んで足すこともできます）。スキンと立ち絵はこの PC の中に置き、外へは通信しません。</p>
      {st?.safeMode ? (
        <div className="skin-callout warn" role="status">
          <div>既定の見た目で動いています（セーフモード）。保存しているスキン：{name(saved)}</div>
          <div className="row" style={{ marginTop: 6 }}>
            {active !== BUILTIN ? (
              <button className="btn" onClick={() => void exitSafeMode()}>
                保存しているスキンに戻す
              </button>
            ) : null}
            <button className="btn" onClick={() => void resetSkin()}>
              既定の見た目のままにする
            </button>
          </div>
        </div>
      ) : null}
      <div className="field">
        <span className="flabel">今のスキン</span>
        <span className="row">
          <strong>{name(current)}</strong>
          {current && !current.builtin
            ? skinPartLabels(current.parts).map((p) => (
                <span key={p} className="skin-tag">
                  {p}
                </span>
              ))
            : null}
        </span>
      </div>
      <div className="skin-grid">
        {list.map((x) => {
          const on = x.id === effective;
          return (
            <div key={x.id} className={"skin-card" + (on ? " active" : "") + (x.broken ? " broken" : "")}>
              <Preview x={x} />
              <div className="skin-name" title={x.builtin ? undefined : x.id}>
                {name(x)}
              </div>
              <div className="skin-meta">{x.builtin ? "PL Player 標準・組み込み" : [x.author, x.version].filter(Boolean).join(" · ")}</div>
              <div className="skin-tags">
                {x.builtin ? <span className="skin-tag">削除不可</span> : skinPartLabels(x.parts).map((p) => <span key={p} className="skin-tag">{p}</span>)}
                {x.id === active && !on ? <span className="skin-tag">保存中</span> : null}
              </div>
              {x.broken ? <div className="error-text">読めません：{errorText[x.broken] ?? x.broken}</div> : null}
              <div className="skin-actions">
                {x.broken ? null : on ? (
                  <button className="btn primary" disabled>
                    適用中
                  </button>
                ) : (
                  <button className="btn" onClick={() => void applySkin(x.id)}>
                    適用
                  </button>
                )}
                {!x.builtin && !x.broken ? (
                  <button className="btn" onClick={() => void exportSkin(x.id)}>
                    書き出す…
                  </button>
                ) : null}
                {!x.builtin ? (
                  <button className="btn danger" onClick={() => void del(x)}>
                    削除
                  </button>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
      <div className="field">
        <span className="flabel">スキンを足す・戻す</span>
        <span className="row">
          <button className="btn primary" disabled={picking} aria-busy={picking} onClick={() => void pickSkinZip()}>
            {picking ? "確かめています…" : "＋ zip を取り込む…"}
          </button>
          <button className="btn" disabled={effective === BUILTIN && active === BUILTIN} onClick={() => void resetSkin()}>
            既定の見た目に戻す
          </button>
          {/* v3.9.0（直し1回目）：立ち絵だけを外す（作った立ち絵は消さない）。「既定の見た目に戻す」では立ち絵は残るため */}
          <button className="btn" disabled={(st?.avatar ?? "skin") === "none"} onClick={() => void hideAvatar()}>
            立ち絵を出さない
          </button>
        </span>
        <span className="hint">
          取り込むときは、中身を全部確かめてから確認の画面を出します。取り込んだだけでは当てません（一覧の「適用」で当てます）。「既定の見た目に戻す」はスキンだけを戻し、立ち絵は残ります。立ち絵も外すときは「立ち絵を出さない」を押してください（作った立ち絵は消えません。下の「立ち絵」の欄の「使う」で戻せます）。
        </span>
      </div>
      <div className="field">
        <span className="flabel">次の起動</span>
        <label className="row" style={{ gap: 6 }}>
          <input
            type="checkbox"
            checked={nextDefault}
            onChange={(e) => updateSettings((x) => ({ ...x, skin: { ...x.skin, safeModeNextStart: e.target.checked } }))}
          />
          次の起動を既定の見た目にする（1回だけ。保存しているスキンは変えません）
        </label>
        <span className="hint">
          画面が読めなくなったときの戻し方：タスクトレイのアイコンを右クリック →「既定の見た目に戻す」（立ち絵は残ります。立ち絵も外すときは、同じメニューの「立ち絵を出さない」）。または、起動の指定 --safe-mode を付けて起動する（起動中に付けて起動しても、動いているアプリが既定の見た目に切り替わります）。
        </span>
      </div>
      <div className="skin-callout">
        スキンで変わるのは、起動中のウィンドウ・タスクバー・タスクトレイのアイコンです。exe・スタートメニュー・ショートカットのアイコンは変わりません（変えたいときは、ショートカットのプロパティの「アイコンの変更」から指定してください）。
      </div>
      <AvatarsField />
      <SkinImportReview />
    </section>
  );
}
