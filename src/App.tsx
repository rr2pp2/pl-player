import { useEffect, useState } from "react";
import { AppShell } from "./layout/AppShell";
import { ContextMenuHost } from "./components/ContextMenu";
import { DialogHost } from "./components/Modal";
import { DropOverlay, useFileDrop } from "./components/FileDrop";
import { MetadataEditorHost } from "./components/MetadataEditor";
import { LyricsEditorHost } from "./components/LyricsEditor";
import { Toasts } from "./components/Toasts";
import { useAppShortcuts } from "./shortcuts/shortcuts";
import { initPlayback } from "./stores/playback";
import { initLyrics } from "./stores/lyrics";
import { initLyricsFetch } from "./stores/lyricsFetch";
import { initSettings } from "./stores/settings";
import { initSkins } from "./stores/skin";
import { initLibrary } from "./stores/library";
import { initNotices, notify } from "./stores/notices";

export default function App() {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    (async () => {
      try {
        await initNotices();
        await initSettings();
        await initLibrary();
        await initPlayback();
        // 着せ替え（v3.2.0）：最初に描く前に、スキンの色と文字を当てる
        await initSkins();
        initLyrics();
        await initLyricsFetch();
      } catch (e) {
        notify({ level: "error", error: e });
      } finally {
        setReady(true);
      }
    })();
  }, []);
  useAppShortcuts();
  useFileDrop();
  // 右クリックの標準メニュー（再読み込みなど）は出さない
  useEffect(() => {
    const block = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea")) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", block);
    return () => window.removeEventListener("contextmenu", block);
  }, []);
  return (
    <>
      {ready ? <AppShell /> : null}
      <DropOverlay />
      <ContextMenuHost />
      <MetadataEditorHost />
      {/* 歌詞の編集画面の上に確認の小窓を出すため、DialogHost より前に置く */}
      <LyricsEditorHost />
      <DialogHost />
      <Toasts />
    </>
  );
}
