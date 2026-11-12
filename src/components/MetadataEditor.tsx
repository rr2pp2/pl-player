// メタデータ編集（原文の10項目）とアルバムアート（埋め込み画像の表示・外部画像の設定・ドラッグ＆ドロップ・画像削除）。
// 保存は「一時ファイル → 検証 → 置換」（Rust 側 library/tags_write.rs）。
import { useEffect, useState } from "react";
import { api } from "../ipc/commands";
import type { ArtChange, MetadataView, TagValues } from "../ipc/types";
import { errorMessage } from "../messages/ja";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { Artwork } from "./Artwork";
import { dropState, IMAGE_RE, registerDropZone } from "./FileDrop";
import { Modal } from "./Modal";
import { IconImage, IconTrash } from "./Icons";
import { editorTrack } from "./trackActions";

const ZONE = "editor-art";

export function MetadataEditorHost() {
  const id = useStore(editorTrack);
  if (id === null) return null;
  return <MetadataEditor key={id} id={id} onClose={() => editorTrack.set(null)} />;
}

function numOrNull(s: string): number | null {
  const n = parseInt(s, 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

function MetadataEditor(props: { id: number; onClose: () => void }) {
  const [view, setView] = useState<MetadataView | null>(null);
  const [v, setV] = useState<TagValues | null>(null);
  const [art, setArt] = useState<ArtChange>({ kind: "keep" });
  const [staged, setStaged] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<{ text: string; detail: string } | null>(null);
  const drop = useStore(dropState);

  const load = () =>
    api
      .metadataRead(props.id)
      .then((m) => {
        setView(m);
        setV(m.values);
      })
      .catch((e) => setError(errorMessage(e)));

  useEffect(() => {
    void load();
  }, [props.id]); // eslint-disable-line

  const applyImage = async (path: string) => {
    try {
      const p = await api.stageImage(path);
      setStaged(p);
      setArt({ kind: "set", path: p });
    } catch (e) {
      notify({ level: "error", error: e });
    }
  };

  // アートの枠への画像のドロップ
  useEffect(
    () =>
      registerDropZone(ZONE, (paths) => {
        const img = paths.find((p) => IMAGE_RE.test(p));
        if (img) void applyImage(img);
        else notify({ level: "error", text: "画像ファイル（PNG・JPG・WebP）をドロップしてください。" });
      }),
    [],
  );

  const set = (k: keyof TagValues, val: any) => setV((cur) => (cur ? { ...cur, [k]: val } : cur));

  const save = async () => {
    if (!v) return;
    setSaving(true);
    setError(null);
    try {
      await api.metadataWrite(props.id, v, art);
      notify({ level: "info", text: "タグを保存しました" });
      props.onClose();
    } catch (e) {
      setError(errorMessage(e));
      void load();
    } finally {
      setSaving(false);
    }
  };

  const setExternal = async () => {
    if (!view || !staged) return;
    try {
      await api.albumArtSetExternal(view.track.albumKey, staged);
      notify({ level: "info", text: "アルバムの外部画像に設定しました（音声ファイルは変更していません）" });
      setArt({ kind: "keep" });
      setStaged(null);
      void load();
    } catch (e) {
      notify({ level: "error", error: e });
    }
  };

  const clearExternal = async () => {
    if (!view) return;
    try {
      await api.albumArtClearExternal(view.track.albumKey);
      void load();
    } catch (e) {
      notify({ level: "error", error: e });
    }
  };

  const shownArt = art.kind === "set" ? staged : art.kind === "remove" ? null : view?.track.artPath;
  const field = (k: keyof TagValues, label: string, opts: { wide?: boolean; num?: boolean; area?: boolean; placeholder?: string } = {}) => (
    <label className={opts.wide ? "wide" : ""}>
      {label}
      {opts.area ? (
        <textarea className="input" value={(v?.[k] as string) ?? ""} onChange={(e) => set(k, e.target.value)} />
      ) : (
        <input
          className="input"
          placeholder={opts.placeholder}
          inputMode={opts.num ? "numeric" : undefined}
          value={opts.num ? (v?.[k] ?? "") + "" : ((v?.[k] as string) ?? "")}
          onChange={(e) => set(k, opts.num ? numOrNull(e.target.value.replace(/[^0-9]/g, "")) : e.target.value)}
        />
      )}
    </label>
  );

  return (
    <Modal
      title="タグを編集"
      onClose={props.onClose}
      footer={
        <>
          {error ? (
            <div className="error-text" style={{ marginRight: "auto", maxWidth: 420 }}>
              {error.text}
              {error.detail ? <div className="path-text">{error.detail}</div> : null}
            </div>
          ) : null}
          <button className="btn" onClick={props.onClose}>
            キャンセル
          </button>
          <button className="btn primary" onClick={save} disabled={!v || saving}>
            {saving ? "保存中…" : "ファイルに保存"}
          </button>
        </>
      }
    >
      {!v || !view ? (
        <div className="empty">{error ? error.text : "読み込み中…"}</div>
      ) : (
        <div className="editor-grid">
          <div className="editor-art">
            <Artwork path={shownArt} dropTarget={ZONE} dropActive={drop.active && drop.zone === ZONE} />
            <div className="path-text">画像をここへドラッグ＆ドロップできます</div>
            <button
              className="btn"
              onClick={async () => {
                const p = await api.pickImage("アルバムアートを選ぶ");
                if (p) void applyImage(p);
              }}
            >
              <IconImage /> 画像を選ぶ…
            </button>
            <button
              className="btn danger"
              disabled={!view.track.hasArt && art.kind !== "set"}
              onClick={() => {
                setArt({ kind: "remove" });
                setStaged(null);
              }}
            >
              <IconTrash /> 埋め込み画像を削除
            </button>
            {staged ? (
              <button className="btn" onClick={setExternal} title="音声ファイルは変えず、このアルバムの表示にだけ使います">
                アルバムの外部画像に設定
              </button>
            ) : null}
            {view.albumOverride ? (
              <button className="btn" onClick={clearExternal}>
                外部画像を外す
              </button>
            ) : null}
            <div className="path-text">
              {art.kind === "set"
                ? "保存すると、この画像をファイルに埋め込みます。"
                : art.kind === "remove"
                  ? "保存すると、埋め込み画像を削除します。"
                  : view.albumOverride
                    ? "このアルバムには外部画像を設定しています。"
                    : view.track.hasArt
                      ? "ファイルに埋め込まれた画像を表示しています。"
                      : "画像はありません。"}
            </div>
          </div>
          <div className="editor-fields">
            {field("title", "タイトル（Title）", {
              wide: true,
              // 曲名タグが無い曲は、一覧の名前（ファイル名）を薄く見せるだけにする。入力しない限りファイルには書かない（v2）
              placeholder: v.title ? undefined : `${view.track.title}（タグに曲名がありません）`,
            })}
            {field("artist", "アーティスト（Artist）")}
            {field("albumArtist", "アルバムアーティスト")}
            {field("album", "アルバム（Album）")}
            {field("genre", "ジャンル（Genre）")}
            {field("year", "年（Year）", { num: true })}
            {field("composer", "作曲者（Composer）")}
            {field("trackNo", "トラック番号（Track Number）", { num: true })}
            {field("discNo", "ディスク番号（Disc Number）", { num: true })}
            {field("comment", "コメント（Comment）", { wide: true, area: true })}
            <div className="wide path-text">
              {view.track.path}（{view.track.format.toUpperCase()}）
            </div>
          </div>
        </div>
      )}
    </Modal>
  );
}
