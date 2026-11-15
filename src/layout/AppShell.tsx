// 骨組み：背景の層の上に、サイドバー・上の検索・一覧・下の再生バーを置く。
// v3.8.0：一覧の側（.main）の幅を測って listPaneWidth に入れる（曲一覧と上の帯が使う）。再生バーの形は PlayerBar が自分の幅で決める。
// 画面内のドラッグ（曲 → プレイリスト／キュー、並び替え）はここでまとめて受ける。
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { DndContext, DragOverlay, PointerSensor, useSensor, useSensors, type DragEndEvent, type DragStartEvent } from "@dnd-kit/core";
import { api } from "../ipc/commands";
import { navigate, playlists, route } from "../stores/library";
import { notify } from "../stores/notices";
import { lyricsRowOn, settings, updateSettings } from "../stores/settings";
import { useStore } from "../stores/store";
import { listPaneWidth, useLayoutSize } from "../components/useLayoutSize";
import { CharacterLayer } from "../components/CharacterLayer";
import { characterLayout, characterStage } from "../skins/character";
import { fallbackView } from "./visibility";
import { playingNow, reducedMotion, useSkin } from "../stores/skin";
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
import { YearlyView } from "../views/YearlyView";

function Content() {
  const r = useStore(route);
  if (r.view === "settings") return <SettingsView />;
  if (r.view === "lyrics") return <LyricsView />;
  if (r.view === "queue") return <QueueView />;
  if (r.view === "playlist") return <PlaylistView />;
  if (r.view === "yearly_top") return <YearlyView />;
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
  // 着せ替えの立ち絵（v3.2.0）：出さないとき（既定のスキンも）は、画面の作りを今のまま変えない
  const skin = useSkin();
  const playing = useStore(playingNow);
  const reduce = useStore(reducedMotion);
  const characterInput = {
    character: skin?.character,
    visible: s.skin?.characterVisible ?? true,
    scale: s.skin?.characterScale ?? 1,
    view,
    windowWidth: size.width,
    playing,
    reducedMotion: reduce,
  };
  const character = characterLayout(characterInput);
  // v3.3.0：歌詞画面・狭い幅で一時的に隠すときも、立ち絵の画像は読み込んだままにする（出し直しで動く画像が頭に戻らないように）
  const stage = characterStage(characterInput);
  // v3.3.0：サイドバーの項目を隠したとき、その項目の画面を開いていれば、出している最初の項目へ移る（起動のときも同じ）
  const sidebarItemsKey = JSON.stringify(s.ui.sidebarItems ?? {});
  useEffect(() => {
    const to = fallbackView(settings.get().ui, route.get().view);
    if (to) navigate({ view: to as any });
  }, [sidebarItemsKey]);
  // v3.8.0：曲一覧の側の幅（.main の幅）を測る。曲一覧の列・ジャケット・行の高さ・自動のコンパクト・上の帯の文字は、この幅で決める
  // （ウィンドウの幅ではなく。配信モード（段2）で曲一覧の横にミキサーを並べたとき、曲一覧だけが狭くなるため）
  const mainRef = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const el = mainRef.current;
    if (!el) return;
    const put = () => listPaneWidth.set(el.getBoundingClientRect().width);
    put();
    const ro = new ResizeObserver(put);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
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
            sidebarAuto={size.sidebarAuto}
            collapsed={collapsed}
            onToggle={() => {
              if (size.sidebarAuto) return; // 狭いときはアイコンだけのまま
              void updateSettings((x) => ({ ...x, ui: { ...x.ui, sidebarCollapsed: !x.ui.sidebarCollapsed } }));
            }}
          />
          <main
            ref={mainRef}
            className={"main panel" + (character ? " has-character-rail" : "")}
            style={character ? ({ ["--character-rail-w" as any]: `${character.railPx}px` } as any) : undefined}
          >
            <TopBar />
            <Content />
            {stage ? <CharacterLayer stage={stage} /> : null}
          </main>
          <PlayerBar />
          {showRow ? <LyricsRow /> : null}
        </div>
        <DragOverlay dropAnimation={null}>{dragLabel ? <div className="drag-chip">♪ {dragLabel}</div> : null}</DragOverlay>
      </DndContext>
    </>
  );
}
