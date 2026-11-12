// 歌詞画面（v3）。中央の画面を切り替えて出す（曲一覧などと同じ扱い）。
// 広いとき：左の列にジャケット・曲情報・出どころ・形式・補正、右に歌詞のカード。
// 狭いとき（600px 未満）：1列。曲情報は横1行、補正は見出しの下に1行。
import { useEffect, useRef, type PointerEvent } from "react";
import { lyricsSourceLabel, lyricsWarning } from "../messages/ja";
import {
  activeLine,
  closeLyricsView,
  follow,
  loadLyrics,
  lyrics,
  openLyricsEditor,
  pauseFollow,
  resumeFollow,
} from "../stores/lyrics";
import { playback } from "../stores/playback";
import { useStore } from "../stores/store";
import { Artwork } from "../components/Artwork";
import { IconBack, IconEdit } from "../components/Icons";
import { LyricsDelay } from "../components/LyricsDelay";
import { LyricsFailed, LyricsNone, LyricsNoTrack } from "../components/LyricsEmpty";
import { LyricsLines } from "../components/LyricsLines";
import { useLayoutSize } from "../components/useLayoutSize";

const SCROLL_KEYS = new Set(["PageUp", "PageDown", "Home", "End"]);
// 一部の時刻が読めない警告はカードの上に出す。ほかの警告は小さく添える
const INLINE_WARNINGS = new Set(["timestamp_invalid"]);

function reducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

export function LyricsView() {
  const st = useStore(lyrics);
  const pb = useStore(playback);
  const active = useStore(activeLine);
  const fl = useStore(follow);
  const size = useLayoutSize();
  const narrow = size.width < 600;
  const scrollRef = useRef<HTMLDivElement>(null);
  const t = pb.track;
  const p = st.payload && t && st.payload.trackId === t.id ? st.payload : null;
  const timed = p?.format === "timed";

  // 開いたときは追従から始める
  useEffect(() => {
    resumeFollow();
  }, []);

  // Escape で閉じる（小窓・メニュー・文字入力の最中は除く）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (document.querySelector(".modal-backdrop, .menu")) return;
      closeLyricsView();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // 今の行が変わったときだけ、その行を中央付近へ
  useEffect(() => {
    if (!timed || fl.paused || !active) return;
    const box = scrollRef.current;
    const el = box?.querySelector<HTMLElement>(`[data-line="${active.start}"]`);
    if (!box || !el) return;
    const top = el.offsetTop - box.clientHeight / 2 + el.offsetHeight / 2;
    box.scrollTo({ top: Math.max(0, top), behavior: reducedMotion() ? "auto" : "smooth" });
  }, [active?.start, active?.end, fl.paused, timed, p]);

  // 手で動かしたら追従を止める（ホイール・タッチ・PageUp/PageDown/Home/End・スクロールバー）
  const manual = () => {
    if (timed) pauseFollow();
  };
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget;
    // スクロールバーの上（中身の幅より右）を押した。行を押したとき（シーク）は手の操作に数えない
    if (e.target === box && e.nativeEvent.offsetX >= box.clientWidth) manual();
  };

  const sourceBadge = p?.source ? <span className="lyrics-badge">{lyricsSourceLabel[p.source]}</span> : null;
  const formatBadge = p?.format ? <span className="lyrics-badge">{p.format === "timed" ? "時刻つき" : "時刻なし"}</span> : null;
  const notes = (p?.warnings ?? []).filter((w) => !INLINE_WARNINGS.has(w));

  let body: any;
  if (!t) body = <LyricsNoTrack />;
  else if (st.status === "error") body = <LyricsFailed text={st.error?.text ?? ""} detail={st.error?.detail ?? ""} onRetry={() => void loadLyrics(t.id)} />;
  else if (!p) body = <div className="empty">読み込み中…</div>;
  else if (!p.source) body = <LyricsNone warnings={p.warnings} trackId={p.trackId} />;
  else body = <LyricsLines payload={p} active={timed ? active : null} />;

  return (
    <div className={"content lyrics-view" + (narrow ? " narrow" : "")}>
      <div className="view-head lyrics-head">
        <button className="icon-btn" onClick={closeLyricsView} title="戻る (Esc)" aria-label="歌詞を閉じて戻る">
          <IconBack />
        </button>
        <h1 className="view-title">歌詞</h1>
        {p?.source ? <span className={"lyrics-badge" + (timed ? " accent" : "")}>{timed ? "同期あり" : "同期なし"}</span> : null}
        <div className="view-actions">
          {timed && !fl.paused ? <span className="hint-text">自動で追従しています</span> : null}
          <button className="btn" onClick={openLyricsEditor} disabled={!t}>
            <IconEdit /> 歌詞を編集
          </button>
        </div>
      </div>
      {timed && fl.paused ? (
        <div className="lyrics-follow-bar" role="status">
          <span>追従を止めています</span>
          <button className="btn small-btn" onClick={resumeFollow}>
            追従再開
          </button>
        </div>
      ) : null}
      {narrow && t && p?.source ? <LyricsDelay value={p.userDelayMs} compact /> : null}
      {narrow && t ? (
        <div className="lyrics-compact-info">
          <Artwork path={t.artPath} className="lyrics-compact-art" />
          <div className="lyrics-compact-text">
            <span className="lyrics-track-title">{t.title}</span>
            <span className="lyrics-track-artist">{t.artist || "不明なアーティスト"}</span>
          </div>
          {sourceBadge}
        </div>
      ) : null}
      <div className="lyrics-layout">
        {!narrow && t ? (
          <aside className="lyrics-side">
            <Artwork path={t.artPath} className="lyrics-side-art" />
            <div className="lyrics-track-title" title={t.title}>
              {t.title}
            </div>
            <div className="lyrics-track-artist">{t.artist || "不明なアーティスト"}</div>
            {sourceBadge || formatBadge ? (
              <div className="row">
                {sourceBadge}
                {formatBadge}
              </div>
            ) : null}
            {p?.source ? <LyricsDelay value={p.userDelayMs} /> : null}
          </aside>
        ) : null}
        <div
          ref={scrollRef}
          className={"lyrics-card" + (timed ? " timed" : "")}
          tabIndex={-1}
          onWheel={manual}
          onTouchMove={manual}
          onPointerDown={onPointerDown}
          onKeyDown={(e) => {
            if (SCROLL_KEYS.has(e.key)) manual();
          }}
        >
          {notes.length && p?.source ? (
            <div className="lyrics-notes">
              {notes.map((w) => (
                <div key={w}>{lyricsWarning(w)}</div>
              ))}
            </div>
          ) : null}
          {body}
        </div>
      </div>
    </div>
  );
}
