// v3.8.0 段2-2：メーターの値の決まり（仕様 2.7）。ほかに依存しない純粋な関数だけを置く（scripts\ui-detail.test.mts から確かめる）。
//
// - Rust は約 33ms ごとに、枠3つの左右の最大（音量を掛けたあと・切り詰める前。1.0 がフルスケール＝0 dBFS）を送る。
//   dB への換算・直前の最大の線（20 秒）・音割れの印（2 秒）はここ（画面側）で決める（13.5 の13）
// - 色の帯は OBS と同じ：緑 -20 dB まで、黄 -20〜-9 dB、赤 -9 dB より上。目盛りは -60〜0 dB
// - 帯は OBS と同じく、上がるときはすぐ、下がるときはゆっくり（METER.decayDbPerSec）。直前の最大の線と音割れの印は、帯ではなく実際の値で決める
// - dB の数字は左右の大きいほうの今の値。毎回書き換えると読めないので、0.1 秒ごとに、その間の最大を出す
// - 鳴っていない出力（オフ・切断中・開けません・重複）は止める（値を 0 にし、線と印も消す）
// - 「動きを減らす」設定でも動かし続ける（音の状況を知るためのもの）。描くのは src\components\Meter.tsx（React の再描画を起こさない）

export const METER = {
  minDb: -60,
  /** 緑の帯の上の端・黄の帯の上の端（これより上は赤） */
  greenTo: -20,
  yellowTo: -9,
  holdMs: 20_000,
  clipMs: 2_000,
  numberMs: 100,
  /**
   * 帯が下がる速さ（dB/秒）。OBS の既定（「速い」）と同じ 40 dB / 1.7 秒 ≒ 23.5 dB/秒（v3.8.0 段2-2・作者の決定「OBS のように」）。
   * OBS の UI\volume-control.cpp：表示の値を「前の値 − 速さ × 前に描いてからの時間」にし、今の値より下へは行かない（上がるときはすぐ）。
   * 直前の最大の線（20 秒）も OBS の既定の peakHoldDuration と同じ
   */
  decayDbPerSec: 40 / 1.7,
  /** 目盛りの数字（dB） */
  ticks: [-60, -50, -40, -30, -20, -10, 0],
} as const;

/** 1.0 を 0 dBFS とした dB。0 以下・数でない値は -Infinity */
export function toDb(lin: number): number {
  if (!(lin > 0) || !Number.isFinite(lin)) return -Infinity;
  return 20 * Math.log10(lin);
}

/** メーターの左端（-60 dB）から右端（0 dB）までの位置（0〜1）。-60 より下は 0、0 より上は 1 */
export function dbFraction(db: number): number {
  if (!(db > METER.minDb)) return 0;
  return Math.min(1, (db - METER.minDb) / -METER.minDb);
}

/** 色の帯の切り替わる位置（0〜1）。緑→黄 -20 dB（0.667）、黄→赤 -9 dB（0.85） */
export const BAND_AT = { yellow: dbFraction(METER.greenTo), red: dbFraction(METER.yellowTo) } as const;

/** 音割れ：0 dB に届いた（浮動小数の誤差は数えない） */
export const isClip = (lin: number): boolean => Number.isFinite(lin) && lin >= 1 - 1e-6;

/** dB の数字の文（小数1桁。鳴っていないときは「-∞」、0 dB を超えたら「+」を付ける） */
export function dbText(db: number): string {
  if (!(db > -100)) return "-∞";
  const r = Math.round(db * 10) / 10;
  if (r > 0) return `+${r.toFixed(1)}`;
  return (Object.is(r, -0) ? 0 : r).toFixed(1);
}

export interface MeterState {
  /** 直前の最大（dB）と、それを決めた時刻 */
  hold: [number, number];
  holdAt: [number, number];
  clipUntil: number;
  /** dB の数字：今出している値と、次に出すまでの最大・前に書いた時刻 */
  number: number;
  numberMax: number;
  numberAt: number;
  /** 帯に出している値（dB。下がるときはゆっくり）と、それを出した時刻（まだ出していなければ -1） */
  shown: [number, number];
  shownAt: number;
}

export function newMeterState(): MeterState {
  return {
    hold: [-Infinity, -Infinity],
    holdAt: [0, 0],
    clipUntil: 0,
    number: -Infinity,
    numberMax: -Infinity,
    numberAt: -Infinity,
    shown: [-Infinity, -Infinity],
    shownAt: -1,
  };
}

/** 描くもの（どれも 0〜1 の位置。線が無いときの hold は -1） */
export interface MeterView {
  active: boolean;
  level: [number, number];
  hold: [number, number];
  clip: boolean;
  text: string;
}

export const IDLE_VIEW: MeterView = { active: false, level: [0, 0], hold: [-1, -1], clip: false, text: "-∞" };

/**
 * 1回分の値を受けて、描くものを返す（st は書き換える）。active が false なら止めて、線・印・数字も消す。
 * now はミリ秒（performance.now など。戻らない時計）
 */
export function meterStep(st: MeterState, peaks: readonly [number, number] | undefined, active: boolean, now: number): MeterView {
  if (!active || !peaks) {
    Object.assign(st, newMeterState());
    return IDLE_VIEW;
  }
  const lin: [number, number] = [peaks[0] ?? 0, peaks[1] ?? 0];
  const db: [number, number] = [toDb(lin[0]), toDb(lin[1])];
  for (const c of [0, 1] as const) {
    if (db[c] >= st.hold[c] || now - st.holdAt[c] > METER.holdMs) {
      st.hold[c] = db[c];
      st.holdAt[c] = now;
    }
  }
  if (isClip(lin[0]) || isClip(lin[1])) st.clipUntil = now + METER.clipMs;
  // 帯の長さ（OBS と同じ動き）：上がるときはすぐ今の値へ。下がるときは前に出した値から 1 秒あたり decayDbPerSec ずつ下げ、今の値より下へは行かない。
  // 前の回から時間が空いた（止めていた・最小化から戻った）ときも、空いた時間ぶん下がるだけ（跳ねない）
  const dt = st.shownAt < 0 ? 0 : Math.max(0, now - st.shownAt) / 1000;
  for (const c of [0, 1] as const) {
    st.shown[c] = Math.max(db[c], st.shown[c] - METER.decayDbPerSec * dt);
  }
  st.shownAt = now;
  const loud = Math.max(db[0], db[1]);
  st.numberMax = Math.max(st.numberMax, loud);
  if (now - st.numberAt >= METER.numberMs) {
    st.number = st.numberMax;
    st.numberMax = -Infinity;
    st.numberAt = now;
  }
  const holdPos = (c: 0 | 1) => (st.hold[c] > METER.minDb ? dbFraction(st.hold[c]) : -1);
  return {
    active: true,
    level: [dbFraction(st.shown[0]), dbFraction(st.shown[1])],
    hold: [holdPos(0), holdPos(1)],
    clip: now < st.clipUntil,
    text: dbText(st.number),
  };
}
