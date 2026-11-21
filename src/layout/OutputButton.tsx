// v3.8.0：再生バーの出力ボタンと、出力のプルダウン（仕様 4.3・4.4）。
// - ボタン：全体の音量の左に、中身で変わらない固定の幅（3列 --player-output-w、1段はアイコンだけの幅）。飾りを増やさず、押しやすい大きさ（高さ 40px）。
//   オンの出力先 0：消音のアイコン＋「出力なし」／1：アイコン＋名前（長ければ「…」）／2〜3：アイコンを横に並べる。1段はどの場合もアイコンだけ。
//   マウスを乗せる・キーボードで選ぶと、オンの出力先の名前を全部出す（吹き出し）。読み上げには aria-label で名前を渡す
//   （吹き出しの文は名前と同じなので、Tip の読み上げ用の文は付けない＝describe={false}）
// - プルダウン：押すと上に開く。行ごとに、チェック（オン/オフ）・アイコン・名前（ここでは差し替えない）・その出力だけの音量つまみ・
//   その下に細いメーター（目盛りなし）・%。下の段は無い（詳細へは上の帯のモード切替で入る）。出力先が無いときは「出力先がありません」
// 決まり（名前・アイコン・状態の文）は src\layout\outputs.ts
import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import type { OutputSlotView } from "../ipc/types";
import { notify } from "../stores/notices";
import { outputs } from "../stores/outputs";
import { useStore } from "../stores/store";
import { Tip } from "../components/Tip";
import { Popover } from "../components/Popover";
import { MeterBars, MeterReadout } from "../components/Meter";
import { OutputIconView } from "../components/Icons";
import { outputButtonModel, pct, registered, slotIcon, slotName, slotTitle, STATUS_TEXT, statusHint } from "./outputs";
import { LatencyButton } from "./LatencyDialog";

const fail = (e: unknown) => notify({ level: "error", error: e });

/**
 * 出力ごとの音量つまみ（今のモードの組）。動かしている間は outputSetVolume、離して 0.4 秒で outputCommitVolume（保存）。
 * 保存を待っている間に小窓を閉じた・詳細を抜けた（この部品が消えた）ときは、消えるときに保存を送る。
 * マウスホイールでも 5% ずつ（全体の音量のつまみと同じ）。ホイールはつまみの上では枠を送らない（passive でない受け口で止める）
 */
export function OutputVolume(props: { slot: OutputSlotView; label: string }) {
  const [local, setLocal] = useState<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const wrap = useRef<HTMLSpanElement>(null);
  const v = local ?? props.slot.volume;
  const set = (nv: number) => {
    const x = Math.round(Math.min(1, Math.max(0, nv)) * 100) / 100;
    setLocal(x);
    api.outputSetVolume(props.slot.slot, x).catch(fail);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = undefined;
      setLocal(null);
      void api.outputCommitVolume().catch(fail);
    }, 400);
  };
  useEffect(
    () => () => {
      if (timer.current === undefined) return;
      window.clearTimeout(timer.current);
      timer.current = undefined;
      void api.outputCommitVolume().catch(fail);
    },
    [],
  );
  // ホイール：React の onWheel は passive で preventDefault できないので、直接受ける。最新の値は参照で渡す
  const wheel = useRef<(dy: number) => void>(() => {});
  wheel.current = (dy) => set(v + (dy < 0 ? 0.05 : -0.05));
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const on = (e: WheelEvent) => {
      if (e.deltaY === 0) return;
      e.preventDefault();
      wheel.current(e.deltaY);
    };
    el.addEventListener("wheel", on, { passive: false });
    return () => el.removeEventListener("wheel", on);
  }, []);
  return (
    <span className="output-volume" ref={wrap}>
      <input
        type="range"
        className="range"
        aria-label={props.label}
        min={0}
        max={1}
        step={0.01}
        value={v}
        style={{ ["--pct" as any]: `${v * 100}%` }}
        title={`${props.label} ${pct(v)}（ホイールで調整）`}
        onChange={(e) => set(Number(e.target.value))}
      />
      <span className="output-pct">{pct(v)}</span>
    </span>
  );
}

/** 名前の右の状態の文字（切断中・開けません・重複）。理由は吹き出し */
export function OutputStatusText(props: { slot: OutputSlotView }) {
  const st = STATUS_TEXT[props.slot.status];
  if (!st) return null;
  const hint = statusHint(props.slot);
  return (
    <Tip text={hint} delay={300} className="inline output-status-tip">
      <span className="output-status" tabIndex={0}>
        {st}
      </span>
    </Tip>
  );
}

function PopupRow(props: { s: OutputSlotView }) {
  const s = props.s;
  const name = slotName(s);
  return (
    <div className={"output-pop-row" + (s.on ? "" : " off")}>
      <label className="output-pop-check" title={slotTitle(s)}>
        <input type="checkbox" checked={s.on} onChange={(e) => api.outputSetOn(s.slot, e.target.checked).catch(fail)} aria-label={`${name}：オン／オフ`} />
        <span className="output-icon" aria-hidden="true">
          <OutputIconView icon={slotIcon(s)} />
        </span>
        <span className="output-name">{name}</span>
      </label>
      <OutputStatusText slot={s} />
      <MeterReadout slot={s.slot} />
      <div className="output-pop-level">
        <OutputVolume slot={s} label={`${name}の音量`} />
        <MeterBars slot={s.slot} label={`${name}のメーター`} />
      </div>
    </div>
  );
}

function OutputPopup(props: { onLatency: () => void }) {
  const v = useStore(outputs);
  const all = registered(v);
  if (all.length === 0) return <div className="output-pop-empty">出力先がありません</div>;
  return (
    <>
      {all.map((s) => (
        <PopupRow key={s.slot} s={s} />
      ))}
      {/* v3.8.0：遅れを測る（仕様 2.3  で、プルダウンにも入口を置くと決めた）。押せないときは理由を文字でも出す */}
      <div className="output-pop-foot">
        <LatencyButton showReason onOpen={props.onLatency} className="small-btn" />
      </div>
    </>
  );
}

export function OutputButton(props: { one: boolean }) {
  const v = useStore(outputs);
  const m = outputButtonModel(v);
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const close = (focusBack: boolean) => {
    setOpen(false);
    if (focusBack) btn.current?.focus();
  };
  const showText = !props.one && m.text !== null;
  // 鳴らせない状態：
  // - 3列で出力先が1つだけ：アイコンを外し、ボタンの中の幅いっぱい（94px）を状態の文字に使う（「開けません」まで「…」で切れずに入る）
  // - それ以外（1段・2〜3つ）：鳴らせない出力のアイコンの角に「!」を重ねる（幅を増やさない。1段でアイコンが3つ並んでも出る）
  // 危険の色はボタンの縁にも使うが、色だけでは伝えない（文字か「!」がいつも出る）
  const stateOnly = showText && m.kind === "one" && m.alert;
  return (
    <>
      <Tip text={m.tip} delay={300} place="above" className="inline output-btn-tip" describe={false}>
        <button
          ref={btn}
          className={
            "output-btn" +
            (props.one ? " one" : "") +
            (open ? " on" : "") +
            (m.kind === "off" || m.kind === "none" ? " silent" : "") +
            (m.alert ? " alert" : "")
          }
          onClick={() => setOpen((o) => !o)}
          aria-label={m.label}
          aria-haspopup="dialog"
          aria-expanded={open}
        >
          {stateOnly ? null : (
            <span className="output-icons" aria-hidden="true">
              {m.icons.map((ic, i) => (
                <span key={i} className="output-icon">
                  <OutputIconView icon={ic} />
                  {m.iconAlerts[i] ? <span className="output-icon-mark">!</span> : null}
                </span>
              ))}
            </span>
          )}
          {showText ? <span className="output-btn-text">{m.text}</span> : null}
        </button>
      </Tip>
      {open ? (
        <Popover anchor={btn} place="above" label="出力先" onClose={close} className="output-pop">
          <OutputPopup onLatency={() => close(false)} />
        </Popover>
      ) : null}
    </>
  );
}
