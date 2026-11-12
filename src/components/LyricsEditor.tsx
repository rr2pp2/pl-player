// 歌詞の編集画面（v3）。曲情報の編集画面（MetadataEditor）とは別。
// 保存先はアプリの中（SQLite の manual）。曲ファイルは変更しない。
import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import type { LyricsPreview } from "../ipc/types";
import { errorMessage, lyricsWarning } from "../messages/ja";
import { applyLyricsPayload, lyricsEditor, type EditorTarget } from "../stores/lyrics";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { confirmDialog, Modal } from "./Modal";
import { IconTrash } from "./Icons";

export function LyricsEditorHost() {
  const target = useStore(lyricsEditor);
  if (!target) return null;
  return <LyricsEditor key={target.trackId} target={target} onClose={() => lyricsEditor.set(null)} />;
}

function LyricsEditor(props: { target: EditorTarget; onClose: () => void }) {
  const t = props.target;
  const [text, setText] = useState(t.initialText);
  const [preview, setPreview] = useState<LyricsPreview | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<{ text: string; detail: string } | null>(null);
  // 確認の小窓を出している間は、Escape などで編集画面を閉じない
  const asking = useRef(false);
  const areaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    areaRef.current?.focus();
  }, []);

  // 保存前の見本（読み解きは Rust 側の1か所だけ。少し待ってまとめる）
  useEffect(() => {
    let alive = true;
    const h = window.setTimeout(() => {
      api
        .previewLyrics(text)
        .then((p) => alive && setPreview(p))
        .catch(() => alive && setPreview(null));
    }, 200);
    return () => {
      alive = false;
      window.clearTimeout(h);
    };
  }, [text]);

  const close = async () => {
    if (asking.current) return;
    if (text !== t.initialText) {
      asking.current = true;
      const ok = await confirmDialog("歌詞の編集をやめる", "保存していない変更があります。閉じると変更は消えます。", "閉じる", true);
      asking.current = false;
      if (!ok) return;
    }
    props.onClose();
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const p = await api.saveManualLyrics(t.trackId, text);
      applyLyricsPayload(p);
      notify({ level: "info", text: "歌詞を保存しました（曲ファイルは変更していません）" });
      props.onClose();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (asking.current) return;
    asking.current = true;
    const ok = await confirmDialog(
      "手動の歌詞を削除",
      "この曲の手動の歌詞を削除します。外部の歌詞か曲のタグの歌詞があれば、そちらの表示に戻ります。曲ファイルは変更しません。",
      "削除",
      true,
    );
    asking.current = false;
    if (!ok) return;
    try {
      const p = await api.deleteManualLyrics(t.trackId);
      applyLyricsPayload(p);
      notify({ level: "info", text: "手動の歌詞を削除しました" });
      props.onClose();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const canSave = !!preview && preview.hasContent && !preview.tooLarge && !saving;
  return (
    <Modal
      title="歌詞を編集"
      onClose={() => void close()}
      footer={
        <>
          {error ? (
            <div className="error-text" style={{ marginRight: "auto", maxWidth: 420 }}>
              {error.text}
              {error.detail ? <div className="path-text">{error.detail}</div> : null}
            </div>
          ) : null}
          {t.hasManual ? (
            <button className="btn danger" onClick={() => void remove()} style={error ? undefined : { marginRight: "auto" }}>
              <IconTrash /> 手動の歌詞を削除
            </button>
          ) : null}
          <button className="btn" onClick={() => void close()}>
            キャンセル
          </button>
          <button className="btn primary" onClick={() => void save()} disabled={!canSave}>
            {saving ? "保存中…" : "保存"}
          </button>
        </>
      }
    >
      <div className="lyrics-editor">
        <div className="path-text">
          {t.title}
          {t.artist ? ` ／ ${t.artist}` : ""}
        </div>
        <textarea
          ref={areaRef}
          className="input lyrics-input"
          value={text}
          spellCheck={false}
          aria-label="歌詞"
          placeholder={"ここに歌詞を貼り付けます。\n[00:12.34] のような時刻タグがあれば時刻つき、無ければ時刻なしとして扱います。"}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="lyrics-preview" aria-live="polite">
          {preview ? (
            <>
              <span className="lyrics-badge">{preview.format === "timed" ? "時刻つき" : "時刻なし"}</span>
              <span>{preview.lineCount}行</span>
              {!preview.hasContent ? <span className="error-text">保存できる本文がありません</span> : null}
              {preview.tooLarge ? <span className="error-text">1MiB を超えているため保存できません</span> : null}
              {preview.warnings.map((w) => (
                <span className="hint-text" key={w}>
                  {lyricsWarning(w)}
                </span>
              ))}
            </>
          ) : (
            <span className="hint-text">確認中…</span>
          )}
        </div>
        <div className="hint-text">保存先はこのアプリの中です。曲ファイルは変更しません。</div>
      </div>
    </Modal>
  );
}
