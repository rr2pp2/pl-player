// 歌詞のカード（時刻つきの行・時刻なしの全文）。行はボタン相当でフォーカスでき、押すとその時刻へシークする。
import type { LyricsLine, LyricsPayload } from "../ipc/types";
import { lyricsWarning } from "../messages/ja";
import { seekToLine, shownTime, type ActiveRange } from "../stores/lyrics";
import { formatTime } from "./format";

function LineRow(props: { line: LyricsLine; index: number; delayMs: number; active: boolean }) {
  const { line, index, active } = props;
  return (
    <button
      type="button"
      className={"lyric-line" + (active ? " active" : "")}
      data-line={index}
      aria-current={active ? "true" : undefined}
      onClick={() => seekToLine(line)}
      title="この行へ移動"
    >
      <span className="lyric-time">{formatTime(shownTime(line, props.delayMs))}</span>
      <span className="lyric-text">{line.text}</span>
    </button>
  );
}

/** 歌詞のカードの中身。スクロールする箱（追従のため）は呼ぶ側が持つ */
export function LyricsLines(props: { payload: LyricsPayload; active: ActiveRange | null }) {
  const p = props.payload;
  const partial = p.warnings.includes("timestamp_invalid");
  if (p.format !== "timed") {
    return (
      <>
        {partial ? <div className="lyrics-warn">{lyricsWarning("timestamp_invalid")}</div> : null}
        <div className="lyrics-plain">{p.plainText}</div>
      </>
    );
  }
  const a = props.active;
  const untimed = p.lines.map((l, i) => [l, i] as const).filter(([l]) => l.kind === "untimed");
  return (
    <>
      {partial ? <div className="lyrics-warn">{lyricsWarning("timestamp_invalid")}</div> : null}
      <div className="lyric-list" role="list" aria-label="歌詞">
        {p.lines.map((l, i) =>
          l.kind === "lyric" ? (
            <div role="listitem" key={l.key}>
              <LineRow line={l} index={i} delayMs={p.userDelayMs} active={!!a && i >= a.start && i <= a.end} />
            </div>
          ) : null,
        )}
      </div>
      {untimed.length ? (
        <div className="lyric-untimed">
          <div className="lyric-untimed-head">時刻なしの行</div>
          {untimed.map(([l]) => (
            <div className="lyric-untimed-line" key={l.key}>
              {l.text}
            </div>
          ))}
        </div>
      ) : null}
    </>
  );
}
