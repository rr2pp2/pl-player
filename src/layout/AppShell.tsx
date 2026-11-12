// 骨組み：背景の層の上に、サイドバー・上の検索・一覧・下の再生バーを置く。
// 画面内のドラッグ（曲 → プレイリスト／キュー、並び替え）はここでまとめて受ける。
import { useState } from "react";
import { DndContext, DragOverlay, PointerSensor, useSensor, useSensors, type DragEndEvent, type DragStartEvent } from "@dnd-kit/core";
import { api } from "../ipc/commands";
import { playlists, route } from "../stores/library";
import { notify } from "../stores/notices";
import { lyricsRowOn, settings, updateSettings } from "../stores/settings";
import { useStore } from "../stores/store";
import { useLayoutSize } from "../components/useLayoutSize";
import type { DragData } from "../components/TrackTable";
import { addToPlaylist } from "../components/trackActions";
import { BackgroundLayer } from "./BackgroundLayer";
import { PlayerBar } from "./PlayerBar";
import { LyricsRow } from "./LyricsRow";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { LibraryView } from "../views/LibraryView";
import { GroupView } from "../views/GroupView";
import { PlaylistView } from "../views/PlaylistView";
import { QueueView } from "../views/QueueView";
import { SettingsView } from "../views/SettingsView";
import { LyricsView } from "../views/LyricsView";

function Content() {
  const r = useStore(route);
  if (r.view === "settings") return <SettingsView />;
  if (r.view === "lyrics") return <LyricsView />;
  if (r.view === "queue") return <QueueView />;
  if (r.view === "playlist") return <PlaylistView />;
  if ((r.view === "artist" || r.view === "album" || r.view === "genre") && (r.key === undefined || r.key === null)) return <GroupView />;
  return <LibraryView />;
}

export function AppShell() {
  const size = useLayoutSize();
  const s = useStore(settings);
  const collapsed = s.ui.sidebarCollapsed || size.sidebarAuto;
  // 流れる歌詞の列（v3.0.2）：設定がオンで、歌詞画面を開いていないときだけ出す（閉じたら設定どおりに戻る）
  const view = useStore(route).view;
  const showRow = lyricsRowOn(s) && view !== "lyrics";
  const [dragLabel, setDragLabel] = useState<string | null>(null);
  // 少し動かしてからドラッグ開始（クリック・ダブルクリックと区別する）
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  const onDragStart = (e: DragStartEvent) => {
    const d = e.active.data.current as DragData | undefined;
    setDragLabel(d?.label ?? null);
  };
  const onDragEnd = (e: DragEndEvent) => {
    setDragLabel(null);
    const d = e.active.data.current as DragData | undefined;
    const overId = e.over ? String(e.over.id) : null;
    if (!d || !overId) return;
    const fail = (err: unknown) => notify({ level: "error", error: err });
    if (overId === "drop:queue") {
      api.queueAdd(d.ids).then(() => notify({ level: "info", text: `キューに${d.ids.length}曲を追加しました` }), fail);
      return;
    }
    if (overId.startsWith("drop:playlist:")) {
      const pid = Number(overId.slice("drop:playlist:".length));
      const name = playlists.get().find((p) => p.id === pid)?.name;
      void addToPlaylist(pid, d.ids, name);
      return;
    }
    // 並び替え（同じ一覧の中）
    const over = e.over?.data.current as DragData | undefined;
    if (d.list && over && over.list === d.list && d.index !== undefined && over.index !== undefined && d.index !== over.index) {
      if (d.list === "queue") {
        api.queueMove(d.index, over.index).catch(fail);
      } else if (d.list === "playlist") {
        const r = route.get();
        const itemId = Number(String(e.active.id).replace("pli:", ""));
        api.playlistMoveItem(Number(r.key), itemId, over.index).catch(fail);
      }
    }
  };

  return (
    <>
      <BackgroundLayer />
      <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragLabel(null)} autoScroll>
        <div className={"app" + (collapsed ? " sidebar-collapsed" : "") + (showRow ? " lyrics-row-on" : "")}>
          <Sidebar
            onToggle={() => {
              if (size.sidebarAuto) return; // 狭いときはアイコンだけのまま
              void updateSettings((x) => ({ ...x, ui: { ...x.ui, sidebarCollapsed: !x.ui.sidebarCollapsed } }));
            }}
          />
          <main className="main panel">
            <TopBar />
            <Content />
          </main>
          <PlayerBar />
          {showRow ? <LyricsRow /> : null}
        </div>
        <DragOverlay dropAnimation={null}>{dragLabel ? <div className="drag-chip">♪ {dragLabel}</div> : null}</DragOverlay>
      </DndContext>
    </>
  );
}
