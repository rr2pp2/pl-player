// 曲一覧。見えている行だけを描く（大量の曲でも重くならないように）。
// 列は幅に合わせて出し入れする（Genre → Album の順で隠す）。v3.8.0：幅は曲一覧の側の幅（ウィンドウの幅ではなく）。
// v3.3.0：設定の「ごみ箱へ移すボタン」がオンのときだけ、行の右端に「ごみ箱へ移す」を出す（既定はオフ）。
// 出すのはライブラリの曲一覧だけ（props.list。プレイリスト・キューの一覧には、行のボタンも右クリックのメニューも出さない）。
// 対象は右クリックのメニューと同じ決まり：押した行が選んでいる行の中にあれば選んでいる全部、そうでなければその行だけ。
// v3.4.0：コンパクト表示（設定の ui.compactList。決まりは src\layout\density.ts）：ジャケットの列を出さず、行を詰める。
// v3.5.0（作者の要望）：コンパクト表示では、タイトルとアーティストを別の列にする（見出しも「タイトル」「アーティスト」）。
// 選択・再生中の印・ドラッグ・右クリック・お気に入り・ごみ箱の列は今までどおり。
// v3.5.0（作者の要望）：設定の「曲一覧の列」の「アーティスト」（ui.columns.artist。無ければオン）。コンパクト表示では列を、
// 通常の表示ではタイトルの下の段を出し入れする（行の高さは変えない）。幅では隠さない。
// v3.6.0：年間まとめ（src\views\YearlyView.tsx）では、順位の列（左端）と期間中の回数の列（時間の前）を足す（props.ranking）。
// 並びは Rust が決めた順位の順のまま（props.fixedOrder：見出しで並べ替えない）。
// v3.7.0（デザインの見直し）：通常の行 60px・ジャケット 44px（useLayoutSize。CSS の --row-h・--row-art と同じ値）。選んだ行の左の線・
// お気に入りとごみ箱の 32px の面（コンパクトでは 28px）・お気に入りのオンの星の黄色は base.css だけで描く（列・見出し・処理は変えない）。
// v3.8.0：アーティストで並べ替えたまま、アーティストの見出しが無いときは、タイトルの見出しに矢印と説明を出す（src\layout\sortHead.ts）。
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
import { useListLayout } from "./useLayoutSize";
import { Tip } from "./Tip";
import { sortHead, SORT_STAND_IN_HEADS } from "../layout/sortHead";

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
  /** v3.6.0：年間まとめ。行ごとの順位と期間中の回数（tracks と同じ並び）。あれば「順位」「回数」の列を出す */
  ranking?: { rank: number; plays: number }[];
  /** v3.6.0：並びが決まっている一覧（年間まとめ）。見出しを押しても並べ替えず、矢印も出さない */
  fixedOrder?: boolean;
}

export function TrackTable(props: Props) {
  // v3.8.0：列・ジャケット・行の高さ・自動のコンパクトは、曲一覧の側の幅で決める（ウィンドウの幅ではなく。src\layout\density.ts の LIST_BELOW）
  const size = useListLayout();
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

  // v3.4.0：コンパクト表示か（v3.8.0：自動のコンパクトは曲一覧の側の幅で決める）と、行の高さ（仮想スクロールの1行の大きさ）
  const compact = compactListOn(ui, size.width);
  const rowH = trackRowHeight(compact, size.rowH);
  // v3.5.0：アーティストを出すか（設定。古い設定でキーが無ければオン）。アルバム・ジャンルと違い、幅の段では隠さない
  const artistOn = cfg.artist !== false;
  const cols = {
    // v3.6.0：年間まとめの順位（左端）と期間中の回数（時間の前）。どちらもコンパクトにも幅の段にもかかわらず出す
    rank: !!props.ranking,
    plays: !!props.ranking,
    // アーティストの列（コンパクト表示のとき）と、タイトルの下の段（通常の表示のとき）。どちらか一方だけが true になる
    artist: compact && artistOn,
    artistLine: !compact && artistOn,
    album: cfg.album !== false && !size.hideAlbum,
    genre: cfg.genre !== false && !size.hideGenre,
    duration: cfg.duration !== false,
    favorite: cfg.favorite !== false,
    trash: trashButtonShown(ui, props.list),
  };
  const template = [
    // 順位の列（v3.6.0）：見出しの「順位」（12px の字で2字）と「100」が入る 40px
    cols.rank ? "40px" : null,
    // ジャケットの列：コンパクト表示では出さない（見出しの行の空き枠も出さない）
    compact ? null : `${size.artSize}px`,
    // タイトルの列（通常の表示では「タイトル / アーティスト」の2段。アーティストを切ればタイトルだけ）
    "minmax(0, 3fr)",
    // アーティストの列：コンパクト表示で、設定がオンのときだけ（v3.5.0）。アルバムの列と同じ 2fr（どちらも名前で、長さが近い）。
    // タイトルは3fr のまま、主に読む列として一番広く取る。アルバム・ジャンルが消える幅（900・820px 未満）では、タイトル：アーティスト＝3：2 で分ける
    cols.artist ? "minmax(0, 2fr)" : null,
    cols.album ? "minmax(0, 2fr)" : null,
    cols.genre ? "minmax(0, 1.2fr)" : null,
    // 期間中の回数の列（v3.6.0）：見出しの「回数」と4桁の数が入る 44px（1年に 1万回は、1日に 27 回を超えるので見込まない）
    cols.plays ? "44px" : null,
    cols.duration ? "52px" : null,
    // お気に入りの列：見出しの「お気に入り」（12px の字で5字＝60px）が切れない 68px（v3.3.0 の作者の要望）。
    // v3.5.0 で一度、見出しを星のマークにしたが、作者の要望で文字に戻した（星は行のボタンとサイドバーの項目に出す）。
    // v3.7.0 のデザインの見直しでも、見出しは文字のまま 68px（星＋文字の 84px の案は採らない。作者の決定）
    cols.favorite ? "68px" : null,
    // ごみ箱の列：見出しの「ごみ箱」（12px の字で3字＝36px）が切れない 44px。窓の幅にかかわらず同じ。
    // v3.7.0：行のボタンを 32px の面にしたが、44px に収まるので幅は変えない（見出しは文字「ごみ箱」のまま。作者の決定）
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

  // v3.6.0：並びが決まっている一覧（年間まとめ）では、押せず、矢印も出さない（並べ替えの設定は一覧ごとに navigate で戻る）。
  // v3.8.0：アーティストで並べ替えたまま、アーティストの見出しが無いとき（通常の表示・コンパクトでアーティストの列を切ったとき）は、
  // 並びはそのままで、タイトルの見出しに矢印を出し、吹き出しと読み上げで「アーティストで並べ替え中」と伝える（src\layout\sortHead.ts）
  const mark = sortHead(sortState, { fixedOrder: !!props.fixedOrder, artistColumn: cols.artist });
  const headBtn = (key: string, label: string, cls = "") => {
    const on = mark.at === key;
    const btn = (
      <button
        className={cls + (on ? " sorted" : "")}
        disabled={props.sortable || props.fixedOrder}
        onClick={() =>
          sort.set((s) => (s.by === key ? (s.desc ? { by: null, desc: false } : { by: key, desc: true }) : { by: key, desc: false }))
        }
      >
        {label}
        {on ? (mark.desc ? " ↓" : " ↑") : ""}
      </button>
    );
    // 代わりに矢印を出すことがある見出し（タイトル）だけ、いつも吹き出しで包む（文の有無で包み方を変えると、ボタンが作り直されてフォーカスが外れる。Tip.tsx）
    // 吹き出しは見出しの下に、左端をそろえて出す（ボタンが列の幅いっぱいなので、右に出すと文字から遠く隣の見出しに重なる）
    return SORT_STAND_IN_HEADS.includes(key) ? <Tip text={on ? mark.hint : null} place="below">{btn}</Tip> : btn;
  };

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
        rank={props.ranking?.[v.index]?.rank}
        plays={props.ranking?.[v.index]?.plays}
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
    <div
      className={"track-table" + (compact ? " compact" : "") + (size.tight ? " tight" : "")}
      style={{ ["--cols" as any]: template, ["--row-h" as any]: `${rowH}px`, ["--row-art" as any]: `${size.artSize}px` }}
    >
      <div className="track-head" role="row">
        {cols.rank ? <span className="col-rank">順位</span> : null}
        {compact ? null : <span />}
        {/* コンパクト表示では「タイトル」「アーティスト」の2つの見出し（v3.5.0）。アーティストの並べ替えは Rust の order_clause の "artist"（アーティスト→アルバム→ディスク→曲番号）。
            通常の表示は「タイトル / アーティスト」。アーティストを切ったときは、どちらの表示でも「タイトル」だけ */}
        {cols.artistLine ? headBtn("title", "タイトル / アーティスト") : headBtn("title", "タイトル")}
        {cols.artist ? headBtn("artist", "アーティスト", "col-artist") : null}
        {cols.album ? headBtn("album", "アルバム", "col-album") : null}
        {cols.genre ? headBtn("genre", "ジャンル", "col-genre") : null}
        {cols.plays ? <span className="col-plays">回数</span> : null}
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
  /** v3.6.0：年間まとめの順位と期間中の回数（cols.rank・cols.plays のときだけ使う） */
  rank?: number;
  plays?: number;
  /** artist：アーティストの列（コンパクト表示）。artistLine：タイトルの下のアーティストの段（通常の表示）。rank・plays：年間まとめ（v3.6.0） */
  cols: {
    rank: boolean;
    artist: boolean;
    artistLine: boolean;
    album: boolean;
    genre: boolean;
    plays: boolean;
    duration: boolean;
    favorite: boolean;
    trash: boolean;
  };
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
      {p.cols.rank ? <div className="t-rank">{p.rank}</div> : null}
      {p.compact ? null : <Artwork path={t.artPath} className="t-art" />}
      {p.compact ? (
        // コンパクト表示：タイトルだけを1行に（アーティストは次の列）。入りきらなければ省略記号で切る
        <div className="t-main">
          <div className="t-title">
            {p.playing ? <IconBars className="eq" title={p.isPlaying ? "再生中" : "一時停止中"} /> : null}
            <span className="t-line">{t.title}</span>
          </div>
        </div>
      ) : (
        <div className="t-main">
          <div className="t-title">
            {p.playing ? <IconBars className="eq" title={p.isPlaying ? "再生中" : "一時停止中"} /> : null}
            <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{t.title}</span>
          </div>
          {/* アーティストを切ったときは段を出さない（行の高さはそのまま。タイトルは行の縦の真ん中に来る） */}
          {p.cols.artistLine ? <div className="t-artist">{t.artist || "不明なアーティスト"}</div> : null}
        </div>
      )}
      {/* アーティストの列（コンパクト表示で、設定がオンのときだけ。v3.5.0）。見た目はアルバム・ジャンルの列と同じ */}
      {p.cols.artist ? <div className="t-cell col-artist">{t.artist || "不明なアーティスト"}</div> : null}
      {p.cols.album ? <div className="t-cell col-album">{t.album}</div> : null}
      {p.cols.genre ? <div className="t-cell col-genre">{t.genre}</div> : null}
      {p.cols.plays ? <div className="t-plays" title={`期間中に ${p.plays} 回再生`}>{p.plays}</div> : null}
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
