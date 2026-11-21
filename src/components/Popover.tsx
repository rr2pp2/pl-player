// v3.8.0：ボタンから開く小窓（出力のプルダウン・ミキサーの「…」・「＋ 出力先を追加」の一覧）。
// 再生バーの「···」（PlayerBar の PlayerMore）と同じ決まり：body の直下に出す（backdrop-filter の枠の中で fixed がずれるため）・
// 開いたら中の最初の操作へフォーカス・Esc で閉じてボタンへ戻る（歌詞の画面の Esc より先に受ける）・外を押す・窓の大きさが変わる・窓から離れると閉じる。
// 置き場所：above はボタンの上（右端をボタンにそろえる）、below はボタンの下（入りきらなければ上）。画面の外へははみ出さない（高さは中で送る）
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";

/** 小窓の中でフォーカスが移る部品（開いたときに最初のものへ移る。Shift+Tab でボタンへ戻る基準） */
const FOCUSABLE = "button:not(:disabled), input:not(:disabled), [tabindex='0']";

export function Popover(props: {
  anchor: { current: HTMLElement | null };
  place: "above" | "below";
  label: string;
  onClose: (focusBack: boolean) => void;
  className?: string;
  children: any;
}) {
  const pop = useRef<HTMLDivElement>(null);
  const [style, setStyle] = useState<CSSProperties>({ visibility: "hidden", right: 8, bottom: 8 });
  const close = useRef(props.onClose);
  close.current = props.onClose;
  useLayoutEffect(() => {
    const a = props.anchor.current;
    const el = pop.current;
    if (!a || !el) return;
    const r = a.getBoundingClientRect();
    const h = el.getBoundingClientRect().height;
    const right = Math.max(8, window.innerWidth - r.right);
    const spaceAbove = r.top - 8 - 8;
    const spaceBelow = window.innerHeight - r.bottom - 8 - 8;
    const above = props.place === "above" || (h > spaceBelow && spaceAbove > spaceBelow);
    if (above) setStyle({ right, bottom: Math.max(8, window.innerHeight - r.top + 8), maxHeight: Math.max(120, spaceAbove) });
    else setStyle({ right, top: r.bottom + 6, maxHeight: Math.max(120, spaceBelow) });
    // 開いたら中の最初の操作へ（キーボードで続けて選べるように）
    el.querySelector<HTMLElement>(FOCUSABLE)?.focus();
  }, []); // eslint-disable-line
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (pop.current?.contains(t) || props.anchor.current?.contains(t)) return;
      close.current(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close.current(true);
      }
    };
    const onLeave = () => close.current(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", onLeave);
    window.addEventListener("blur", onLeave);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("resize", onLeave);
      window.removeEventListener("blur", onLeave);
    };
  }, []); // eslint-disable-line
  // 最初の操作で Shift+Tab を押したら、小窓を閉じて開いたボタンへ戻る（body の直下に出しているので、
  // そのままだと DOM の順で別の場所へ移ってしまう）
  const onKeyDown = (e: { key: string; shiftKey: boolean; preventDefault: () => void }) => {
    if (e.key !== "Tab" || !e.shiftKey) return;
    const first = pop.current?.querySelector<HTMLElement>(FOCUSABLE);
    if (first && document.activeElement === first) {
      e.preventDefault();
      close.current(true);
    }
  };
  return createPortal(
    <div
      ref={pop}
      className={"popover " + (props.className ?? "")}
      role="dialog"
      aria-label={props.label}
      style={style}
      data-capture-keys
      onKeyDown={onKeyDown}
    >
      {props.children}
    </div>,
    document.body,
  );
}
