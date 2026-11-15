// 上：検索と、取り込みの操作（ファイル選択・複数選択・フォルダ追加）。
// v3.3.0 の直し（作者の要望）：設定の画面では、検索欄の場所に見出し「設定」を出す（設定の画面の中の見出しの行は無くした）。
// v3.8.0：右端（ファイル・フォルダのボタンの隣）にモード切替「通常／コンパクト」（1つを選ぶ形。押したほうがそのまま設定になる。
// 決まりは src\layout\density.ts の VIEW_MODES・currentViewMode・pressViewMode）。文字が入らない幅ではアイコンだけにし、
// マウスを乗せる・キーボードで選ぶと文字の吹き出し（Tip）、読み上げには aria-label で名前を渡す
// （吹き出しの文は名前と同じなので、Tip の読み上げ用の文は付けない＝describe={false}。同じ名前を二重に読ませない）。
// ファイル・フォルダの文字を隠す幅も、曲一覧の側の幅で決める（.topbar.narrow。3.7.0 まではウィンドウの幅 720px 未満）。
import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import { importProgress, route, search } from "../stores/library";
import { notify } from "../stores/notices";
import { settings, updateSettings } from "../stores/settings";
import { useStore } from "../stores/store";
import { listPaneWidth } from "../components/useLayoutSize";
import { Tip } from "../components/Tip";
import { IconFilePlus, IconFolderPlus, IconSearch, IconViewCompact, IconViewNormal, IconX } from "../components/Icons";
import { currentViewMode, listLayoutFor, pressViewMode, viewModeLabelsFit, VIEW_MODES, type ViewMode } from "./density";

const VIEW_MODE_ICON: Record<ViewMode, () => any> = {
  normal: () => <IconViewNormal />,
  compact: () => <IconViewCompact />,
};

/** v3.8.0：モード切替（通常／コンパクト。配信は段2、ミニはあとで VIEW_MODES に足す） */
function ViewModeSwitch(props: { width: number; labels: boolean }) {
  const ui = useStore(settings).ui;
  const cur = currentViewMode(ui, props.width);
  const press = (m: ViewMode) => {
    const next = pressViewMode(ui, props.width, m);
    if (next.compactList === ui.compactList && next.compactAuto === (ui.compactAuto === true)) return;
    void updateSettings((x) => ({ ...x, ui: { ...x.ui, ...pressViewMode(x.ui, listPaneWidth.get(), m) } }));
  };
  return (
    <span className={"seg view-mode" + (props.labels ? "" : " icons")} role="radiogroup" aria-label="表示のモード">
      {VIEW_MODES.map((m) => (
        <Tip key={m.id} text={props.labels ? null : m.label} delay={300} place="below" className="inline" describe={false}>
          <button role="radio" aria-checked={cur === m.id} aria-label={m.label} className={cur === m.id ? "on" : ""} onClick={() => press(m.id)}>
            {VIEW_MODE_ICON[m.id]()}
            {props.labels ? <span>{m.label}</span> : null}
          </button>
        </Tip>
      ))}
    </span>
  );
}

export function TopBar() {
  const r = useStore(route);
  const prog = useStore(importProgress);
  const listW = useStore(listPaneWidth);
  const narrow = listLayoutFor(listW).topbarNarrow;
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
    <header className={"topbar" + (narrow ? " narrow" : "")}>
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
        <ViewModeSwitch width={listW} labels={viewModeLabelsFit(listW, narrow)} />
      </div>
    </header>
  );
}
