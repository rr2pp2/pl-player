import { useState } from "react";
import { api } from "../ipc/commands";
import { position } from "../stores/playback";
import { useStore } from "../stores/store";
import { formatTime } from "./format";

/**
 * シークバー。現在時間と総時間を出す（原文「現在時間と総時間を表示する」）。
 * v3.8.0：line は再生バーが1段のときの形。再生バーの上の縁に横いっぱいの細い線だけを出す（時間の文字は SeekTime で曲名の下に）
 */
export function SeekBar(props: { disabled?: boolean; variant?: "full" | "line" }) {
  const p = useStore(position);
  const [drag, setDrag] = useState<number | null>(null);
  const dur = Math.max(0, p.durationMs);
  const cur = drag ?? Math.min(p.positionMs, dur || p.positionMs);
  const pct = dur > 0 ? (cur / dur) * 100 : 0;
  const commit = () => {
    if (drag !== null) {
      void api.seek(drag);
      position.set({ positionMs: drag, durationMs: dur });
    }
    setDrag(null);
  };
  const input = (
    <input
      type="range"
      className={"range" + (props.variant === "line" ? " seek-line" : "")}
      aria-label="再生位置"
      aria-valuetext={`${formatTime(cur)} / ${formatTime(dur)}`}
      min={0}
      max={dur || 1}
      step={250}
      value={cur}
      disabled={props.disabled || dur <= 0}
      style={{ ["--pct" as any]: `${pct}%` }}
      onChange={(e) => setDrag(Number(e.target.value))}
      onPointerUp={commit}
      onKeyUp={commit}
      onBlur={() => drag !== null && commit()}
    />
  );
  if (props.variant === "line") return input;
  return (
    <div className="seek">
      <span aria-label="現在時間">{formatTime(cur)}</span>
      {input}
      <span aria-label="総時間">{formatTime(dur)}</span>
    </div>
  );
}

/** v3.8.0：再生バーが1段のとき、曲名の下に出す「今の時間 / 長さ」 */
export function SeekTime() {
  const p = useStore(position);
  const dur = Math.max(0, p.durationMs);
  const cur = Math.min(p.positionMs, dur || p.positionMs);
  return (
    <div className="player-time">
      {formatTime(cur)} / {formatTime(dur)}
    </div>
  );
}
