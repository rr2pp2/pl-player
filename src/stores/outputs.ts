// v3.8.0 段2-2：出力先（同時再生）と表示のモード（通常／詳細）の写し、メーターの受け口。正しい状態は Rust 側にある。
//
// - 出力先の設定を書くのは Rust だけ（画面は api.output* の細かい操作を送るだけ）。変わるたびに pl://outputs で全部が来る
//   （グローバルホットキーで詳細を切り替えたときも、これで届く）
// - メーター：見ている部品（ミキサー・出力のプルダウン）が1つでもある間だけ meterStart し、全部いなくなったら meterStop。
//   窓が見えない（最小化など。document が hidden）間も止める（Rust 側でも最小化を確かめて送らない）。
//   値（dB・直前の最大の線・音割れの印）は枠ごとに1つだけ持ち、どの部品にも同じものを渡す（同じ出力のメーターは同じ値。4.4）
import { api } from "../ipc/commands";
import { on } from "../ipc/events";
import type { DeviceInfo, MeterFrame, OutputsView } from "../ipc/types";
import { IDLE_VIEW, meterStep, newMeterState, type MeterView } from "../layout/meter";
import { slotSounding } from "../layout/outputs";
import { createStore } from "./store";

export const outputs = createStore<OutputsView>({
  mode: "normal",
  master: 0.8,
  masters: { normal: 0.8, detail: 0.8 },
  muted: false,
  slots: [null, null, null],
});

/** 詳細（仕様の「配信モード」）か */
export const isDetail = (v: OutputsView): boolean => v.mode === "detail";

/**
 * 起動時：先に知らせを受け始めてから、今の全部を引く（引いている間に来た変化を取りこぼさない。段2-2 検査1回目の指摘10）。
 * 引いている間に知らせが来ていたら、そちらのほうが新しいので、引いた結果は使わない
 */
export async function initOutputs() {
  let pushed = false;
  await on("outputs", (v) => {
    pushed = true;
    outputs.set(v);
  });
  try {
    const v = await api.outputsGet();
    if (!pushed) outputs.set(v);
  } catch {
    /* 起動直後は無視（知らせが来れば入る） */
  }
}

// ---------- 再生デバイスの一覧 ----------

/** 再生デバイスの一覧（足す・差し替える・設定画面）。見ている部品がある間だけ、デバイスの変化の知らせで引き直す */
export const devices = createStore<DeviceInfo[]>([]);
let deviceUsers = 0;
let deviceUnlisten: Promise<() => void> | null = null;
let devicesTimer: number | undefined;

export function loadDevices(): Promise<void> {
  return api.devices().then(
    (d) => devices.set(d),
    () => {},
  );
}

/**
 * 一覧を見始める（返した関数で止める）。最初の1つで一覧を引き、デバイスの変化の知らせを受け始める。
 * 最後の1つが止めたら知らせの受け口を外す（画面が出ていない間は引き直さない。段2-2 検査1回目の指摘4。3.7.0 の設定画面と同じ）
 */
export function watchDevices(): () => void {
  deviceUsers++;
  void loadDevices();
  if (!deviceUnlisten) {
    deviceUnlisten = on("devices", () => {
      window.clearTimeout(devicesTimer);
      devicesTimer = window.setTimeout(() => void loadDevices(), 300);
    });
  }
  let done = false;
  return () => {
    if (done) return;
    done = true;
    deviceUsers--;
    if (deviceUsers > 0 || !deviceUnlisten) return;
    const p = deviceUnlisten;
    deviceUnlisten = null;
    window.clearTimeout(devicesTimer);
    void p.then((un) => un());
  };
}

// ---------- メーター ----------

type MeterListener = (views: readonly MeterView[]) => void;
const listeners = new Set<MeterListener>();
const states = [newMeterState(), newMeterState(), newMeterState()];
let running = false;
const IDLE: readonly MeterView[] = [IDLE_VIEW, IDLE_VIEW, IDLE_VIEW];

/**
 * 開始・停止の通し番号（段2-2 検査1回目の指摘3）。meter_start・meter_stop は非同期のコマンドで、実行の順番が入れ替わりうる。
 * Rust はいちばん大きい番号だけを効かせる（MeterControl）。画面を読み込み直しても前より大きくなるよう、今の時刻（マイクロ秒ほど）から始める
 */
let seq = 0;
const nextSeq = () => (seq = Math.max(seq + 1, Date.now() * 1000));
/** 今の開始の番号（止めた・まだ始めていないなら 0） */
let startSeq = 0;
let retryTimer: number | undefined;
/** 開始に失敗したとき、まだ見ている部品があれば試し直すまでの時間 */
const METER_RETRY_MS = 1000;

function onFrame(f: MeterFrame) {
  if (!running) return;
  const now = performance.now();
  const slots = outputs.get().slots;
  const views = [0, 1, 2].map((i) => meterStep(states[i], f.peaks[i], slotSounding(slots[i]), now));
  listeners.forEach((l) => l(views));
}

function sync() {
  const want = listeners.size > 0 && document.visibilityState === "visible";
  if (want && !running) {
    running = true;
    window.clearTimeout(retryTimer);
    const my = nextSeq();
    startSeq = my;
    api.meterStart(my, onFrame).catch(() => {
      // あとから別の開始・停止をしていれば、この失敗は古いので何もしない
      if (startSeq !== my) return;
      running = false;
      startSeq = 0;
      retryTimer = window.setTimeout(sync, METER_RETRY_MS);
    });
  } else if (!want && running) {
    running = false;
    startSeq = 0;
    void api.meterStop(nextSeq()).catch(() => {});
    for (let i = 0; i < states.length; i++) states[i] = newMeterState();
    listeners.forEach((l) => l(IDLE));
  } else if (!want) {
    window.clearTimeout(retryTimer);
  }
}

if (typeof document !== "undefined") document.addEventListener("visibilitychange", sync);

/** メーターを見始める（返した関数で止める）。最初の1つで meterStart、最後の1つが止めたら meterStop */
export function subscribeMeter(l: MeterListener): () => void {
  listeners.add(l);
  l(IDLE);
  sync();
  return () => {
    listeners.delete(l);
    sync();
  };
}
