// 骨組み：背景の層の上に、サイドバー・上の検索・一覧・下の再生バーを置く。
// v3.8.0：一覧の側（.main）の幅を測って listPaneWidth に入れる（曲一覧と上の帯が使う）。再生バーの形は PlayerBar が自分の幅で決める。
// v3.8.0：詳細（仕様の「配信モード」）では、ミキサー（Mixer.tsx）を .app の格子に足す。左右か上下か・ミキサーの幅と高さ・
//   流れる歌詞の列を自動で隠すかは src\layout\detailLayout.ts が窓の大きさで決める（決まった窓の幅では区切らない）。
//   詳細を抜けたあとの曲一覧の側の幅（listPaneExitWidth）も入れる（上の帯の「通常」「コンパクト」を押したときに使う）
// 画面内のドラッグ（曲 → プレイリスト／キュー、並び替え）はここでまとめて受ける。
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { DndContext, DragOverlay, PointerSensor, useSensor, useSensors, type DragEndEvent, type DragStartEvent } from "@dnd-kit/core";
import { api } from "../ipc/commands";
import { navigate, playlists, route } from "../stores/library";
import { notify } from "../stores/notices";
import { lyricsRowOn, settings, updateSettings } from "../stores/settings";
import { useStore } from "../stores/store";
import { characterPaneWidth, listPaneExitWidth, listPaneWidth, mainPaneWidth, useLayoutSize } from "../components/useLayoutSize";
import { CharacterLayer } from "../components/CharacterLayer";
import { characterLayout, characterStage, listSideWidth } from "../skins/character";
import { fallbackView } from "./visibility";
import { centerWidth } from "./detailLayout";
import { characterMinListOf, characterPaneOf, useDetailLayout } from "./useDetailLayout";
import { Mixer } from "./Mixer";
import { playingNow, reducedMotion, useCharacter } from "../stores/skin";
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
  // 流れる歌詞の列（v3.0.2）：設定がオンなら出す。v3.8.0 から歌詞画面を開いている間も出す
  // （3.7.0 までは歌詞画面で隠していた。作者「歌詞を全画面表示中にも再生曲の下に歌詞を出したい」2026-11-18）
  const view = useStore(route).view;
  const rowWanted = lyricsRowOn(s);
  // v3.8.0：詳細の配置（詳細でなければ null）。詳細で曲一覧が2行を切るときは、流れる歌詞の列を自動で隠す（設定は書き換えない。窓を広げたら戻る）
  const D = useDetailLayout(size, s, collapsed, rowWanted);
  const showRow = rowWanted && !(D?.hideLyricsRow ?? false);
  const exitW = centerWidth(size.width, collapsed);
  useLayoutEffect(() => listPaneExitWidth.set(exitW), [exitW]);
  // 着せ替えの立ち絵（v3.2.0）：出さないとき（既定のスキンも）は、画面の作りを今のまま変えない。
  // v3.9.0：出す絵は Rust が「使う立ち絵」（スキンの立ち絵・登録した立ち絵・ほかのスキンの立ち絵・出さない）から決めたもの
  const character0 = useCharacter();
  const playing = useStore(playingNow);
  const reduce = useStore(reducedMotion);
  const characterInput = {
    character: character0,
    // v3.9.0：「出さない」は Rust が character を null にして伝える（「立ち絵を出す」のチェックは「使う立ち絵」の一覧にまとめた）
    visible: true,
    scale: s.skin?.characterScale ?? 1,
    view,
    // v3.9.0：専用の幅は窓の幅で決める（clamp(140, 窓 × 18% × 大きさ, 480)。立ち絵ごとの幅は使わない）
    windowWidth: size.width,
    // 隠すかは、曲一覧の側（専用の幅を取る前）から専用の幅を引いた残りで決める。詳細でミキサーを左右に並べたときはその左の幅、
    // 上下のときと詳細でないときは中央の幅（centerWidth）
    paneWidth: characterPaneOf(size.width, D, collapsed),
    // 残す幅：詳細なら 360（DETAIL.listMin。作者の決定「詳細でも出す」）、そうでなければ 634
    minListPx: characterMinListOf(D),
    playing,
    reducedMotion: reduce,
  };
  const character = characterLayout(characterInput);
  useLayoutEffect(() => characterPaneWidth.set(characterInput.paneWidth), [characterInput.paneWidth]);
  // v3.3.0：歌詞画面・狭い幅で一時的に隠すときも、立ち絵の画像は読み込んだままにする（出し直しで動く画像が頭に戻らないように）
  const stage = characterStage(characterInput);
  // v3.3.0：サイドバーの項目を隠したとき、その項目の画面を開いていれば、出している最初の項目へ移る（起動のときも同じ）
  const sidebarItemsKey = JSON.stringify(s.ui.sidebarItems ?? {});
  useEffect(() => {
    const to = fallbackView(settings.get().ui, route.get().view);
    if (to) navigate({ view: to as any });
  }, [sidebarItemsKey]);
  // v3.8.0：曲一覧の側の幅（.main の幅）を測る。曲一覧の列・ジャケット・行の高さ・自動のコンパクト・上の帯の文字は、この幅で決める
  // （ウィンドウの幅ではなく。配信モードで曲一覧の横にミキサーを並べたとき、曲一覧だけが狭くなるため）
  // v3.9.0：立ち絵を出しているときは、曲一覧の列を決める幅から専用の幅（railPx。曲一覧の margin-right と同じ値）を引く。
  // 出す・隠すは上の characterLayout が専用の幅を引く前の幅で決めてある。ここで引いた幅は出す判断に戻さない（引きずり合って切り替わらない）
  const mainRef = useRef<HTMLElement>(null);
  const railRef = useRef<typeof character>(character);
  railRef.current = character;
  const railPx = character ? character.railPx : 0;
  useLayoutEffect(() => {
    const el = mainRef.current;
    if (!el) return;
    const put = () => {
      const w = el.getBoundingClientRect().width;
      mainPaneWidth.set(w);
      listPaneWidth.set(listSideWidth(w, railRef.current));
    };
    put();
    const ro = new ResizeObserver(put);
    ro.observe(el);
    return () => ro.disconnect();
  }, [railPx]);
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
        <div
          className={
            "app" + (collapsed ? " sidebar-collapsed" : "") + (showRow ? " lyrics-row-on" : "") + (D ? (D.side ? " detail detail-side" : " detail detail-stack") : "")
          }
          style={D ? ({ ["--mixer-w" as any]: `${D.mixerW}px`, ["--mixer-h" as any]: `${D.mixerH}px` } as any) : undefined}
        >
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
          {D ? <Mixer scrolls={D.mixerScrolls} /> : null}
          <PlayerBar />
          {showRow ? <LyricsRow /> : null}
        </div>
        <DragOverlay dropAnimation={null}>{dragLabel ? <div className="drag-chip">♪ {dragLabel}</div> : null}</DragOverlay>
      </DndContext>
    </>
  );
}
