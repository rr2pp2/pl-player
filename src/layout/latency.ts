// v3.8.0 段3：遅れを測る（仕様「同時再生と配信モード」2.3 の段3）の、見せ方の決まり。ほかに依存しない純粋な関数だけを置く
// （scripts\ui-latency.test.mts から確かめる）。
//
// - 入口のボタンは2か所：出力のプルダウンと「詳細」のミキサー。押すと小窓（LatencyDialog.tsx）が開く
// - 測るのは曲を止めているときだけ。流している間は押せず、理由を吹き出し（と、プルダウンでは文字）で出す
// - 合わせ方は「いちばん遅く届く出力に揃える」：入れる値＝「測れた出力の届くまでの遅れの最大 − その出力の遅れ」（Rust が計算する）
// - 測れなかった出力は「測れませんでした」と理由つきで出し、値は入れない（その出力だけ）。色だけで伝えない（必ず文字）
import type { CaptureSource, LatencyAbort, LatencyFail, LatencyReport, LatencyRow, LatencySetup, OutputsView, OutputStatus } from "../ipc/types";

export const LATENCY_TEXT = {
  button: "遅れを測る…",
  title: "遅れを測る",
  /** 測る前の断り（作者の決定の文） */
  caution: "クリック音が配信・録画に乗るので、止めてから測ってください。",
  quiet: "ほかのアプリの音も止めておくと測りやすくなります。",
  how: "オンの出力先それぞれに短いクリック音を鳴らして録り、届くまでの遅れを測ります（通常は約 6 秒。デバイスを開くのに時間がかかると、長くて 15 秒ほど）。いちばん遅く届く出力先に揃える値を出します（早く届く出力先を遅らせます）。今の遅らせ量は、測るときには使いません。",
  /** 測れる範囲（仕様 2.3 の段3） */
  scope: "測れるのは Windows がデバイスへ渡すところまでです。機器の中の遅れ（USB の先・HDMI の先・Bluetooth）は入りません。",
  /** 仮想ケーブルの遅れは変わる（仕様 13.3） */
  // 2026-11-19 の実機の確かめ：OBS の録画の中のずれ 4.8ms に対し、アプリの値は 10ms（出口を別に開き直して測るため）
  cable:
    "仮想ケーブルの遅れは、OBS を起動し直すと変わることがあります。そのときは測り直してください。OBS を開いたままで測るのが本番に近くなりますが、それでも OBS が受け取る遅れとは数ミリ秒ずれることがあります（だいたいの値です）。",
  measuring: "測っています…（通常は約 6 秒。クリック音が鳴ります。「やめる」でいつでもやめられます）",
  apply: "この値を入れる",
  retry: "測り直す",
  start: "測る",
} as const;

/** 曲を流している間の、押せない理由 */
export const LATENCY_PLAYING = "曲を流している間は測れません（一時停止か停止をしてから押してください）";
/** 鳴らせるオンの出力先が2つ未満のときの、押せない理由 */
export const LATENCY_NOT_ENOUGH = "鳴らせるオンの出力先が2つ以上あるときに測れます（揃える相手が要るため）";

/** 入口のボタン・「測る」を押せるか。押せないときは理由（押せるなら null） */
export function latencyBlocked(v: Pick<OutputsView, "slots">, playing: boolean): string | null {
  if (playing) return LATENCY_PLAYING;
  // 鳴らせるオンの出力先（outputs.ts の registered と同じ拾い方。純粋なファイルどうしは値を読み込まない：Node の検査が .ts を直接読むため）
  const sounding = v.slots.filter((s) => s !== null && s !== undefined && s.on && s.status === "ok").length;
  if (sounding < 2) return LATENCY_NOT_ENOUGH;
  return null;
}

/** 測れなかった理由の文 */
export const FAIL_TEXT: Record<LatencyFail, string> = {
  output_missing: "デバイスが見つかりません（抜けた可能性があります）",
  output_busy: "ほかのアプリがこのデバイスを独り占めしています",
  output_format: "このデバイスの形式は使えません",
  output_open: "このデバイスを開けませんでした",
  capture_missing: "録る所のデバイスが見つかりません（選び直してください）",
  capture_busy: "ほかのアプリが録る所のデバイスを独り占めしています",
  capture_format: "録る所のデバイスの形式は使えません",
  // 仮想ケーブルの出口も録音デバイスとして開くので、Windows の「マイクへのアクセス」を切っていると開けない（検査1回目の指摘4。
  // cpal がどの種類で返すかは実機で確かめていないので、その他の失敗の文にも添える）
  capture_open: "録る所のデバイスを開けませんでした（Windows の設定の「マイクへのアクセス」がオフになっていないかも確かめてください）",
  capture_denied: "Windows が録る所のデバイスを開くのを断りました（Windows の設定の「マイクへのアクセス」と「デスクトップ アプリがマイクにアクセスできるようにする」をオンにしてください）",
  open_timeout: "デバイスを開くのに時間がかかりすぎました",
  clock: "デバイスの時刻がアプリの時計とそろわないので、比べられません",
  no_timing: "デバイスから鳴る時刻が届きませんでした",
  capture_silent: "録る所から音が届きませんでした（録る所が違う可能性があります）",
  stream_error: "測っている途中でデバイスが止まりました（抜けた・設定が変わった）",
  late: "クリック音を鳴らすのが間に合いませんでした（PC が重い可能性があります）",
  gap: "録音が途切れました（PC が重い可能性があります）",
  no_signal: "クリック音が届きませんでした（録る所が違う・音量が 0 かミュート・遅れが 0.4 秒より大きい）",
  weak: "クリック音がほかの音に埋もれました（ほかのアプリの音を止めてください）",
  noisy: "ほかの音とクリック音を見分けられませんでした（ほかのアプリの音を止めてください）",
  unstable: "回ごとの結果がそろいませんでした（もう一度測ってください）",
};

/** 全体をやめた・始めなかった理由の文 */
export const ABORT_TEXT: Record<LatencyAbort, string> = {
  playing: "曲を流し始めたので、測るのをやめました（クリック音と曲が混ざるため）。止めてから測り直してください。",
  cancelled: "測るのをやめました。",
  busy: "前の測定がまだ終わっていません。少し待ってから、もう一度押してください。",
  not_enough: LATENCY_NOT_ENOUGH + "。",
  failed: "測る途中で思わぬエラーが起きました。もう一度お試しください。",
};

/** 測らない出力の文（オフ・切断中など） */
export function skipText(status: OutputStatus): string {
  if (status === "off") return "オフなので測りません";
  if (status === "disconnected") return "切断中なので測れません";
  if (status === "unavailable") return "開けないので測れません";
  if (status === "duplicate") return "ほかの出力先と同じデバイスなので測りません";
  return "測りません";
}

/** ms の数（小数1桁。-0.0 は 0.0） */
export function msText(v: number): string {
  const r = Math.round(v * 10) / 10;
  return `${(Object.is(r, -0) ? 0 : r).toFixed(1)} ミリ秒`;
}

/** 結果の1行に出すもの */
export interface LatencyRowModel {
  slot: number;
  name: string;
  /** 「届くまでの遅れ」の文（測れたとき） */
  arrival: string | null;
  /** 「入れる値」の文（値があるとき） */
  fill: string | null;
  /** 理由・断りの文（測れなかった・測らない・揃える相手がいない） */
  note: string | null;
  /** 測れなかった（危険の色の文字。色だけで伝えないよう、文は「測れませんでした」から始める） */
  failed: boolean;
}

export function latencyRowModel(r: LatencyRow): LatencyRowModel {
  const base = { slot: r.slot, name: r.name };
  if (!r.measured) return { ...base, arrival: null, fill: null, note: skipText(r.status), failed: false };
  if (r.fail || r.arrivalMs === null)
    return { ...base, arrival: null, fill: null, note: `測れませんでした：${r.fail ? FAIL_TEXT[r.fail] : FAIL_TEXT.no_signal}`, failed: true };
  const spread = r.spreadMs !== null && r.spreadMs >= 0.05 ? `（ばらつき ${msText(r.spreadMs)}）` : "";
  const arrival = `${msText(r.arrivalMs)}${spread}`;
  if (r.fillMs === null) return { ...base, arrival, fill: null, note: "揃える相手が測れなかったので、入れる値は出しません", failed: false };
  const now = r.fillMs === r.delayMs ? "（今と同じ）" : `（今は ${r.delayMs} ミリ秒）`;
  return { ...base, arrival, fill: `${r.fillMs} ミリ秒${now}`, note: null, failed: false };
}

/** 「この値を入れる」で送るもの（値がある出力だけ） */
export function applyItems(rep: LatencyReport): { slot: number; deviceId: string | null; delayMs: number }[] {
  if (rep.abort) return [];
  return rep.rows.filter((r) => r.measured && r.fillMs !== null).map((r) => ({ slot: r.slot, deviceId: r.deviceId, delayMs: r.fillMs as number }));
}

/** 結果の下に添える文 */
export function reportNotes(rep: LatencyReport): string[] {
  if (rep.abort) return [];
  const out: string[] = [];
  const measured = rep.rows.filter((r) => r.measured);
  const failed = measured.filter((r) => r.fail || r.arrivalMs === null);
  const valued = measured.filter((r) => r.fillMs !== null);
  if (failed.length > 0 && valued.length > 0) out.push("測れなかった出力先は揃える相手に入っていません。その出力先の遅らせ量は変えません。");
  if (valued.length === 0 && measured.length > 0) out.push("測れた出力先が2つに足りないので、入れる値はありません（揃える相手が要るため）。");
  return out;
}

// ---------- 録る所 ----------

export const LOOPBACK_LABEL = "この出力の音（ループバック）";

/** select の値（"loopback" か "input:<id>"） */
export function captureKey(c: CaptureSource): string {
  return c.kind === "loopback" ? "loopback" : `input:${c.id}`;
}

export function captureFromKey(k: string): CaptureSource {
  return k.startsWith("input:") ? { kind: "input", id: k.slice("input:".length) } : { kind: "loopback" };
}

/** 録る所の名前（結果の行・選ぶ一覧） */
export function captureName(c: CaptureSource | null, inputs: LatencySetup["inputs"]): string {
  if (!c || c.kind === "loopback") return LOOPBACK_LABEL;
  return inputs.find((d) => d.id === c.id)?.name ?? "録音デバイス（見つかりません）";
}

/** 録る所の選び方の一覧。名前から自動で選んだものには「（自動）」を添える。先頭はループバック、続いて録音デバイス */
export function captureOptions(auto: CaptureSource, inputs: LatencySetup["inputs"]): { value: string; label: string }[] {
  const autoKey = captureKey(auto);
  const opts = [{ value: "loopback", label: LOOPBACK_LABEL }, ...inputs.map((d) => ({ value: `input:${d.id}`, label: `録音：${d.name}` }))];
  // 自動で選んだ録音デバイスが一覧に無い（その間に抜けた）なら、名前の分からない項目として足す
  if (!opts.some((o) => o.value === autoKey)) opts.push({ value: autoKey, label: captureName(auto, inputs) });
  return opts.map((o) => (o.value === autoKey ? { ...o, label: `${o.label}（自動）` } : o));
}

/**
 * 録る所の選択の今の値。選び直したものが今の一覧に無い（そのデバイスを抜いた）なら、選び直しを捨てて自動に戻す
 * （検査1回目の指摘3：見た目はループバックなのに、抜いたデバイスを送って「見つかりません」になっていた）
 */
export function selectedCapture(auto: CaptureSource, inputs: LatencySetup["inputs"], pick: string | undefined): string {
  if (pick === undefined) return captureKey(auto);
  return captureOptions(auto, inputs).some((o) => o.value === pick) ? pick : captureKey(auto);
}

/**
 * 選び直した録る所（画面を開いている間だけ覚える。出力先の名前ごと）。自動と同じ・今の一覧に無いものは送らない（Rust が名前から選ぶ）
 */
export function captureChoices(
  rows: LatencySetup["rows"],
  picked: Readonly<Record<string, string>>,
  inputs: LatencySetup["inputs"],
): { slot: number; capture: CaptureSource }[] {
  return rows
    .filter((r) => r.measurable)
    .flatMap((r) => {
      const k = selectedCapture(r.autoCapture, inputs, picked[r.name]);
      if (k === captureKey(r.autoCapture)) return [];
      return [{ slot: r.slot, capture: captureFromKey(k) }];
    });
}
