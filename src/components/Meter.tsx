// v3.8.0 段2-2：出力のメーター（仕様 2.7・4.4・6.2）。
// 値は 1 秒に 30 回ほど来るので、React の再描画を起こさない：形は1回だけ描き、受けるたびに参照（ref）から style と文字を直接書く。
// 決まり（dB・色の帯・直前の最大の線 20 秒・音割れの印 2 秒）は src\layout\meter.ts、受け口は src\stores\outputs.ts の subscribeMeter。
// - ミキサー（scale あり）：左右2本・-60〜0 dB の目盛り
// - 出力のプルダウン（scale なし）：細い2本（目盛りなし。値と色の帯・音割れの印はミキサーと同じ）
// 色の帯は緑 --c-ok・黄 --c-favorite・赤 --c-danger（新しい色の変数は足さない）。溝は --c-border の枠（Light でも見える）。
// 「動きを減らす」設定でも動かし続ける（移り変わりの動き transition は付けない。値をそのまま書く）
import { useEffect, useRef } from "react";
import { BAND_AT, dbFraction, METER, type MeterView } from "../layout/meter";
import { subscribeMeter } from "../stores/outputs";

/** 帯の色（緑・黄・赤）。-20 dB と -9 dB で切り替わる */
const FILL = `linear-gradient(to right, var(--c-ok) 0 ${BAND_AT.yellow * 100}%, var(--c-favorite) ${BAND_AT.yellow * 100}% ${BAND_AT.red * 100}%, var(--c-danger) ${BAND_AT.red * 100}% 100%)`;

/** 書く値が変わったときだけ書く（同じ値を毎回書かない） */
function put(el: HTMLElement | null, key: string, v: string, cache: Record<string, string>) {
  if (!el || cache[key] === v) return;
  cache[key] = v;
  (el.style as any)[key] = v;
}

export function MeterBars(props: { slot: number; scale?: boolean; label: string }) {
  const fills = useRef<(HTMLDivElement | null)[]>([null, null]);
  const holds = useRef<(HTMLDivElement | null)[]>([null, null]);
  useEffect(() => {
    const cache: Record<string, string>[] = [{}, {}, {}, {}];
    return subscribeMeter((views: readonly MeterView[]) => {
      const v = views[props.slot];
      if (!v) return;
      for (const c of [0, 1] as const) {
        put(fills.current[c], "clipPath", `inset(0 ${((1 - v.level[c]) * 100).toFixed(2)}% 0 0)`, cache[c]);
        const h = holds.current[c];
        put(h, "left", `${(Math.max(0, v.hold[c]) * 100).toFixed(2)}%`, cache[2 + c]);
        put(h, "visibility", v.hold[c] > 0 ? "visible" : "hidden", cache[2 + c]);
      }
    });
  }, [props.slot]);
  return (
    <div className={"meter" + (props.scale ? " with-scale" : " mini")} role="img" aria-label={props.label}>
      {[0, 1].map((c) => (
        <div key={c} className="meter-bar">
          <div className="meter-fill" ref={(el) => void (fills.current[c] = el)} style={{ background: FILL, clipPath: "inset(0 100% 0 0)" }} />
          <div className="meter-hold" ref={(el) => void (holds.current[c] = el)} style={{ visibility: "hidden" }} />
        </div>
      ))}
      {props.scale ? (
        <div className="meter-scale" aria-hidden="true">
          {METER.ticks.map((db) => (
            <span key={db} style={{ left: `${dbFraction(db) * 100}%` }}>
              {db}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** 音割れの印（2 秒残す。色だけでなく文字で）と、dB の数字（number があるときだけ。左右の大きいほう） */
export function MeterReadout(props: { slot: number; number?: boolean }) {
  const clip = useRef<HTMLSpanElement>(null);
  const num = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    let lastClip: boolean | null = null;
    let lastText = "";
    return subscribeMeter((views) => {
      const v = views[props.slot];
      if (!v) return;
      if (clip.current && v.clip !== lastClip) {
        lastClip = v.clip;
        clip.current.hidden = !v.clip;
      }
      if (num.current && v.text !== lastText) {
        lastText = v.text;
        num.current.textContent = v.text;
      }
    });
  }, [props.slot]);
  return (
    <>
      <span ref={clip} className="meter-clip" hidden>
        音割れ
      </span>
      {props.number ? (
        <span className="meter-db" title="今の大きさ（dB。左右の大きいほう）">
          <span ref={num}>-∞</span>
          <span className="meter-db-unit"> dB</span>
        </span>
      ) : null}
    </>
  );
}
