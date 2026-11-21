// エクスプローラーからのドラッグ＆ドロップ（Tauri の dragDropEnabled）。
//
// 落とされた場所に data-drop の印がある要素（アルバムアートなど）があれば、その受け手に画像を渡す。
// 印が無ければ、曲の取り込み（ファイル・フォルダ）として扱う。
// v3.11.0：設定の「アイコン」の欄の枠（data-drop="icon:<名前>:<状態>"）に落としたものは、取り込みに回さずその枠へ。
// 枠には落とした位置だけを渡し、パスは Rust が自分で控えたものを使う（複数・フォルダ・ファイルでないものは Rust が断る）。
// 振り分けの決まりは src\skins\iconSlots.ts の routeDrop（枠の外に落としたものは今までどおり）
// 画面内の並べ替え（dnd-kit、ポインタ操作）とは別の仕組みなので、互いにぶつからない。
import { useEffect } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api } from "../ipc/commands";
import { createStore, useStore } from "../stores/store";
import { notify } from "../stores/notices";
import { routeDrop } from "../skins/iconSlots";

export const dropState = createStore<{ active: boolean; zone: string | null }>({ active: false, zone: null });

/** 受け手：落としたパスと、落とした位置（ドロップの知らせのまま。ウィンドウの中の物理ピクセル） */
type Handler = (paths: string[], position: { x: number; y: number }) => void;
const zones = new Map<string, Handler>();

/** 受け手を登録する（戻り値で外す） */
export function registerDropZone(key: string, handler: Handler): () => void {
  zones.set(key, handler);
  return () => {
    if (zones.get(key) === handler) zones.delete(key);
  };
}

export const IMAGE_RE = /\.(png|jpe?g|webp)$/i;

function zoneAt(x: number, y: number): string | null {
  const dpr = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(x / dpr, y / dpr) as HTMLElement | null;
  const z = el?.closest?.("[data-drop]") as HTMLElement | null;
  const key = z?.dataset.drop ?? null;
  return key && zones.has(key) ? key : null;
}

export function useFileDrop() {
  useEffect(() => {
    let un: (() => void) | undefined;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter" || p.type === "over") {
          dropState.set({ active: true, zone: zoneAt(p.position.x, p.position.y) });
        } else if (p.type === "leave") {
          dropState.set({ active: false, zone: null });
        } else if (p.type === "drop") {
          const zone = zoneAt(p.position.x, p.position.y);
          dropState.set({ active: false, zone: null });
          const paths = p.paths ?? [];
          const route = routeDrop(zone, paths.length);
          if (route === "none") return;
          if (route === "zone" && zone) {
            zones.get(zone)?.(paths, { x: p.position.x, y: p.position.y });
            return;
          }
          api.libraryImport(paths).catch((e) => notify({ level: "error", error: e }));
        }
      })
      .then((f) => {
        if (disposed) f();
        else un = f;
      });
    return () => {
      disposed = true;
      un?.();
    };
  }, []);
}

/** 取り込みのドロップ中に全体へ出す枠 */
export function DropOverlay() {
  const s = useStore(dropState);
  if (!s.active || s.zone) return null;
  return <div className="drop-overlay">ここにドロップして曲を取り込む（ファイル・フォルダ）</div>;
}
