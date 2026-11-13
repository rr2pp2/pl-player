// 上：検索と、取り込みの操作（ファイル選択・複数選択・フォルダ追加）。
// v3.3.0 の直し（作者の要望）：設定の画面では、検索欄の場所に見出し「設定」を出す（設定の画面の中の見出しの行は無くした）。
import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import { importProgress, route, search } from "../stores/library";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { IconFilePlus, IconFolderPlus, IconSearch, IconX } from "../components/Icons";

export function TopBar() {
  const r = useStore(route);
  const prog = useStore(importProgress);
  const [text, setText] = useState(search.get());
  const timer = useRef<number | undefined>(undefined);
  const inputRef = useRef<HTMLInputElement>(null);

  // 画面を移ったら検索語を消す
  useEffect(() => {
    setText("");
    search.set("");
  }, [r.view, r.key]);

  // Ctrl+F で検索欄へ
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.code === "KeyF") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const onChange = (v: string) => {
    setText(v);
    window.clearTimeout(timer.current);
    // 打つたびに引く（約 120ms まとめる）
    timer.current = window.setTimeout(() => search.set(v.trim()), 120);
  };

  const searchable = r.view !== "settings" && r.view !== "lyrics";
  return (
    <header className="topbar">
      {r.view === "settings" ? <h1 className="view-title topbar-title">設定</h1> : null}
      <div className="search" style={{ visibility: searchable ? "visible" : "hidden", display: r.view === "settings" ? "none" : undefined }}>
        <IconSearch />
        <input
          ref={inputRef}
          type="search"
          placeholder="曲名・アーティスト・アルバムで検索"
          aria-label="検索"
          value={text}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onChange("");
          }}
        />
        {text ? (
          <button className="icon-btn small" onClick={() => onChange("")} title="検索語を消す">
            <IconX />
          </button>
        ) : null}
      </div>
      {prog ? (
        <div className="import-status" title={prog.current}>
          <span>
            取り込み中… {prog.done} / {prog.total}
          </span>
          <div className="progress-line">
            <div style={{ width: `${prog.total ? (prog.done / prog.total) * 100 : 0}%` }} />
          </div>
        </div>
      ) : null}
      <div className="topbar-actions">
        <button
          className="btn"
          onClick={() => api.libraryPickFiles().catch((e) => notify({ level: "error", error: e }))}
          title="ファイルを選んで取り込む（複数選べます）"
        >
          <IconFilePlus />
          <span className="hide-narrow">ファイル</span>
        </button>
        <button
          className="btn"
          onClick={() => api.libraryPickFolder().catch((e) => notify({ level: "error", error: e }))}
          title="フォルダを追加（サブフォルダまで探します）"
        >
          <IconFolderPlus />
          <span className="hide-narrow">フォルダ</span>
        </button>
      </div>
    </header>
  );
}
