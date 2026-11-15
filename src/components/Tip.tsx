// v3.3.0：自前の吹き出し。マウスを乗せて少し止めたとき、またはキーボードで移ったときに出す。
// ブラウザの title は出るまでが遅く、キーボードでは出ない。そこで、
// - 目で見る人：マウスを `delay` ミリ秒止めると出す。キーボードで移ったら（:focus-visible）すぐ出す。Esc・離れる・押すで消す
// - 読み上げ：吹き出しと同じ文を見えない文字で置き、包んだ要素の aria-describedby で結ぶ（吹き出しそのものは読ませない）
// 吹き出しは document.body の直下に出す（サイドバーなど backdrop-filter を持つ枠の中では position: fixed がずれるため）。
// 文が無いとき（text が null・空）も、同じ形で包む（包み方を変えると、包んだボタンが作り直されてフォーカスが外れる。tipParts.ts）。
import { cloneElement, useEffect, useId, useRef, useState, type ReactElement } from "react";
import { createPortal } from "react-dom";
import { tipParts } from "./tipParts";

export function Tip(props: {
  text: string | null;
  children: ReactElement<any>;
  delay?: number;
  className?: string;
  place?: "right" | "below";
  /** v3.8.0：false なら読み上げ用の文と aria-describedby を付けない（包んだ要素の aria-label と同じ文を二重に読ませないため。吹き出しは出す） */
  describe?: boolean;
}) {
  const id = useId();
  const ref = useRef<HTMLSpanElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const [pos, setPos] = useState<{ left: number; top: number; below: boolean } | null>(null);
  const descId = `${id}-tip`;
  const full = tipParts(props.text, descId);
  // 読み上げを付けないときも、包み方（span と子へ渡す属性の形）は同じ。結びつけと見えない文だけを外す
  const parts = props.describe === false ? { ...full, childProps: {}, srText: null } : full;
  useEffect(() => () => window.clearTimeout(timer.current), []);
  // 文が無くなったら、出ている吹き出しと待っている表示を消す
  useEffect(() => {
    if (!parts.canShow) {
      window.clearTimeout(timer.current);
      setPos(null);
    }
  }, [parts.canShow]);
  const place = () => {
    const el = ref.current?.firstElementChild ?? ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    // v3.8.0：below は下に出す（上の帯の右端のモード切替。右に出すと画面の外へはみ出すため）。左端をそろえ、右へはみ出すなら左へ寄せる
    if (props.place === "below") {
      setPos({ left: Math.max(8, Math.min(r.left, window.innerWidth - 288)), top: r.bottom + 6, below: true });
      return;
    }
    // 右に出す。はみ出すときは下に出す（狭いウィンドウ）
    const left = Math.min(r.right + 8, Math.max(8, window.innerWidth - 300));
    setPos({ left, top: r.top + r.height / 2, below: false });
  };
  const show = (now: boolean) => {
    window.clearTimeout(timer.current);
    if (!parts.canShow) return;
    if (now) place();
    else timer.current = window.setTimeout(place, props.delay ?? 500);
  };
  const hide = () => {
    window.clearTimeout(timer.current);
    setPos(null);
  };
  return (
    <span
      ref={ref}
      className={"tip-anchor " + (props.className ?? "")}
      onMouseEnter={() => show(false)}
      onMouseLeave={hide}
      onFocus={(e) => {
        const t = e.target as HTMLElement;
        if (t.matches?.(":focus-visible")) show(true);
      }}
      onBlur={hide}
      onPointerDown={hide}
      onKeyDown={(e) => {
        if (e.key === "Escape" && pos) {
          e.stopPropagation();
          hide();
        }
      }}
    >
      {cloneElement(props.children, parts.childProps)}
      {parts.srText !== null ? (
        <span id={descId} className="sr-only">
          {parts.srText}
        </span>
      ) : null}
      {pos && parts.canShow
        ? createPortal(
            <div className={"tip-bubble" + (pos.below ? " below" : "")} style={{ left: pos.left, top: pos.top }} aria-hidden="true">
              {full.srText}
            </div>,
            document.body,
          )
        : null}
    </span>
  );
}
