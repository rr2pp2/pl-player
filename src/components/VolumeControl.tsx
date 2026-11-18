import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import { playback } from "../stores/playback";
import { useStore } from "../stores/store";
import { settings } from "../stores/settings";
import { comboLabel } from "../shortcuts/shortcuts";
import { IconVolume } from "./Icons";

export function VolumeControl() {
  const s = useStore(playback);
  // v3.8.0（仕様 2.4）：ミュートのキーは設定で決まるもの（3.7.0 までは「M」と決め打ちしていた）
  const muteKey = comboLabel(useStore(settings).shortcuts.mute ?? "");
  const [local, setLocal] = useState<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const v = local ?? s.volume;
  const shown = s.muted ? 0 : v;
  // v3.8.0 段2-2：保存を待っている間に消えた（「···」を閉じた・再生バーの形が変わって音量が「···」へ移った）ときは、消えるときに保存を送る
  useEffect(
    () => () => {
      if (timer.current === undefined) return;
      window.clearTimeout(timer.current);
      timer.current = undefined;
      void api.commitVolume();
    },
    [],
  );
  // マウスホイールで音量（ミュートのボタンと帯の上にカーソルがあるとき）。1目盛り 5%
  const onWheel = (e: { deltaY: number }) => {
    if (e.deltaY === 0) return;
    const base = local ?? s.volume;
    const nv = Math.round(Math.min(1, Math.max(0, base + (e.deltaY < 0 ? 0.05 : -0.05))) * 100) / 100;
    setLocal(nv);
    void api.setVolume(nv);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = undefined;
      setLocal(null);
      void api.commitVolume();
    }, 400);
  };
  return (
    <div className="volume" onWheel={onWheel} title={`音量 ${Math.round(shown * 100)}%（ホイールで調整）`}>
      <button
        className={"icon-btn" + (s.muted ? " on" : "")}
        title={s.muted ? `ミュートを解除 (${muteKey})` : `ミュート (${muteKey})`}
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
            timer.current = undefined;
            setLocal(null);
            void api.commitVolume();
          }, 400);
        }}
      />
    </div>
  );
}
