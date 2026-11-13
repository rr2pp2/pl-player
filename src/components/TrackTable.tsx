// 曲一覧。見えている行だけを描く（大量の曲でも重くならないように）。
// 列は幅に合わせて出し入れする（Genre → Album の順で隠す）。
// v3.3.0：設定の「ごみ箱へ移すボタン」がオンのときだけ、行の右端に「ごみ箱へ移す」を出す（既定はオフ）。
// 出すのはライブラリの曲一覧だけ（props.list。プレイリスト・キューの一覧には、行のボタンも右クリックのメニューも出さない）。
// 対象は右クリックのメニューと同じ決まり：押した行が選んでいる行の中にあれば選んでいる全部、そうでなければその行だけ。
// v3.4.0：コンパクト表示（設定の ui.compactList。決まりは src\layout\density.ts）：ジャケットの列を出さず、行を詰め、
// タイトルとアーティストを1行に並べる。選択・再生中の印・ドラッグ・右クリック・お気に入り・ごみ箱の列は今までどおり。
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useDraggable } from "@dnd-kit/core";
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { api } from "../ipc/commands";
import type { Track } from "../ipc/types";
import { playback } from "../stores/playback";
import { settings } from "../stores/settings";
import { sort } from "../stores/library";
import { useStore } from "../stores/store";
import { notify } from "../stores/notices";
import { Artwork } from "./Artwork";
import { openMenu, type MenuItem } from "./ContextMenu";
import { formatTime } from "./format";
import { IconBars, IconStar, IconTrash } from "./Icons";
import { trackMenu, trashBusy, trashTracksFlow } from "./trackActions";
import { trashButtonShown, trashTargets, type TrackListKind } from "../layout/visibility";
import { compactListOn, trackRowHeight } from "../layout/density";
import { useLayoutSize } from "./useLayoutSize";

export interface DragData {
  kind: "tracks";
  ids: number[];
  label: string;
  /** 並べ替えのとき：どの一覧の何番目か */
  list?: "playlist" | "queue";
  index?: number;
}

interface Props {
  tracks: Track[];
  /** どの一覧か（「ごみ箱へ移す」を出すかの決め手。src\layout\visibility.ts の trashButtonShown） */
  list: TrackListKind;
  /** 並べ替えできる一覧なら、行ごとの ID（プレイリストの item / キューの位置） */
  sortableIds?: string[];
  sortableList?: "playlist" | "queue";
  sortable?: boolean;
  onPlay: (index: number) => void;
  extraMenu?: (indexes: number[]) => MenuItem[];
  onDeleteKey?: (indexes: number[]) => void;
  empty?: any;
}

export function TrackTable(props: Props) {
  const size = useLayoutSize();
  const ui = useStore(settings).ui;
  const cfg = ui.columns;
  const busy = useStore(trashBusy);
  const pb = useStore(playback);
  const sortState = useStore(sort);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const anchor = useRef<number | null>(null);

  // 一覧が入れ替わったら選択を外す
  useEffect(() => {
    setSelected(new Set());
    anchor.current = null;
  }, [props.tracks]);

  // v3.4.0：コンパクト表示か（「狭いときだけ」はウィンドウの幅で決める）と、行の高さ（仮想スクロールの1行の大きさ）
  const compact = compactListOn(ui, size.width);
  const rowH = trackRowHeight(compact, size.rowH);
  const cols = {
    album: cfg.album !== false && !size.hideAlbum,
    genre: cfg.genre !== false && !size.hideGenre,
    duration: cfg.duration !== false,
    favorite: cfg.favorite !== false,
    trash: trashButtonShown(ui, props.list),
  };
  const template = [
    // ジャケットの列：コンパクト表示では出さない（見出しの行の空き枠も出さない）
    compact ? null : `${size.artSize}px`,
    "minmax(0, 3fr)",
    cols.album ? "minmax(0, 2fr)" : null,
    cols.genre ? "minmax(0, 1.2fr)" : null,
    cols.duration ? "52px" : null,
    // お気に入りの列：見出しの「お気に入り」（12px の字で5字＝60px）が切れない 68px（v3.3.0 の作者の要望。マークの見た目は次の版のデザインの見直しで）
    cols.favorite ? "68px" : null,
    // ごみ箱の列：見出しの「ごみ箱」（12px の字で3字＝36px）が切れない 44px。窓の幅にかかわらず同じ
    cols.trash ? "44px" : null,
  ]
    .filter(Boolean)
    .join(" ");

  const virt = useVirtualizer({
    count: props.tracks.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowH,
    overscan: 12,
  });
  // この描画で並べた全体の高さ（下の div の高さ。行の高さを替えた直後の描画では、まだ前の高さで並べた値）
  const totalSize = virt.getTotalSize();
  // 行の高さが変わったら（コンパクト表示の切り替え・幅の段）、覚えている行の位置を捨てて測り直す。
  // estimateSize を替えただけでは、仮想スクロールは前の高さで並べたまま（measure で作り直す）。
  // 描く前（useLayoutEffect）に測り直し、前の高さで並んだ行が一瞬見えないようにする。
  // スクロールの位置は、いちばん上に見えていた行が同じ行になるように、新しい高さで置き直す（次の描画のあと）
  const prevRowH = useRef(rowH);
  const keepTopRow = useRef<number | null>(null);
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && prevRowH.current !== rowH && prevRowH.current > 0) keepTopRow.current = el.scrollTop / prevRowH.current;
    prevRowH.current = rowH;
    virt.measure();
  }, [rowH]); // eslint-disable-line
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (keepTopRow.current === null || !el) return;
    // 描いた全体の高さが新しい行の高さに追いついてから置く（追いつく前に置くと、前の短い全体の高さで切られる）
    if (totalSize !== props.tracks.length * rowH) return;
    el.scrollTop = keepTopRow.current * rowH;
    keepTopRow.current = null;
  });

  const selIndexes = () => [...selected].sort((a, b) => a - b);

  const clickRow = (i: number, e: any) => {
    if (e.shiftKey && anchor.current !== null) {
      const [a, b] = [anchor.current, i].sort((x, y) => x - y);
      const s = new Set<number>(e.ctrlKey ? selected : []);
      for (let k = a; k <= b; k++) s.add(k);
      setSelected(s);
    } else if (e.ctrlKey || e.metaKey) {
      const s = new Set(selected);
      if (s.has(i)) s.delete(i);
      else s.add(i);
      setSelected(s);
      anchor.current = i;
    } else {
      setSelected(new Set([i]));
      anchor.current = i;
    }
  };

  const menuFor = (i: number, e: any) => {
    let idx = selIndexes();
    if (!selected.has(i)) {
      idx = [i];
      setSelected(new Set([i]));
      anchor.current = i;
    }
    const ts = idx.map((k) => props.tracks[k]).filter(Boolean);
    openMenu(e, trackMenu({ tracks: ts, list: props.list, playAll: () => props.onPlay(idx[0]), extra: props.extraMenu?.(idx) }));
  };

  const trashRow = (i: number) => {
    const idx = trashTargets(selected, i);
    if (idx.length === 1) {
      setSelected(new Set([i]));
      anchor.current = i;
    }
    void trashTracksFlow(idx.map((k) => props.tracks[k]).filter(Boolean));
  };

  const onKeyDown = (e: any) => {
    if (e.key === "Enter" && selected.size) {
      e.preventDefault();
      props.onPlay(selIndexes()[0]);
    } else if (e.key === "Delete" && selected.size && props.onDeleteKey) {
      e.preventDefault();
      props.onDeleteKey(selIndexes());
    } else if (e.key === "a" && e.ctrlKey) {
      e.preventDefault();
      setSelected(new Set(props.tracks.map((_, i) => i)));
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && !e.ctrlKey) {
      e.preventDefault();
      e.stopPropagation();
      const cur = anchor.current ?? -1;
      const n = Math.max(0, Math.min(props.tracks.length - 1, cur + (e.key === "ArrowDown" ? 1 : -1)));
      setSelected(new Set([n]));
      anchor.current = n;
      virt.scrollToIndex(n);
    }
  };

  const headBtn = (key: string, label: string, cls = "") => (
    <button
      className={cls + (sortState.by === key ? " sorted" : "")}
      disabled={props.sortable}
      onClick={() =>
        sort.set((s) => (s.by === key ? (s.desc ? { by: null, desc: false } : { by: key, desc: true }) : { by: key, desc: false }))
      }
    >
      {label}
      {sortState.by === key ? (sortState.desc ? " ↓" : " ↑") : ""}
    </button>
  );

  if (props.tracks.length === 0) {
    return <div className="empty">{props.empty ?? "曲がありません"}</div>;
  }

  const rows = virt.getVirtualItems().map((v) => {
    const t = props.tracks[v.index];
    return (
      <Row
        key={props.sortableIds ? props.sortableIds[v.index] : `${t.id}:${v.index}`}
        index={v.index}
        start={v.start}
        track={t}
        cols={cols}
        compact={compact}
        playing={pb.track?.id === t.id}
        isPlaying={pb.status === "playing"}
        selected={selected.has(v.index)}
        sortId={props.sortable ? props.sortableIds?.[v.index] : undefined}
        list={props.sortableList}
        dragIds={() => (selected.has(v.index) ? selIndexes().map((k) => props.tracks[k].id) : [t.id])}
        onClick={(e) => clickRow(v.index, e)}
        onDoubleClick={() => props.onPlay(v.index)}
        onContextMenu={(e) => menuFor(v.index, e)}
        onTrash={() => trashRow(v.index)}
        trashLabel={selected.has(v.index) && selected.size > 1 ? `選んでいる ${selected.size} 曲をごみ箱へ移す` : "この曲をごみ箱へ移す"}
        trashBusy={busy}
      />
    );
  });

  return (
    <div className={"track-table" + (compact ? " compact" : "")} style={{ ["--cols" as any]: template, ["--row-h" as any]: `${rowH}px` }}>
      <div className="track-head" role="row">
        {compact ? null : <span />}
        {headBtn("title", "タイトル / アーティスト")}
        {cols.album ? headBtn("album", "アルバム", "col-album") : null}
        {cols.genre ? headBtn("genre", "ジャンル", "col-genre") : null}
        {cols.duration ? (
          <span style={{ textAlign: "right" }}>{headBtn("duration", "時間")}</span>
        ) : null}
        {/* v3.3.0 の直し（作者の要望）：「時間」「ジャンル」と同じ見出しの行に「お気に入り」「ごみ箱」。並べ替えには使わないのでボタンにしない */}
        {cols.favorite ? <span className="col-fav">お気に入り</span> : null}
        {cols.trash ? <span className="col-trash">ごみ箱</span> : null}
      </div>
      <div className="track-scroll" ref={scrollRef} tabIndex={0} onKeyDown={onKeyDown} role="grid" aria-rowcount={props.tracks.length}>
        <div style={{ height: totalSize, position: "relative" }}>
          {props.sortable && props.sortableIds ? (
            <SortableContext items={props.sortableIds} strategy={verticalListSortingStrategy}>
              {rows}
            </SortableContext>
          ) : (
            rows
          )}
        </div>
      </div>
    </div>
  );
}

interface RowProps {
  index: number;
  start: number;
  track: Track;
  cols: { album: boolean; genre: boolean; duration: boolean; favorite: boolean; trash: boolean };
  /** v3.4.0：コンパクト表示（ジャケットなし・1行） */
  compact: boolean;
  playing: boolean;
  isPlaying: boolean;
  selected: boolean;
  sortId?: string;
  list?: "playlist" | "queue";
  dragIds: () => number[];
  onClick: (e: any) => void;
  onDoubleClick: () => void;
  onContextMenu: (e: any) => void;
  onTrash: () => void;
  trashLabel: string;
  trashBusy: boolean;
}

function Row(p: RowProps) {
  return p.sortId ? <SortableRow {...p} sortId={p.sortId} /> : <DraggableRow {...p} />;
}

function dragData(p: RowProps): DragData {
  const ids = p.dragIds();
  return {
    kind: "tracks",
    ids,
    label: ids.length > 1 ? `${ids.length}曲` : p.track.title,
    list: p.list,
    index: p.index,
  };
}

function DraggableRow(p: RowProps) {
  const d = useDraggable({ id: `row:${p.track.id}:${p.index}`, data: dragData(p) as any });
  return (
    <RowBody
      p={p}
      setRef={d.setNodeRef}
      listeners={d.listeners}
      attributes={d.attributes}
      dragging={d.isDragging}
      ty={0}
      transition={undefined}
    />
  );
}

function SortableRow(p: RowProps & { sortId: string }) {
  const s = useSortable({ id: p.sortId, data: dragData(p) as any });
  return (
    <RowBody
      p={p}
      setRef={s.setNodeRef}
      listeners={s.listeners}
      attributes={s.attributes}
      dragging={s.isDragging}
      ty={s.transform?.y ?? 0}
      transition={s.transition ?? undefined}
    />
  );
}

function RowBody(props: { p: RowProps; setRef: any; listeners: any; attributes: any; dragging: boolean; ty: number; transition?: string }) {
  const { p } = props;
  const t = p.track;
  const toggleFav = (e: any) => {
    e.stopPropagation();
    api.setFavorite([t.id], !t.favorite).catch((err) => notify({ level: "error", error: err }));
  };
  const cls =
    "track-row" +
    (p.selected ? " selected" : "") +
    (p.playing ? " playing" : "") +
    (t.missing ? " missing" : "") +
    (props.dragging ? " dragging" : "");
  const attrs = { ...props.attributes };
  delete attrs.role; // 行の役割は grid の row にする
  delete attrs.tabIndex;
  return (
    <div
      ref={props.setRef}
      {...attrs}
      {...props.listeners}
      role="row"
      aria-selected={p.selected}
      className={cls}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        transform: `translate3d(0, ${p.start + props.ty}px, 0)`,
        transition: props.transition,
      }}
      onClick={p.onClick}
      onDoubleClick={p.onDoubleClick}
      onContextMenu={p.onContextMenu}
      title={t.missing ? "ファイルが見つかりません" : undefined}
    >
      {p.compact ? null : <Artwork path={t.artPath} className="t-art" />}
      {p.compact ? (
        // コンパクト表示：「タイトル — アーティスト」を1行に。入りきらなければ行の終わりを省略記号で切る
        <div className="t-main">
          <div className="t-title">
            {p.playing ? <IconBars className="eq" title={p.isPlaying ? "再生中" : "一時停止中"} /> : null}
            <span className="t-line">
              <span className="t-line-title">{t.title}</span>
              <span className="t-line-artist"> — {t.artist || "不明なアーティスト"}</span>
            </span>
          </div>
        </div>
      ) : (
        <div className="t-main">
          <div className="t-title">
            {p.playing ? <IconBars className="eq" title={p.isPlaying ? "再生中" : "一時停止中"} /> : null}
            <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{t.title}</span>
          </div>
          <div className="t-artist">{t.artist || "不明なアーティスト"}</div>
        </div>
      )}
      {p.cols.album ? <div className="t-cell col-album">{t.album}</div> : null}
      {p.cols.genre ? <div className="t-cell col-genre">{t.genre}</div> : null}
      {p.cols.duration ? <div className="t-dur">{formatTime(t.durationMs)}</div> : null}
      {p.cols.favorite ? (
        <div className="t-fav">
          <button
            className={"icon-btn small" + (t.favorite ? " on" : "")}
            onClick={toggleFav}
            onPointerDown={(e) => e.stopPropagation()}
            title={t.favorite ? "お気に入りから外す" : "お気に入りに追加"}
            aria-pressed={t.favorite}
          >
            <IconStar filled={t.favorite} />
          </button>
        </div>
      ) : null}
      {p.cols.trash ? (
        <div className="t-trash">
          {/* 中身は列の見出し「ごみ箱」で分かるので、ボタンはアイコンだけ。読み上げ（aria-label）と吹き出し（title）で何をするかを伝える */}
          <button
            className="icon-btn small"
            onClick={(e) => {
              e.stopPropagation();
              if (!p.trashBusy) p.onTrash();
            }}
            onDoubleClick={(e) => e.stopPropagation()}
            onPointerDown={(e) => e.stopPropagation()}
            disabled={p.trashBusy}
            title={`${p.trashLabel}（曲ファイルを Windows のごみ箱へ。確認あり）`}
            aria-label={p.trashLabel}
          >
            <IconTrash />
          </button>
        </div>
      ) : null}
    </div>
  );
}

export function useTrackIdsMemo(tracks: Track[]) {
  return useMemo(() => tracks.map((t) => t.id), [tracks]);
}
