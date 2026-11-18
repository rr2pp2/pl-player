// v3.8.0 段2-2：出力先（同時再生）の見せ方の決まり。ほかに依存しない純粋な関数だけを置く（scripts\ui-detail.test.mts から確かめる）。
//
// 仕様（同時再生と配信モード 4.3・4.4・6.2・6.4・7章）：
// - 出力ボタン：オンの出力先が 0 なら消音のアイコン＋「出力なし」、1 ならそのアイコン＋名前、2〜3 ならアイコンを横に並べる（名前は出さない）。
//   1段のときはどの場合もアイコンだけ。マウスを乗せる・キーボードで選ぶと、オンの出力先の名前を全部出す（読み上げにも渡す）
// - 出力先がまだ1つも無いときは「出力先がありません」（全部オフとは分ける）
// - 状態は色だけで伝えない：切断中・開けません・重複は文字を出し、理由はマウスを乗せると出す
// - 同じデバイスを2つの出力先には入れない（ほかの出力先で使っているデバイスは一覧に出さない。比べるのは ID。名前では比べない）
import type { DeviceInfo, OutputIcon, OutputSlotView, OutputsView, OutputStatus } from "../ipc/types";

/** 選べるアイコン（並びは「…」の中に出す順）。配信（仮想ケーブル）の呼び名はそのまま（6章の頭の注） */
export const OUTPUT_ICONS: readonly { id: OutputIcon; label: string }[] = [
  { id: "headphones", label: "ヘッドホン" },
  { id: "speaker", label: "スピーカー" },
  { id: "stream", label: "配信（仮想ケーブル）" },
  { id: "monitor", label: "モニター" },
];

export const DEFAULT_DEVICE_NAME = "既定のデバイス";
/** 3.7.0 から引き継いだ行で、そのとき PC に無かったデバイス（名前を持っていない） */
export const UNKNOWN_DEVICE_NAME = "前に使っていたデバイス";

/** 行・ボタンに出す名前 */
export function slotName(s: Pick<OutputSlotView, "deviceId" | "deviceName" | "savedName">): string {
  if (s.deviceId === null) return DEFAULT_DEVICE_NAME;
  return s.deviceName || s.savedName || UNKNOWN_DEVICE_NAME;
}

/** 名前の吹き出し（既定のデバイスなら、今どのデバイスか） */
export function slotTitle(s: Pick<OutputSlotView, "deviceId" | "deviceName" | "savedName">): string {
  if (s.deviceId === null) return s.deviceName ? `${DEFAULT_DEVICE_NAME}（Windows の設定に従う。今は ${s.deviceName}）` : `${DEFAULT_DEVICE_NAME}（Windows の設定に従う）`;
  return slotName(s);
}

export function slotIcon(s: Pick<OutputSlotView, "icon" | "autoIcon">): OutputIcon {
  return s.icon ?? s.autoIcon;
}

/** 状態の文字（オフ・正常は出さない。色だけで伝えないため、出すものは必ず文字） */
export const STATUS_TEXT: Partial<Record<OutputStatus, string>> = {
  disconnected: "切断中",
  unavailable: "開けません",
  duplicate: "重複",
};

/** 状態の理由（マウスを乗せると出す） */
export function statusHint(s: Pick<OutputSlotView, "status" | "reason">): string | null {
  if (s.status === "disconnected") return "デバイスが見つかりません（抜けた・この PC に無い）。つながると自動で戻ります";
  if (s.status === "duplicate") return "ほかの出力先と同じデバイスです（既定のデバイスがほかの出力先と同じになりました）。この出力先は鳴らしません";
  if (s.status === "unavailable") {
    if (s.reason === "format") return "このデバイスの形式は使えません";
    if (s.reason === "busy") return "ほかのアプリが独り占めしています";
    return "このデバイスを開けませんでした";
  }
  return null;
}

/** 鳴っている（メーターを動かす）か */
export const slotSounding = (s: Pick<OutputSlotView, "status"> | null | undefined): boolean => s?.status === "ok";

export function registered(v: Pick<OutputsView, "slots">): OutputSlotView[] {
  return v.slots.filter((s): s is OutputSlotView => s !== null && s !== undefined);
}

export interface OutputButtonModel {
  kind: "none" | "off" | "one" | "many";
  /** 並べるアイコン（"muted" は消音のアイコン） */
  icons: (OutputIcon | "muted")[];
  /** 3列のときにアイコンの右に出す文字（2〜3 のときは null） */
  text: string | null;
  /** 読み上げの名前（ボタンの aria-label） */
  label: string;
  /** 吹き出し（オンの出力先の名前を全部。状態があれば添える） */
  tip: string;
  /**
   * オンの出力先に、鳴らせない状態（切断中・開けません・重複）のものがある（段2-2 検査1回目の指摘2）。
   * 1つだけのときは text が状態の文字になる。ボタンには文字の印「!」も出す（色だけで伝えない）
   */
  alert: boolean;
  /**
   * アイコンごとに、その出力先が鳴らせない状態か（icons と同じ並び）。ボタンはそのアイコンの角に「!」を重ねる
   * （幅を増やさずに、1段でアイコンが3つ並ぶときも色だけで伝えないため。段2-2 検査2回目の N1）
   */
  iconAlerts: boolean[];
}

/** 出力ボタンに出すもの（4.3） */
export function outputButtonModel(v: Pick<OutputsView, "slots">): OutputButtonModel {
  const all = registered(v);
  if (all.length === 0)
    return {
      kind: "none",
      icons: ["muted"],
      text: "出力なし",
      label: "出力先：出力先がありません",
      tip: "出力先がありません",
      alert: false,
      iconAlerts: [false],
    };
  const on = all.filter((s) => s.on);
  if (on.length === 0)
    return {
      kind: "off",
      icons: ["muted"],
      text: "出力なし",
      label: "出力先：出力なし（全部オフ）",
      tip: "出力なし（全部オフ）",
      alert: false,
      iconAlerts: [false],
    };
  const named = on.map((s) => {
    const st = STATUS_TEXT[s.status];
    return st ? `${slotName(s)}（${st}）` : slotName(s);
  });
  // 1つだけで鳴らせない状態なら、見える文字は名前ではなく状態（名前は吹き出しと読み上げに残る）
  const oneStatus = on.length === 1 ? STATUS_TEXT[on[0].status] : undefined;
  return {
    kind: on.length === 1 ? "one" : "many",
    icons: on.map(slotIcon),
    text: on.length === 1 ? oneStatus ?? slotName(on[0]) : null,
    label: `出力先：${named.join("、")}`,
    tip: named.join("\n"),
    alert: on.some((s) => STATUS_TEXT[s.status] !== undefined),
    iconAlerts: on.map((s) => STATUS_TEXT[s.status] !== undefined),
  };
}

/** デバイスの比べる鍵（既定のデバイスは "default"。Rust の outputs.rs の device_key と同じ） */
export const deviceKey = (id: string | null | undefined): string => id ?? "default";

export interface DeviceChoice {
  /** null は既定のデバイス */
  id: string | null;
  name: string;
  /** 今の既定（一覧に添える） */
  isDefault: boolean;
}

/**
 * 選べるデバイス（先頭は既定のデバイス）。ほかの出力先で使っているものは出さない（2.1）。
 * slot は差し替える枠（その枠が今使っているデバイスは出す）。足すときは null
 */
export function deviceChoices(devices: readonly DeviceInfo[], v: Pick<OutputsView, "slots">, slot: number | null): DeviceChoice[] {
  const used = new Set(registered(v).filter((s) => s.slot !== slot).map((s) => deviceKey(s.deviceId)));
  const list: DeviceChoice[] = [{ id: null, name: `${DEFAULT_DEVICE_NAME}（Windows の設定に従う）`, isDefault: false }];
  for (const d of devices) list.push({ id: d.id, name: d.name, isDefault: d.isDefault });
  return list.filter((c) => !used.has(deviceKey(c.id)));
}

/** 出力先を足せるか（3つまで） */
export const canAddOutput = (v: Pick<OutputsView, "slots">): boolean => registered(v).length < 3;

/** 出力ごとの音量つまみ・全体の音量の % */
export const pct = (v: number): string => `${Math.round(Math.min(1, Math.max(0, v)) * 100)}%`;
