// 下：再生バー（画面下部に固定）。
// v3.3.0：シャッフル・リピート・音量・流れる歌詞の列のオン・オフ・歌詞のボタンは、設定で隠せる（visibility.ts）。
// 再生・一時停止・前・次・シークバーは隠せない。ボタンを隠しても、ショートカット（Ctrl+L など）はそのまま効く。
// v3.8.0：再生バーの幅に中身が入るかどうかで、3列と1段を切り替える（決めるのは src\layout\playerLayout.ts）。
//   3列：左＝ジャケット・曲名・アーティスト（・今の歌詞の1行）／中＝操作のボタンとシークバー（再生ボタンは中央に固定）／右＝全体の音量・歌詞のボタン・「···」
//   1段：上の縁にシークバーの細い線／左＝ジャケット・曲名・今の時間 / 長さ／中＝前・再生・次／右＝「···」（全体の音量・シャッフル・リピート・歌詞のボタン）
//   入りきらないボタンは「···」へ。設定で隠したボタンは「···」にも出さない。
//   出力ボタン（v3.8.0。OutputButton.tsx）は右の列の全体の音量の左に、固定幅で入れる（playerLayout の outputW。1段ではアイコンだけ）。
//   設定の「再生バーのボタン」で隠せる（visibility.ts の outputs）。隠したら幅も取らない。
import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "../ipc/commands";
import { playback } from "../stores/playback";
import { navigate, route } from "../stores/library";
import { activeLine, isShown, lyrics, nextLine, openLyricsView, toggleLyricsView, type ActiveRange } from "../stores/lyrics";
import { rangeParts } from "../stores/lyricsTime";
import type { LyricsLine } from "../ipc/types";
import { lyricsRowOn, settings, toggleLyricsRow } from "../stores/settings";
import { useStore } from "../stores/store";
import { comboLabel } from "../shortcuts/shortcuts";
import { Artwork } from "../components/Artwork";
import { SeekBar, SeekTime } from "../components/SeekBar";
import { VolumeControl } from "../components/VolumeControl";
import { IconLyrics, IconLyricsRow, IconMore, IconNext, IconPause, IconPlay, IconPrev, IconRepeat, IconShuffle } from "../components/Icons";
import { LyricParts } from "../components/LyricParts";
import { openMenu } from "../components/ContextMenu";
import { trackMenu } from "../components/trackActions";
import { playerButtonShown } from "./visibility";
import { PLAYER, playerLayout, type MovableButton } from "./playerLayout";
import { OutputButton } from "./OutputButton";

const REPEAT_LABEL = { off: "リピート：切", all: "リピート：全曲", one: "リピート：1曲" } as const;

/** 同じ時刻のまとまりを1つの文にする（「消す」合図と空の行は外す） */
function rangeText(lines: LyricsLine[], r: ActiveRange | null): string {
  if (!r) return "";
  return lines
    .slice(r.start, r.end + 1)
    .filter(isShown)
    .map((l) => l.text)
    .join(" ／ ");
}

/**
 * アーティスト名の下に出す今の歌詞の1行（時刻つきのとき）と、その右に薄く次の行。
 * 時刻なしは「時刻同期なし」、歌詞なしは何も出さない。
 * 1行に収まらないときは右から切る（次の行が先に隠れ、今の行は残る）。狭い幅では次の行を出さない（base.css）
 * 括弧の中（掛け合い・合いの手）は一段薄く・小さく出す（v3.0.2。分け方は lyricsTime.ts の rangeParts）。
 * 流れる歌詞の列がオンのあいだは出さない（PlayerBar）
 */
function PlayerLyricLine(props: { trackId: number }) {
  const st = useStore(lyrics);
  const a = useStore(activeLine);
  const nx = useStore(nextLine);
  const p = st.payload;
  if (!p || p.trackId !== props.trackId || !p.source) return null;
  if (p.format !== "timed") return <div className="player-lyric muted">時刻同期なし</div>;
  const text = rangeText(p.lines, a);
  const next = rangeText(p.lines, nx);
  const title = [text, next ? `次：${next}` : ""].filter(Boolean).join("\n");
  return (
    <button className="player-lyric" onClick={openLyricsView} title={title ? `${title}\n（歌詞を開く）` : "歌詞を開く"}>
      {text && a ? (
        <span className="player-lyric-now">
          <LyricParts parts={rangeParts(p.lines, a.start, a.end)} />
        </span>
      ) : null}
      {next ? (
        <span className={"player-lyric-next" + (text ? "" : " alone")}>
          {text ? (
            <span className="player-lyric-sep" aria-hidden="true">
              ›
            </span>
          ) : null}
          {nx ? <LyricParts parts={rangeParts(p.lines, nx.start, nx.end)} /> : null}
        </span>
      ) : null}
      {!text && !next ? "\u00a0" : null}
    </button>
  );
}

/** 再生バーの幅を測る（ResizeObserver）。最初の描画はウィンドウの幅から見積もる（.app の左右の余白 10px） */
function useBarWidth(ref: { current: HTMLElement | null }): number {
  const [w, setW] = useState(() => Math.max(0, window.innerWidth - 20));
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const put = () => setW(el.getBoundingClientRect().width);
    put();
    const ro = new ResizeObserver(put);
    ro.observe(el);
    return () => ro.disconnect();
  }, []); // eslint-disable-line
  return w;
}

/** 「···」（その他）：押すと上に開く小窓。全体の音量と、入りきらなかったボタンを文字つきで出す */
function PlayerMore(props: { items: MovableButton[]; render: (b: MovableButton) => any }) {
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ right: number; bottom: number }>({ right: 8, bottom: 8 });
  const close = (focusBack: boolean) => {
    setOpen(false);
    if (focusBack) btn.current?.focus();
  };
  useLayoutEffect(() => {
    if (!open || !btn.current) return;
    const r = btn.current.getBoundingClientRect();
    setPos({ right: Math.max(8, window.innerWidth - r.right), bottom: Math.max(8, window.innerHeight - r.top + 8) });
    // 開いたら中の最初の操作へ（キーボードで続けて選べるように）
    pop.current?.querySelector<HTMLElement>("button, input")?.focus();
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (pop.current?.contains(t) || btn.current?.contains(t)) return;
      close(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close(true);
      }
    };
    const onLeave = () => close(false);
    window.addEventListener("mousedown", onDown);
    // 先に受ける（capture）：歌詞の画面の Esc（閉じる）より前に、この小窓だけを閉じる（LyricsView は defaultPrevented を見て何もしない）
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", onLeave);
    window.addEventListener("blur", onLeave);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("resize", onLeave);
      window.removeEventListener("blur", onLeave);
    };
  }, [open]);
  return (
    <>
      <button
        ref={btn}
        className={"icon-btn player-more" + (open ? " on" : "")}
        onClick={() => setOpen((o) => !o)}
        title="その他"
        aria-label="その他"
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        <IconMore />
      </button>
      {open
        ? createPortal(
            <div ref={pop} className="player-more-pop" role="dialog" aria-label="その他の操作" style={{ right: pos.right, bottom: pos.bottom }}>
              {props.items.map((b) => (
                <div key={b} className="player-more-item">
                  {props.render(b)}
                </div>
              ))}
            </div>,
            document.body,
          )
        : null}
    </>
  );
}

export function PlayerBar() {
  const s = useStore(playback);
  const t = s.track;
  const playing = s.status === "playing";
  const lyricsOpen = useStore(route).view === "lyrics";
  const st = useStore(settings);
  const lyricsKey = comboLabel(st.shortcuts.toggleLyrics ?? "");
  const rowKey = comboLabel(st.shortcuts.toggleLyricsRow ?? "");
  // 流れる歌詞の列（v3.0.2）がオンのあいだは、下の小さな歌詞の1行を出さない
  const rowOn = lyricsRowOn(st);
  const shown = (id: string) => playerButtonShown(st.ui, id);
  const ref = useRef<HTMLElement>(null);
  const barW = useBarWidth(ref);
  const outputsOn = shown("outputs");
  const L = playerLayout(barW, shown, outputsOn ? PLAYER.output3 : 0);
  const one = L.mode === "one";

  // 置き場所が動くボタン。inMenu は「···」の中（文字つきの行）
  const movable = (b: MovableButton, inMenu: boolean) => {
    if (b === "volume") return <VolumeControl />;
    const item = (o: { on: boolean; label: string; title: string; onClick: () => void; icon: any; cls?: string }) =>
      inMenu ? (
        <button className="menu-item" onClick={o.onClick} title={o.title} aria-pressed={o.on}>
          {o.icon}
          <span style={{ flex: 1 }}>{o.label}</span>
        </button>
      ) : (
        <button
          className={"icon-btn" + (o.cls ? " " + o.cls : "") + (o.on ? " on" : "")}
          onClick={o.onClick}
          title={o.title}
          aria-label={o.label}
          aria-pressed={o.on}
        >
          {o.icon}
        </button>
      );
    if (b === "shuffle")
      return item({
        on: s.shuffle,
        label: inMenu ? `シャッフル：${s.shuffle ? "入" : "切"}` : "シャッフル",
        title: s.shuffle ? "シャッフル：入 (S)" : "シャッフル：切 (S)",
        onClick: () => void api.setShuffle(!s.shuffle),
        icon: <IconShuffle on={s.shuffle} />,
      });
    if (b === "repeat")
      return item({
        on: s.repeat !== "off",
        label: REPEAT_LABEL[s.repeat],
        title: `${REPEAT_LABEL[s.repeat]} (R)`,
        onClick: () => void api.action("cycleRepeat"),
        icon: <IconRepeat one={s.repeat === "one"} mode={s.repeat} />,
      });
    // 流れる歌詞の列のオン・オフ（v3.0.2）。「歌詞」ボタンのすぐ左。オン・オフで位置は変わらない
    if (b === "lyricsRow")
      return item({
        on: rowOn,
        label: inMenu ? `流れる歌詞の列：${rowOn ? "入" : "切"}` : "流れる歌詞の列",
        title: `流れる歌詞の列：${rowOn ? "入" : "切"} (${rowKey})`,
        onClick: () => void toggleLyricsRow(),
        icon: <IconLyricsRow />,
      });
    return item({
      on: lyricsOpen,
      label: "歌詞",
      title: `歌詞 (${lyricsKey})`,
      onClick: toggleLyricsView,
      icon: <IconLyrics />,
      cls: "lyrics-btn",
    });
  };
  // 3列の操作の5つの枠の両端（シャッフル・リピート）。出さないとき・「···」へ移したときも空の枠を残し、再生ボタンを中央に保つ
  const centerSlot = (b: "shuffle" | "repeat", on: boolean) => (on ? movable(b, false) : <span aria-hidden="true" />);

  return (
    <footer ref={ref} className={"player panel " + (one ? "one-row" : "three-col")} aria-label="再生バー">
      {one ? <SeekBar disabled={!t} variant="line" /> : null}
      <div className="player-left">
        <button
          className="art-btn"
          style={{ border: "none", background: "none", padding: 0, cursor: t ? "pointer" : "default" }}
          onClick={() => t && navigate({ view: "album", key: t.albumKey, title: t.album || "不明なアルバム" })}
          title={t ? "このアルバムを開く" : undefined}
        >
          <Artwork path={t?.artPath} className="player-art" />
        </button>
        <div className="player-mid" onContextMenu={(e) => t && openMenu(e, trackMenu({ tracks: [t], list: "playerBar" }))}>
          <div className="player-title" title={t?.title}>
            {t ? t.title : "曲を選んでください"}
          </div>
          {/* 1段のときは、アーティスト名と歌詞の1行の代わりに「今の時間 / 長さ」 */}
          {one ? (
            t ? <SeekTime /> : null
          ) : (
            <div className="player-artist">{t ? t.artist || "不明なアーティスト" : "ファイルやフォルダをドラッグ＆ドロップで取り込めます"}</div>
          )}
          {!one && t && !rowOn ? <PlayerLyricLine trackId={t.id} /> : null}
        </div>
      </div>
      <div className="player-center">
        <div className="player-controls">
          {one ? null : centerSlot("shuffle", L.centerShuffle)}
          <button className="icon-btn" onClick={() => void api.action("previous")} title="前の曲 (Ctrl+←)" aria-label="前の曲">
            <IconPrev />
          </button>
          <button
            className="icon-btn play-btn"
            onClick={() => void api.action("toggle")}
            title={playing ? "一時停止 (Space)" : "再生 (Space)"}
            aria-label={playing ? "一時停止" : "再生"}
          >
            {playing ? <IconPause /> : <IconPlay />}
          </button>
          <button className="icon-btn" onClick={() => void api.action("next")} title="次の曲 (Ctrl+→)" aria-label="次の曲">
            <IconNext />
          </button>
          {one ? null : centerSlot("repeat", L.centerRepeat)}
        </div>
        {one ? null : <SeekBar disabled={!t} />}
      </div>
      <div className="player-right">
        {/* 出力ボタン（固定幅）：全体の音量の左。幅は playerLayout の outputW にも渡している */}
        {L.output > 0 ? <OutputButton one={one} /> : null}
        {L.right.map((b) => (
          <Fragment key={b}>{movable(b, false)}</Fragment>
        ))}
        {L.more.length > 0 ? <PlayerMore items={L.more} render={(b) => movable(b, true)} /> : null}
      </div>
    </footer>
  );
}
