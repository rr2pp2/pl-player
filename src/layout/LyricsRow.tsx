// 再生バーの下の「流れる歌詞の列」（v3.0.2）。歌詞を横一列に並べ、行が変わったときだけ列全体を左へ送る。
// 並べるもの（今・次・その先・歌い終わった行・状態の印）は stores/lyricsTime.ts の buildRow（画面に依存しない関数）。
// 再生位置は毎フレーム見ない。stores/lyrics.ts の activeLine・nextLine（今の行・次の行が変わったときだけ書かれる）だけを見る。
import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { lyricsRowHint, lyricsRowText } from "../messages/ja";
import { activeLine, lyrics, nextLine } from "../stores/lyrics";
import { buildRow, buildRowUnits, type RowItem, type RowModel, type RowSource, type RowUnit } from "../stores/lyricsTime";
import { playback } from "../stores/playback";
import { settings } from "../stores/settings";
import { useStore } from "../stores/store";
import { comboLabel } from "../shortcuts/shortcuts";
import { LyricParts } from "../components/LyricParts";

const NO_UNITS: RowUnit[] = [];

function reducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/** 列の今の横の位置（動いている途中なら、その時点の値） */
function currentX(el: HTMLElement): number {
  const t = getComputedStyle(el).transform;
  if (!t || t === "none") return 0;
  try {
    return new DOMMatrixReadOnly(t).m41;
  } catch {
    return 0;
  }
}

function Item(props: { item: RowItem; hint: string }) {
  const { item } = props;
  let body: any;
  if (item.kind === "status") body = lyricsRowText[item.status ?? ""] ?? "";
  else if (item.kind === "hint") body = props.hint;
  else body = <LyricParts parts={item.parts} />;
  return (
    <span
      className={"lyrics-row-item is-" + item.kind}
      data-key={item.key}
      aria-current={item.kind === "current" ? "true" : undefined}
      title={item.text || undefined}
    >
      <span className="lyrics-row-text">{body}</span>
      {item.sep ? (
        <span className="lyrics-row-sep" aria-hidden="true">
          ›
        </span>
      ) : null}
    </span>
  );
}

export function LyricsRow() {
  const st = useStore(lyrics);
  const t = useStore(playback).track;
  const a = useStore(activeLine);
  const nx = useStore(nextLine);
  const lyricsKey = useStore(settings).shortcuts.toggleLyrics ?? "";
  const hint = lyricsRowHint(lyricsKey ? comboLabel(lyricsKey) : "");

  const p = st.payload && t && st.payload.trackId === t.id ? st.payload : null;
  const status: RowSource["status"] = !t ? "idle" : st.trackId !== t.id ? "loading" : st.status === "error" ? "error" : p ? "ready" : "loading";
  // 単位の一覧は、歌詞か補正が変わったときだけ作る
  const units = useMemo(
    () => (p && p.source && p.format === "timed" ? buildRowUnits(p.lines, p.userDelayMs) : NO_UNITS),
    [p?.lines, p?.userDelayMs, p?.format, p?.source],
  );
  // 並びは、今の行・次の行・単位の一覧が変わったときだけ作り直す
  const model: RowModel = useMemo(
    () =>
      buildRow(
        { status, source: p?.source ?? null, format: p?.format ?? null, lines: p?.lines ?? [] },
        units,
        a,
        nx,
        `${t?.id ?? ""}:`,
      ),
    [status, p?.source, p?.format, p?.lines, units, a, nx, t?.id],
  );

  const winRef = useRef<HTMLDivElement>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const modelRef = useRef(model);
  modelRef.current = model;
  /** 前に置いたときの、各要素の左端（key → 列の中の位置）と、そろえた要素 */
  const prev = useRef<{ offsets: Map<string, number>; anchor: string | null } | null>(null);

  /**
   * 今の行（か状態の印）の左端を、列の左寄り（--lyrics-row-anchor）に合わせる。
   * animate のとき：前の並びと今の並びに共通する要素を1つ選び、まずその要素が画面上で動かない位置に置き直してから
   * （並べる範囲がずれて左の要素が消えても跳ばないように）、--dur-panel で目的の位置へ送る。
   * 動きを減らす設定・曲が変わった（共通の要素が無い）・幅が変わったときは、すぐに置く
   */
  const align = (animate: boolean) => {
    const track = trackRef.current;
    const win = winRef.current;
    if (!track || !win) return;
    const anchorX = parseFloat(getComputedStyle(win).getPropertyValue("--lyrics-row-anchor")) || 0;
    const offsets = new Map<string, number>();
    for (const el of Array.from(track.children) as HTMLElement[]) {
      const k = el.dataset.key;
      if (k) offsets.set(k, el.offsetLeft);
    }
    const m = modelRef.current;
    const target = anchorX - (m.anchorKey ? (offsets.get(m.anchorKey) ?? 0) : 0);
    const before = prev.current;
    let from: number | null = null;
    if (animate && before && !reducedMotion()) {
      for (const k of [before.anchor, ...offsets.keys()]) {
        if (k && before.offsets.has(k) && offsets.has(k)) {
          from = currentX(track) + before.offsets.get(k)! - offsets.get(k)!;
          break;
        }
      }
    }
    prev.current = { offsets, anchor: m.anchorKey };
    track.style.transition = "none";
    track.style.transform = `translateX(${from ?? target}px)`;
    void track.offsetWidth; // ここで一度置き切る（次の書き換えだけを動かす）
    track.style.transition = "";
    if (from !== null) track.style.transform = `translateX(${target}px)`;
  };

  useLayoutEffect(() => {
    align(true);
  }, [model]);

  // 幅が変わったら置き直す（動かさない）
  useEffect(() => {
    const win = winRef.current;
    if (!win || typeof ResizeObserver === "undefined") return;
    let raf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => align(false));
    });
    ro.observe(win);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <section className="lyrics-row" aria-label="流れる歌詞">
      <div className="lyrics-row-window" ref={winRef}>
        <div className="lyrics-row-track" ref={trackRef}>
          {model.items.map((it) => (
            <Item key={it.key} item={it} hint={hint} />
          ))}
        </div>
      </div>
    </section>
  );
}
