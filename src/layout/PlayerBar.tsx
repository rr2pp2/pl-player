// 下：再生バー（画面下部に固定）。左：アート／中央：曲名・アーティスト・今の歌詞の1行・進み具合／右：操作と音量・歌詞。
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
import { SeekBar } from "../components/SeekBar";
import { VolumeControl } from "../components/VolumeControl";
import { IconLyrics, IconLyricsRow, IconNext, IconPause, IconPlay, IconPrev, IconRepeat, IconShuffle } from "../components/Icons";
import { LyricParts } from "../components/LyricParts";
import { openMenu } from "../components/ContextMenu";
import { trackMenu } from "../components/trackActions";

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
  return (
    <footer className="player panel" aria-label="再生バー">
      <button
        className="art-btn"
        style={{ border: "none", background: "none", padding: 0, cursor: t ? "pointer" : "default" }}
        onClick={() => t && navigate({ view: "album", key: t.albumKey, title: t.album || "不明なアルバム" })}
        title={t ? "このアルバムを開く" : undefined}
      >
        <Artwork path={t?.artPath} className="player-art" />
      </button>
      <div className="player-mid" onContextMenu={(e) => t && openMenu(e, trackMenu({ tracks: [t] }))}>
        <div className="player-title" title={t?.title}>
          {t ? t.title : "曲を選んでください"}
        </div>
        <div className="player-artist">{t ? t.artist || "不明なアーティスト" : "ファイルやフォルダをドラッグ＆ドロップで取り込めます"}</div>
        {t && !rowOn ? <PlayerLyricLine trackId={t.id} /> : null}
        <SeekBar disabled={!t} />
      </div>
      <div className="player-controls">
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
        <button
          className={"icon-btn" + (s.shuffle ? " on" : "")}
          onClick={() => void api.setShuffle(!s.shuffle)}
          title={s.shuffle ? "シャッフル：入 (S)" : "シャッフル：切 (S)"}
          aria-pressed={s.shuffle}
          aria-label="シャッフル"
        >
          <IconShuffle on={s.shuffle} />
        </button>
        <button
          className={"icon-btn" + (s.repeat !== "off" ? " on" : "")}
          onClick={() => void api.action("cycleRepeat")}
          title={`${REPEAT_LABEL[s.repeat]} (R)`}
          aria-label={REPEAT_LABEL[s.repeat]}
        >
          <IconRepeat one={s.repeat === "one"} mode={s.repeat} />
        </button>
        <VolumeControl />
        {/* 流れる歌詞の列のオン・オフ（v3.0.2）。「歌詞」ボタンのすぐ左。オン・オフで位置は変わらない */}
        <button
          className={"icon-btn" + (rowOn ? " on" : "")}
          onClick={() => void toggleLyricsRow()}
          title={`流れる歌詞の列：${rowOn ? "入" : "切"} (${rowKey})`}
          aria-label="流れる歌詞の列"
          aria-pressed={rowOn}
        >
          <IconLyricsRow />
        </button>
        <button
          className={"icon-btn lyrics-btn" + (lyricsOpen ? " on" : "")}
          onClick={toggleLyricsView}
          title={`歌詞 (${lyricsKey})`}
          aria-label="歌詞"
          aria-pressed={lyricsOpen}
        >
          <IconLyrics />
        </button>
      </div>
    </footer>
  );
}
