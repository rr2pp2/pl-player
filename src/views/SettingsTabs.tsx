// v3.3.0：設定画面の上のタブ。ドラッグで並べ替えられ、並びは設定（ui.settingsTabs）に保存する。
// - キーボード：←→・Home・End でタブを移る（移ると開く）。Space か Enter で持ち上げ、矢印キー（上下左右）で動かし、Space か Enter で置く（Esc で取りやめ）
// - 入りきらなければ、幅に合わせて複数行に折り返す（横には送らない。v3.3.0 の直し。前は行をまたぐと動きが分かりにくいので
//   1行にしていたが、作者の要望「横スクロールを無くしたい」で上書き）。
//   並べ替えの戦略は rectSortingStrategy（格子・折り返し向き。横1列用の horizontalListSortingStrategy は行をまたぐと位置がずれる）。
//   キーボードでの動かし方は sortableKeyboardCoordinates が上下左右の一番近いタブを選ぶので、折り返しても同じ部品で動く
// - 「元の順番に戻す」：保存した並びを消す（既定の順）
// 並びの合わせ方（知らない名前・抜けたタブ）は visibility.ts の mergeTabOrder。
import { useRef, useState } from "react";
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type Announcements,
  type DragEndEvent,
} from "@dnd-kit/core";
import { rectSortingStrategy, SortableContext, sortableKeyboardCoordinates, useSortable } from "@dnd-kit/sortable";
import { isDefaultTabOrder, moveTab, SETTINGS_TABS } from "../layout/visibility";

const labelOf = (id: string | number | undefined) => SETTINGS_TABS.find((t) => t.id === id)?.label ?? String(id ?? "");

const announcements: Announcements = {
  onDragStart: ({ active }) => `「${labelOf(active.id)}」のタブを持ち上げました。矢印キー（上下左右）で動かし、Space か Enter で置きます。Esc で取りやめます。`,
  onDragOver: ({ active, over }) => (over ? `「${labelOf(active.id)}」を「${labelOf(over.id)}」の位置へ動かしています。` : undefined),
  onDragEnd: ({ active, over }) => (over ? `「${labelOf(active.id)}」を「${labelOf(over.id)}」の位置に置きました。` : `「${labelOf(active.id)}」を置きました。`),
  onDragCancel: ({ active }) => `「${labelOf(active.id)}」の並べ替えを取りやめました。`,
};

function Tab(props: { id: string; active: boolean; onSelect: () => void; onKeyNav: (e: any) => void; setRef: (el: HTMLButtonElement | null) => void }) {
  const s = useSortable({ id: props.id });
  const attrs: any = { ...s.attributes };
  delete attrs.role;
  delete attrs.tabIndex;
  delete attrs["aria-pressed"];
  const listeners: any = s.listeners ?? {};
  return (
    <button
      {...attrs}
      {...listeners}
      ref={(el) => {
        s.setNodeRef(el);
        props.setRef(el);
      }}
      id={`settings-tab-${props.id}`}
      role="tab"
      aria-selected={props.active}
      aria-controls="settings-tabpanel"
      aria-roledescription="並べ替えできるタブ"
      tabIndex={props.active ? 0 : -1}
      className={"settings-tab" + (s.isDragging ? " dragging" : "")}
      style={{
        // 動かすのは位置だけ（縦横）。rectSortingStrategy が返す拡大率は使わない（タブの幅がまちまちなので、使うと文字が伸び縮みする）
        transform: s.transform ? `translate3d(${Math.round(s.transform.x)}px, ${Math.round(s.transform.y)}px, 0)` : undefined,
        transition: s.transition ?? undefined,
      }}
      onClick={props.onSelect}
      onKeyDown={(e) => {
        listeners.onKeyDown?.(e);
        if (!e.defaultPrevented) props.onKeyNav(e);
      }}
    >
      {labelOf(props.id)}
    </button>
  );
}

export function SettingsTabs(props: {
  order: string[];
  active: string;
  onSelect: (id: string) => void;
  onReorder: (next: string[]) => void;
  onReset: () => void;
}) {
  const refs = useRef(new Map<string, HTMLButtonElement>());
  const [dragging, setDragging] = useState(false);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const onKeyNav = (e: any) => {
    if (dragging) return;
    const i = props.order.indexOf(props.active);
    let to = -1;
    if (e.key === "ArrowRight") to = (i + 1) % props.order.length;
    else if (e.key === "ArrowLeft") to = (i - 1 + props.order.length) % props.order.length;
    else if (e.key === "Home") to = 0;
    else if (e.key === "End") to = props.order.length - 1;
    if (to < 0) return;
    e.preventDefault();
    const id = props.order[to];
    props.onSelect(id);
    refs.current.get(id)?.focus();
  };

  const onDragEnd = (e: DragEndEvent) => {
    setDragging(false);
    if (!e.over || e.active.id === e.over.id) return;
    props.onReorder(moveTab(props.order, String(e.active.id), String(e.over.id)));
    // キーボードで動かしたとき、フォーカスを動かしたタブに残す
    const id = String(e.active.id);
    requestAnimationFrame(() => refs.current.get(id)?.focus());
  };

  return (
    <div className="settings-tabs">
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragStart={() => setDragging(true)}
        onDragEnd={onDragEnd}
        onDragCancel={() => setDragging(false)}
        accessibility={{
          announcements,
          screenReaderInstructions: {
            draggable: "左右の矢印キーでタブを移ります。並べ替えるときは、Space か Enter で持ち上げ、矢印キー（上下左右）で動かし、Space か Enter で置きます。",
          },
        }}
      >
        <SortableContext items={props.order} strategy={rectSortingStrategy}>
          <div className="settings-tablist" role="tablist" aria-label="設定の項目（ドラッグで並べ替えられます）">
            {props.order.map((id) => (
              <Tab
                key={id}
                id={id}
                active={id === props.active}
                onSelect={() => props.onSelect(id)}
                onKeyNav={onKeyNav}
                setRef={(el) => {
                  if (el) refs.current.set(id, el);
                  else refs.current.delete(id);
                }}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>
      <button
        className="btn settings-tabs-reset"
        disabled={isDefaultTabOrder(props.order)}
        onClick={props.onReset}
        title="タブの並びを、はじめの順番に戻します"
      >
        元の順番に戻す
      </button>
    </div>
  );
}
