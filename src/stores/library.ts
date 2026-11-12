// どの一覧を出しているか・検索語・並び順・選択・プレイリスト一覧の写し。
import { api } from "../ipc/commands";
import { on } from "../ipc/events";
import type { ImportProgress, Playlist, ViewKind } from "../ipc/types";
import { createStore } from "./store";
import { notify } from "./notices";

export interface Route {
  view: ViewKind;
  /** artist / album / genre の鍵、playlist の ID */
  key?: string | null;
  /** 画面の見出し（アーティスト名・アルバム名など） */
  title?: string;
}

export const route = createStore<Route>({ view: "all" });
export const search = createStore<string>("");
export const sort = createStore<{ by: string | null; desc: boolean }>({ by: null, desc: false });
export const playlists = createStore<Playlist[]>([]);
/** ライブラリが変わった回数（一覧を引き直す合図） */
export const libraryVersion = createStore<number>(0);
export const queueVersion = createStore<number>(0);
export const playlistsVersion = createStore<number>(0);
export const importProgress = createStore<ImportProgress | null>(null);

export function navigate(r: Route) {
  route.set(r);
  sort.set({ by: null, desc: false });
}

export async function refreshPlaylists() {
  try {
    playlists.set(await api.playlists());
  } catch (e) {
    notify({ level: "error", error: e });
  }
}

export async function initLibrary() {
  await refreshPlaylists();
  await on("library", () => libraryVersion.set((v) => v + 1));
  await on("queue", () => queueVersion.set((v) => v + 1));
  await on("playlists", () => {
    playlistsVersion.set((v) => v + 1);
    void refreshPlaylists();
  });
  await on("importProgress", (p) => importProgress.set(p));
  await on("importDone", (s) => {
    importProgress.set(null);
    const parts: string[] = [];
    if (s.added) parts.push(`${s.added}曲を追加`);
    if (s.updated) parts.push(`${s.updated}曲を更新`);
    if (!s.added && !s.updated) parts.push(s.unchanged ? "新しい曲はありませんでした" : "取り込める曲がありませんでした");
    notify({ level: "info", text: `取り込みが終わりました（${parts.join("・")}）` });
    if (s.unsupported.length)
      notify({ level: "error", text: `非対応形式のため取り込めなかったファイル：${s.unsupported.length}件`, detail: s.unsupported.slice(0, 5).join("\n") });
    if (s.tagErrors.length)
      notify({ level: "error", text: `タグを読み込めなかったファイル：${s.tagErrors.length}件（ファイル名で取り込みました）`, detail: s.tagErrors.slice(0, 5).join("\n") });
    if (s.notFound.length)
      notify({ level: "error", text: `見つからなかったファイル：${s.notFound.length}件`, detail: s.notFound.slice(0, 5).join("\n") });
    if (s.accessDenied.length)
      notify({ level: "error", text: `アクセスを拒否されたファイル・フォルダ：${s.accessDenied.length}件`, detail: s.accessDenied.slice(0, 5).join("\n") });
    if (s.network.length)
      notify({ level: "error", text: `ネットワーク上の場所なので取り込まなかったもの：${s.network.length}件（この PC のドライブにある場所だけを取り込みます）`, detail: s.network.slice(0, 5).join("\n") });
    if (s.otherErrors.length)
      notify({ level: "error", text: `取り込み中にエラーが起きたもの：${s.otherErrors.length}件`, detail: s.otherErrors.slice(0, 5).join("\n") });
  });
}
