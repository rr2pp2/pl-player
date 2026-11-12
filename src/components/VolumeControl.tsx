import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import { playback } from "../stores/playback";
import { useStore } from "../stores/store";
import { IconVolume } from "./Icons";

export function VolumeControl() {
  const s = useStore(playback);
  const [local, setLocal] = useState<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const v = local ?? s.volume;
  const shown = s.muted ? 0 : v;
  useEffect(() => () => window.clearTimeout(timer.current), []);
  // マウスホイールで音量（ミュートのボタンと帯の上にカーソルがあるとき）。1目盛り 5%
  const onWheel = (e: { deltaY: number }) => {
    if (e.deltaY === 0) return;
    const base = local ?? s.volume;
    const nv = Math.round(Math.min(1, Math.max(0, base + (e.deltaY < 0 ? 0.05 : -0.05))) * 100) / 100;
    setLocal(nv);
    void api.setVolume(nv);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      setLocal(null);
      void api.commitVolume();
    }, 400);
  };
  return (
    <div className="volume" onWheel={onWheel} title={`音量 ${Math.round(shown * 100)}%（ホイールで調整）`}>
      <button
        className={"icon-btn" + (s.muted ? " on" : "")}
        title={s.muted ? "ミュートを解除 (M)" : "ミュート (M)"}
        aria-label="ミュート"
        onClick={() => void api.setMuted(!s.muted)}
      >
        <IconVolume muted={s.muted} level={v} />
      </button>
      <input
        type="range"
        className="range"
        aria-label="音量"
        min={0}
        max={1}
        step={0.01}
        value={shown}
        style={{ ["--pct" as any]: `${shown * 100}%` }}
        title={`音量 ${Math.round(shown * 100)}%`}
        onChange={(e) => {
          const nv = Number(e.target.value);
          setLocal(nv);
          void api.setVolume(nv);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => {
            setLocal(null);
            void api.commitVolume();
          }, 400);
        }}
      />
    </div>
  );
}
