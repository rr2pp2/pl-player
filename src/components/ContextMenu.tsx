// 右クリックのメニュー（1段の入れ子まで）。
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createStore, useStore } from "../stores/store";
import { IconChevron } from "./Icons";

export interface MenuItem {
  label?: string;
  icon?: any;
  danger?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  submenu?: MenuItem[];
  separator?: boolean;
}

const menuState = createStore<{ x: number; y: number; items: MenuItem[] } | null>(null);

export function openMenu(e: { clientX: number; clientY: number; preventDefault?: () => void }, items: MenuItem[]) {
  e.preventDefault?.();
  menuState.set({ x: e.clientX, y: e.clientY, items });
}

export function closeMenu() {
  menuState.set(null);
}

function MenuBox(props: { x: number; y: number; items: MenuItem[]; level: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: props.x, y: props.y });
  const [open, setOpen] = useState<number | null>(null);
  const [subPos, setSubPos] = useState({ x: 0, y: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    let x = props.x;
    let y = props.y;
    if (x + r.width > window.innerWidth - 8) x = Math.max(8, props.level ? props.x - r.width * 2 : window.innerWidth - r.width - 8);
    if (y + r.height > window.innerHeight - 8) y = Math.max(8, window.innerHeight - r.height - 8);
    setPos({ x, y });
  }, [props.x, props.y, props.level]);
  return (
    <div className="menu" ref={ref} style={{ left: pos.x, top: pos.y }} onMouseDown={(e) => e.stopPropagation()} role="menu">
      {props.items.map((it, i) =>
        it.separator ? (
          <div key={i} className="menu-sep" />
        ) : (
          <button
            key={i}
            role="menuitem"
            className={"menu-item" + (it.danger ? " danger" : "") + (open === i ? " open" : "")}
            disabled={it.disabled}
            onMouseEnter={(e) => {
              if (it.submenu) {
                const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                setSubPos({ x: r.right + 2, y: r.top - 6 });
                setOpen(i);
              } else {
                setOpen(null);
              }
            }}
            onClick={() => {
              if (it.submenu) return;
              closeMenu();
              it.onClick?.();
            }}
          >
            {it.icon ?? null}
            <span style={{ flex: 1 }}>{it.label}</span>
            {it.submenu ? <IconChevron /> : null}
          </button>
        ),
      )}
      {open !== null && props.items[open]?.submenu ? (
        <MenuBox x={subPos.x} y={subPos.y} items={props.items[open].submenu!} level={props.level + 1} />
      ) : null}
    </div>
  );
}

export function ContextMenuHost() {
  const m = useStore(menuState);
  useEffect(() => {
    if (!m) return;
    const close = () => closeMenu();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeMenu();
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [m]);
  if (!m) return null;
  return <MenuBox x={m.x} y={m.y} items={m.items} level={0} />;
}
