// 上：検索と、取り込みの操作（ファイル選択・複数選択・フォルダ追加）。
// v3.3.0 の直し：設定の画面では、検索欄の場所に見出し「設定」を出す（設定の画面の中の見出しの行は無くした）。
// v3.8.0：右端（ファイル・フォルダのボタンの隣）にモード切替「通常／コンパクト／詳細」（1つを選ぶ形。通常・コンパクトは押したほうがそのまま設定になる。
// 決まりは src\layout\density.ts の VIEW_MODES・currentViewMode・pressViewMode）。文字が入らない幅ではアイコンだけにし、
// マウスを乗せる・キーボードで選ぶと文字の吹き出し（Tip）、読み上げには aria-label で名前を渡す
// （吹き出しの文は名前と同じなので、Tip の読み上げ用の文は付けない＝describe={false}。同じ名前を二重に読ませない）。
// ファイル・フォルダの文字を隠す幅も、曲一覧の側の幅で決める（.topbar.narrow。3.7.0 まではウィンドウの幅 720px 未満）。
// v3.8.0：モード切替に「詳細」（仕様の「配信モード」）を足した。詳細かどうかは Rust が持つ（api.viewModeSet。pl://outputs で戻る）。
//   詳細で「通常」「コンパクト」を押すと詳細を抜けて押したほうになり、設定は詳細を抜けたあとの曲一覧の側の幅で決める（density.ts の pressMode）。
//   詳細では、歌詞の画面を開いていても検索欄を出す（仕様 6.1。曲を探してキューに足せるように）。歌詞の画面で打ち始めたら「すべての曲」へ移って探す
import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import { importProgress, navigate, route, search } from "../stores/library";
import { notify } from "../stores/notices";
import { settings, updateSettings } from "../stores/settings";
import { useStore } from "../stores/store";
import { listPaneExitWidth, listPaneWidth, mainPaneWidth } from "../components/useLayoutSize";
import { isDetail, outputs } from "../stores/outputs";
import { Tip } from "../components/Tip";
import { IconFilePlus, IconFolderPlus, IconSearch, IconViewCompact, IconViewDetail, IconViewNormal, IconX } from "../components/Icons";
import { currentViewMode, listLayoutFor, pressMode, viewModeLabelsFit, VIEW_MODES, type ViewMode } from "./density";

const VIEW_MODE_ICON: Record<ViewMode, () => any> = {
  normal: () => <IconViewNormal />,
  compact: () => <IconViewCompact />,
  detail: () => <IconViewDetail />,
};

/** v3.8.0：モード切替（通常／コンパクト／詳細。ミニはあとで VIEW_MODES に足す） */
function ViewModeSwitch(props: { width: number; labels: boolean }) {
  const ui = useStore(settings).ui;
  const detail = isDetail(useStore(outputs));
  const cur = currentViewMode(ui, props.width, detail);
  const press = (m: ViewMode) => {
    const next = pressMode(ui, props.width, listPaneExitWidth.get(), detail, m);
    if (next.settings) {
      const changed = next.settings.compactList !== ui.compactList || next.settings.compactAuto !== (ui.compactAuto === true);
      if (changed) {
        void updateSettings((x) => ({
          ...x,
          ui: { ...x.ui, ...pressMode(x.ui, listPaneWidth.get(), listPaneExitWidth.get(), detail, m).settings },
        }));
      }
    }
    if (next.detail !== detail) api.viewModeSet(next.detail ? "detail" : "normal").catch((e) => notify({ level: "error", error: e }));
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
  // v3.9.0：曲一覧の実際の幅（立ち絵の専用の幅を引いたもの）は自動のコンパクトに、上の帯の文字は .main の幅（上の帯は立ち絵の上にもかかる）で決める
  const listW = useStore(listPaneWidth);
  const mainW = useStore(mainPaneWidth);
  const narrow = listLayoutFor(mainW).topbarNarrow;
  const [text, setText] = useState(search.get());
  const timer = useRef<number | undefined>(undefined);
  const inputRef = useRef<HTMLInputElement>(null);

  // 画面を移ったら検索語を消す。v3.8.0：歌詞の画面で打ち始めて「すべての曲」へ移ったときだけは、打った語を持っていく
  const carry = useRef<string | null>(null);
  useEffect(() => {
    if (carry.current !== null) {
      const v = carry.current;
      carry.current = null;
      setText(v);
      search.set(v.trim());
      return;
    }
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
    // 詳細で歌詞の画面を開いているとき（検索欄を出している）：歌詞の画面には曲の一覧が無いので、「すべての曲」へ移って探す
    if (route.get().view === "lyrics" && v.trim() !== "") {
      carry.current = v;
      setText(v);
      navigate({ view: "all" });
      return;
    }
    setText(v);
    window.clearTimeout(timer.current);
    // 打つたびに引く（約 120ms まとめる）
    timer.current = window.setTimeout(() => search.set(v.trim()), 120);
  };

  const detail = isDetail(useStore(outputs));
  const searchable = r.view !== "settings" && (r.view !== "lyrics" || detail);
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
        <ViewModeSwitch width={listW} labels={viewModeLabelsFit(mainW, narrow)} />
      </div>
    </header>
  );
}
