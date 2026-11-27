// 設定の「着せ替え」の中の「立ち絵」の欄（v3.9.0）：使う立ち絵の一覧（スキンの立ち絵・出さない・取り込んだスキンの立ち絵・追加した立ち絵）・
// カードを押すか「使う」で切り替え・「変更」・「削除」・「＋ 立ち絵を追加…」・大きさ。3.8.1 までの「立ち絵を出す」のチェックは、一覧の「出さない」にまとめた。
// 画像は Rust がファイルを選ぶ画面を出して確かめ、見本（data: URL）と印 token だけを返す（画面はパスを持たない）。
// 名前は空でよい（Rust がファイル名から作る）・「変更」の小窓（差し替え・停止中を外す・名前）・カード全体を押せる。
import { useEffect, useState } from "react";
import { fileUrl } from "../ipc/events";
import type { AvatarSummary, PickedImage } from "../ipc/types";
import { errorText } from "../messages/ja";
import { lyricsRowOn, settings, updateSettings } from "../stores/settings";
import {
  avatarList,
  cancelAvatarPick,
  chooseAvatar,
  createAvatar,
  deleteAvatar,
  loadAvatars,
  pickAvatarImage,
  skinListVersion,
  skinState,
  updateAvatar,
} from "../stores/skin";
import { useStore } from "../stores/store";
import { confirmDialog, Modal } from "../components/Modal";
import { characterWindowNote, MIN_LIST_SIDE_PX, RAIL_MAX_PX, RAIL_MIN_PX, RAIL_WINDOW_RATIO, SCALE_MAX, SCALE_MIN } from "../skins/character";
import { characterPaneWidth, useLayoutSize } from "../components/useLayoutSize";
import { characterMinListOf, useCharacterPaneFor, useDetailLayout } from "../layout/useDetailLayout";
import { DETAIL } from "../layout/detailLayout";
import {
  AVATAR_NAME_MAX,
  avatarCardLabel,
  avatarNameOk,
  avatarPreviewText,
  avatarRows,
  canCreateAvatar,
  cardClickSwitches,
  cardKeySwitches,
  CHOICE_SKIN,
  type AvatarRow,
} from "../skins/avatars";

function ScaleSlider() {
  const s = useStore(settings);
  const [local, setLocal] = useState<number | null>(null);
  const v = local ?? s.skin?.characterScale ?? 1;
  const pct = ((v - SCALE_MIN) / (SCALE_MAX - SCALE_MIN)) * 100;
  const commit = () => {
    if (local !== null) void updateSettings((x) => ({ ...x, skin: { ...x.skin, characterScale: local } }));
    setLocal(null);
  };
  return (
    <span className="row" style={{ flexWrap: "nowrap" }}>
      <input
        type="range"
        className="range"
        aria-label="立ち絵の大きさ"
        min={SCALE_MIN}
        max={SCALE_MAX}
        step={0.05}
        value={v}
        style={{ ["--pct" as any]: `${pct}%`, maxWidth: 320 }}
        onChange={(e) => {
          const nv = Number(e.target.value);
          setLocal(nv);
          // 見た目はすぐ変える（保存は手を離したとき）
          settings.set((x) => ({ ...x, skin: { ...x.skin, characterScale: nv } }));
        }}
        onPointerUp={commit}
        onKeyUp={commit}
      />
      <span style={{ width: 48, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{Math.round(v * 100)}%</span>
    </span>
  );
}

/**
 * 大きさのつまみの下：この大きさで立ち絵が出る窓の幅と、今の窓で出せるか（窓の幅で決める形に合わせた）。
 * どの立ち絵でも同じ決まり。「今の窓で出せるか」は AppShell が実際に使った曲一覧の側の幅で決める（詳細の表示でも食い違わない）。
 * 出せないときは文で書き、印（「隠しています」）も文字で付ける（色だけで伝えない）
 */
function WindowNote() {
  const st = useStore(skinState);
  const s = useStore(settings);
  const size = useLayoutSize();
  const pane = useStore(characterPaneWidth);
  // 詳細の表示か（境目が 360px になる）。AppShell と同じ入力で詳細の配置を求める
  const D = useDetailLayout(size, s, s.ui.sidebarCollapsed || size.sidebarAuto, lyricsRowOn(s));
  const paneFor = useCharacterPaneFor(size, s, lyricsRowOn(s));
  const note = characterWindowNote(!!st?.character, s.skin?.characterScale ?? 1, size.width, pane, paneFor, characterMinListOf(D), !!D);
  if (!note) return <span className="hint" role="status">今は立ち絵を出していません（「立ち絵」の欄で選んだものを出します）。</span>;
  return (
    <span className={"hint" + (note.fits ? "" : " avatar-window-note narrow")} role="status" data-fits={note.fits}>
      {note.fits ? null : <strong>【隠しています】</strong>}
      {note.text}
    </span>
  );
}

function RowPreview(props: { row: AvatarRow; skinStill: string | null }) {
  const r = props.row;
  const src = r.kind === "skin" ? props.skinStill : r.preview;
  const url = fileUrl(src);
  const [bad, setBad] = useState(false);
  return (
    <div className="skin-preview avatar-card-preview" aria-hidden="true">
      {url && !bad ? (
        <img src={url} alt="" draggable={false} onError={() => setBad(true)} />
      ) : (
        <span className="avatar-preview-text">{avatarPreviewText(r.kind)}</span>
      )}
    </div>
  );
}

const describe = (p: PickedImage) =>
  `${p.width}×${p.height}px` + (p.animated ? `・動く画像 ${p.frames} コマ・${(p.durationMs / 1000).toFixed(1)} 秒` : "") + `・${Math.max(1, Math.round(p.bytes / 1024))} KB`;

/** 1つの絵の行（見本・説明・ボタン）。見本は選んだ画像の data: URL か、今の絵（Rust が確かめた場所のパス） */
function ImageSlot(props: { label: string; image: string | null; note: string; emptyText: string; children: any }) {
  return (
    <div className="field field-top">
      <span className="flabel">{props.label}</span>
      <span className="row" style={{ alignItems: "flex-start" }}>
        <span className="skin-preview avatar-pick-preview" aria-hidden="true">
          {props.image ? <img src={props.image} alt="" draggable={false} /> : <span className="avatar-preview-text">{props.emptyText}</span>}
        </span>
        <span style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 0 }}>
          <span className="hint-text">{props.note}</span>
          <span className="row">{props.children}</span>
        </span>
      </span>
    </div>
  );
}

/** 選ぶ・確かめる（追加と変更で同じ pick と印。小窓を閉じたら Rust が持つ選んだ画像を捨てる＝世代が進み、あとから届いたものも捨てる） */
function usePick() {
  const [busy, setBusy] = useState(false);
  useEffect(() => () => void cancelAvatarPick(), []);
  const pick = async (slot: "playing" | "stopped"): Promise<PickedImage | null> => {
    setBusy(true);
    try {
      return await pickAvatarImage(slot);
    } finally {
      setBusy(false);
    }
  };
  return { busy, setBusy, pick };
}

const NAME_HINT = `1〜${AVATAR_NAME_MAX} 文字`;

/** 「＋ 立ち絵を追加…」の小窓：再生中を選ぶ → 停止中を選ぶ（なしでよい）→ 名前（空ならファイル名）→ 追加 */
function CreateDialog(props: { onClose: () => void }) {
  const [playing, setPlaying] = useState<PickedImage | null>(null);
  const [stopped, setStopped] = useState<PickedImage | null>(null);
  const [name, setName] = useState("");
  const { busy, setBusy, pick } = usePick();
  const choose = async (slot: "playing" | "stopped") => {
    const p = await pick(slot);
    if (!p) return;
    if (slot === "playing") {
      // 名前がまだ空か、前に入れた既定のままなら、ファイル名から作った名前を入れ直す（Rust の name_from_file と同じ値）
      if (!name.trim() || name === playing?.suggestedName) setName(p.suggestedName);
      setPlaying(p);
    } else {
      setStopped(p);
    }
  };
  const ready = !!playing && avatarNameOk(name) && !busy;
  const make = async () => {
    if (!playing || !ready) return;
    setBusy(true);
    const ok = await createAvatar(name.trim(), playing.token, stopped?.token ?? null);
    setBusy(false);
    if (ok) props.onClose();
  };
  return (
    <Modal
      title="立ち絵を追加"
      onClose={props.onClose}
      footer={
        <>
          <button className="btn" onClick={props.onClose}>
            キャンセル
          </button>
          <button className="btn primary" disabled={!ready} aria-busy={busy} onClick={() => void make()}>
            追加
          </button>
        </>
      }
    >
      <div className="skin-steps" aria-label="手順">
        <span className={"skin-step" + (!playing ? " active" : "")}>1 再生中の絵を選ぶ</span>
        <span className="skin-step">2 停止中の絵を選ぶ（なしでよい）</span>
        <span className="skin-step">3 名前（空ならファイル名）</span>
      </div>
      <ImageSlot label="再生中の絵（必須）" image={playing?.previewDataUrl ?? null} note={playing ? describe(playing) : "まだ選んでいません"} emptyText="">
        <button className="btn" disabled={busy} aria-busy={busy} onClick={() => void choose("playing")}>
          {playing ? "選び直す…" : "画像を選ぶ…"}
        </button>
      </ImageSlot>
      <ImageSlot label="停止中の絵" image={stopped?.previewDataUrl ?? null} note={stopped ? describe(stopped) : "まだ選んでいません（止めている間も再生中の絵）"} emptyText={playing ? "再生中と同じ" : ""}>
        <button className="btn" disabled={busy} aria-busy={busy} onClick={() => void choose("stopped")}>
          {stopped ? "選び直す…" : "画像を選ぶ…"}
        </button>
        {stopped ? (
          <button className="btn" disabled={busy} onClick={() => setStopped(null)}>
            なしにする
          </button>
        ) : null}
      </ImageSlot>
      <div className="field">
        <span className="flabel">名前</span>
        <input
          className="input"
          value={name}
          maxLength={AVATAR_NAME_MAX * 2}
          aria-label="立ち絵の名前"
          placeholder={`${NAME_HINT}（空なら再生中の絵のファイル名）`}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && ready) void make();
          }}
        />
        <span className="hint">
          使える画像は PNG・WebP・GIF（動く画像も）。1つ 15MB・縦横 4096px・120 コマ・90 秒まで（スキンの立ち絵と同じ決まり）。画像はアプリのデータのフォルダへ写して置きます（元のファイルは使い続けません）。停止中の絵を選ばなければ、止めている間も再生中の絵を出します。名前を空にすると、再生中の絵のファイル名を名前にします。
        </span>
      </div>
    </Modal>
  );
}

/** 「変更」の小窓：今の再生中・停止中の絵と名前を見せ、再生中だけ／停止中だけを差し替える・停止中を外す・名前を変える */
function EditDialog(props: { avatar: AvatarSummary; onClose: () => void }) {
  const a = props.avatar;
  const [playing, setPlaying] = useState<PickedImage | null>(null);
  const [stopped, setStopped] = useState<PickedImage | null>(null);
  const [removeStopped, setRemoveStopped] = useState(false);
  const [name, setName] = useState(a.name);
  const { busy, setBusy, pick } = usePick();
  const choose = async (slot: "playing" | "stopped") => {
    const p = await pick(slot);
    if (!p) return;
    if (slot === "playing") setPlaying(p);
    else {
      setStopped(p);
      setRemoveStopped(false);
    }
  };
  const nameChanged = name.trim() !== "" && name.trim() !== a.name;
  const changed = !!playing || !!stopped || (removeStopped && a.hasStopped) || nameChanged;
  const ready = changed && avatarNameOk(name) && !busy;
  const save = async () => {
    if (!ready) return;
    setBusy(true);
    const ok = await updateAvatar(a.id, name.trim(), playing?.token ?? null, stopped?.token ?? null, removeStopped && !stopped);
    setBusy(false);
    if (ok) props.onClose();
  };
  const hasStoppedNow = !!stopped || (a.hasStopped && !removeStopped);
  return (
    <Modal
      title="立ち絵を変更"
      onClose={props.onClose}
      footer={
        <>
          <button className="btn" onClick={props.onClose}>
            キャンセル
          </button>
          <button className="btn primary" disabled={!ready} aria-busy={busy} onClick={() => void save()}>
            保存
          </button>
        </>
      }
    >
      <ImageSlot
        label="再生中の絵"
        image={playing?.previewDataUrl ?? fileUrl(a.playingPreview)}
        note={playing ? `差し替える：${describe(playing)}` : "今の絵"}
        emptyText=""
      >
        <button className="btn" disabled={busy} aria-busy={busy} onClick={() => void choose("playing")}>
          差し替える…
        </button>
        {playing ? (
          <button className="btn" disabled={busy} onClick={() => setPlaying(null)}>
            元に戻す
          </button>
        ) : null}
      </ImageSlot>
      <ImageSlot
        label="停止中の絵"
        image={stopped?.previewDataUrl ?? (hasStoppedNow ? fileUrl(a.stoppedPreview) : null)}
        note={stopped ? `差し替える：${describe(stopped)}` : removeStopped && a.hasStopped ? "外す（止めている間も再生中の絵）" : a.hasStopped ? "今の絵" : "なし（止めている間も再生中の絵）"}
        emptyText="再生中と同じ"
      >
        <button className="btn" disabled={busy} aria-busy={busy} onClick={() => void choose("stopped")}>
          {a.hasStopped || stopped ? "差し替える…" : "画像を選ぶ…"}
        </button>
        {hasStoppedNow ? (
          <button
            className="btn"
            disabled={busy}
            onClick={() => {
              setStopped(null);
              setRemoveStopped(true);
            }}
          >
            外す
          </button>
        ) : null}
        {stopped || (removeStopped && a.hasStopped) ? (
          <button
            className="btn"
            disabled={busy}
            onClick={() => {
              setStopped(null);
              setRemoveStopped(false);
            }}
          >
            元に戻す
          </button>
        ) : null}
      </ImageSlot>
      <div className="field">
        <span className="flabel">名前</span>
        <input
          className="input"
          value={name}
          maxLength={AVATAR_NAME_MAX * 2}
          aria-label="立ち絵の名前"
          placeholder={`${NAME_HINT}（空なら今の名前のまま）`}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && ready) void save();
          }}
        />
        <span className="hint">差し替える画像の決まりは追加と同じです。「保存」を押すまで、今の絵は変わりません（保存の途中で失敗しても、今の絵が残ります）。</span>
      </div>
    </Modal>
  );
}

export function AvatarsField() {
  const st = useStore(skinState);
  const list = useStore(avatarList);
  const ver = useStore(skinListVersion);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<AvatarSummary | null>(null);
  useEffect(() => {
    void loadAvatars();
  }, [ver, st?.effectiveId]);
  const choice = st?.avatar ?? CHOICE_SKIN;
  const skinChar = st?.skin?.character ?? null;
  const skinStill = skinChar ? skinChar.stopped.still ?? skinChar.stopped.path : null;
  const rows = avatarRows(list, st?.skin?.name ?? null, !!skinChar);
  const canCreate = canCreateAvatar(list);

  const edit = (r: AvatarRow) => {
    const a = list?.avatars.find((x) => x.id === r.avatarId);
    if (a && !a.broken) setEditing(a);
  };
  const del = async (r: AvatarRow) => {
    if (!r.avatarId) return;
    const using = r.choice === choice;
    const ok = await confirmDialog(
      "立ち絵を削除",
      `「${r.broken ? "読めない立ち絵" : r.name}」を削除します。${using ? "使っている立ち絵なので、「スキンの立ち絵」に戻します。" : ""}元に戻せません（元の画像が手元にあれば、追加し直せます）。`,
      "削除",
      true,
    );
    if (ok) await deleteAvatar(r.avatarId);
  };

  return (
    <div className="avatar-block">
      <h4 className="skin-subhead">立ち絵</h4>
      {st?.safeMode ? <div className="skin-callout warn">既定の見た目で動いている間（セーフモード）は、立ち絵を出しません。選んでおくことはできます。</div> : null}
      {/* v3.12.0：カードは左に絵・右に名前・説明・印・ボタン（.avatar-grid・.avatar-card。上の「スキン」の一覧のカードは今のまま） */}
      <div className="skin-grid avatar-grid" role="group" aria-label="使う立ち絵（カードを押すと切り替わります）">
        {rows.map((r) => {
          const on = r.choice === choice;
          const pickable = !r.broken;
          return (
            // カード全体を押すと、その立ち絵に切り替わる。中のボタン（使う・変更・削除）から来たクリック・キーは、
            // ボタン自身が処理するので切り替えない（cardClickSwitches・cardKeySwitches）
            <div
              key={r.choice}
              className={"skin-card avatar-card" + (on ? " active" : "") + (r.broken ? " broken" : "") + (pickable ? " pickable" : "")}
              role="button"
              tabIndex={pickable ? 0 : -1}
              aria-pressed={on}
              aria-disabled={!pickable}
              aria-label={avatarCardLabel(r, on)}
              onClick={(e) => {
                if (cardClickSwitches(r, on, !!(e.target as HTMLElement).closest("button"))) void chooseAvatar(r.choice);
              }}
              onKeyDown={(e) => {
                if (cardKeySwitches(e.key, r, on, e.target === e.currentTarget)) {
                  e.preventDefault();
                  void chooseAvatar(r.choice);
                }
              }}
            >
              <RowPreview row={r} skinStill={skinStill} />
              <div className="avatar-card-body">
                <div className="skin-name">{r.name}</div>
                {/* v3.12.0：説明はどのカードも切らずに折り返す（「スキンの立ち絵」の「（いまは無し）」など、文の終わりに意味がある）。
                    取り込んだスキンの立ち絵には「スキン」の印を付ける。印は「スキン」の一覧の札（.skin-tag）と同じ形と色（文字で伝え、色だけに頼らない） */}
                {/* 説明が空（読めない立ち絵）なら出さない（名前と理由の間に空の行を作らない） */}
                {r.note ? <div className="skin-meta wrap">{r.note}</div> : null}
                {r.badge ? (
                  <div className="skin-tags">
                    <span className="skin-tag">{r.badge}</span>
                  </div>
                ) : null}
                {r.broken ? <div className="error-text">読めません：{errorText[r.broken] ?? r.broken}</div> : null}
                <div className="skin-actions">
                  {r.broken ? null : on ? (
                    <button className="btn primary" disabled>
                      使用中
                    </button>
                  ) : (
                    <button className="btn" onClick={() => void chooseAvatar(r.choice)}>
                      使う
                    </button>
                  )}
                  {r.kind === "avatar" && !r.broken ? (
                    <button className="btn" onClick={() => edit(r)}>
                      変更
                    </button>
                  ) : null}
                  {r.kind === "avatar" ? (
                    <button className="btn danger" onClick={() => void del(r)}>
                      削除
                    </button>
                  ) : null}
                </div>
              </div>
            </div>
          );
        })}
      </div>
      <div className="field">
        <span className="flabel">立ち絵を足す</span>
        <span className="row">
          <button className="btn primary" disabled={!canCreate} onClick={() => setCreating(true)}>
            ＋ 立ち絵を追加…
          </button>
          {list ? (
            <span className="hint-text">
              {list.avatars.length} / {list.max} 体
            </span>
          ) : null}
        </span>
        <span className="hint">
          {canCreate || !list
            ? "画像を選ぶだけで追加できます（zip は要りません）。追加しただけでは出しません（一覧のカードか「使う」で出します）。スキンを替えても、選んだ立ち絵はそのままです。"
            : `登録できるのは ${list.max} 体までです。使わない立ち絵を削除してから追加してください。`}
        </span>
      </div>
      <div className="field">
        <span className="flabel">立ち絵の大きさ</span>
        <ScaleSlider />
        <WindowNote />
        <span className="hint">
          立ち絵は右側に専用の幅を取って出します（曲一覧はその左まで）。専用の幅は窓の幅の {Math.round(RAIL_WINDOW_RATIO * 100)}% × 大きさで、{RAIL_MIN_PX}〜{RAIL_MAX_PX}px に収めます（どの立ち絵でも同じ）。曲一覧の側に {MIN_LIST_SIDE_PX}px（詳細の表示では {DETAIL.listMin}px）を残すため、足りないときはまず専用の幅を {RAIL_MIN_PX}px まで縮め、それでも残らないとき（窓が狭いとき）と歌詞画面では隠します。絵は高さに合わせて出し、横長の絵の左右ははみ出した分だけ切れます。Windows の「アニメーション効果」をオフにしているときは、動く画像を止まった絵で出します。
        </span>
      </div>
      {/* 小窓はカードの外に置く（小窓の中のクリックがカードへ伝わって切り替わらないように） */}
      {creating ? <CreateDialog onClose={() => setCreating(false)} /> : null}
      {editing ? <EditDialog avatar={editing} onClose={() => setEditing(null)} /> : null}
    </div>
  );
}
