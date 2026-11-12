// 曲ごとの時刻補正（−100ms・リセット・+100ms）。＋は歌詞を遅らせる。
import { changeDelay } from "../stores/lyrics";

const STEP = 100;

export const delayLabel = (ms: number) => `${ms > 0 ? "+" : ms < 0 ? "−" : "±"}${Math.abs(ms)}ms`;

export function LyricsDelay(props: { value: number; compact?: boolean }) {
  const v = props.value;
  return (
    <div className={"lyrics-delay" + (props.compact ? " compact" : "")} role="group" aria-label="この曲の歌詞の時刻補正">
      {props.compact ? null : <div className="lyrics-delay-label">時刻の補正</div>}
      <div className="row lyrics-delay-row">
        <button className="btn small-btn" onClick={() => void changeDelay(v - STEP)} aria-label="100ms 早める">
          −100ms
        </button>
        <span className="lyrics-delay-value" aria-live="polite">
          {delayLabel(v)}
        </span>
        <button className="btn small-btn" onClick={() => void changeDelay(v + STEP)} aria-label="100ms 遅らせる">
          +100ms
        </button>
        {props.compact ? null : (
          <button className="btn small-btn" onClick={() => void changeDelay(0)} disabled={v === 0}>
            リセット
          </button>
        )}
      </div>
      {props.compact ? null : <div className="hint-text">＋は歌詞を遅らせます</div>}
    </div>
  );
}
