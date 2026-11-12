import { useState } from "react";
import { api } from "../ipc/commands";
import { position } from "../stores/playback";
import { useStore } from "../stores/store";
import { formatTime } from "./format";

/** シークバー。現在時間と総時間を出す（原文「現在時間と総時間を表示する」） */
export function SeekBar(props: { disabled?: boolean }) {
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
  return (
    <div className="seek">
      <span aria-label="現在時間">{formatTime(cur)}</span>
      <input
        type="range"
        className="range"
        aria-label="再生位置"
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
      <span aria-label="総時間">{formatTime(dur)}</span>
    </div>
  );
}
