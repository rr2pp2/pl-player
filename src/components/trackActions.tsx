// 曲に対する操作（右クリックのメニュー・ドラッグの受け手で共用）。
import { api } from "../ipc/commands";
import type { Track } from "../ipc/types";
import { playlists, refreshPlaylists } from "../stores/library";
import { notify } from "../stores/notices";
import { createStore } from "../stores/store";
import type { MenuItem } from "./ContextMenu";
import { confirmDialog, promptText } from "./Modal";
import { IconEdit, IconFolder, IconList, IconPlay, IconPlus, IconQueue, IconStar, IconTrash } from "./Icons";

/** タグ編集の小窓で開いている曲 */
export const editorTrack = createStore<number | null>(null);

const run = (p: Promise<unknown>) => p.catch((e) => notify({ level: "error", error: e }));

export async function addToPlaylist(playlistId: number, ids: number[], name?: string) {
  await run(api.playlistAddTracks(playlistId, ids));
  notify({ level: "info", text: `${name ? `「${name}」に` : "プレイリストに"}${ids.length}曲を追加しました` });
}

export async function newPlaylistWith(ids: number[]) {
  const name = await promptText("新しいプレイリスト", "名前", "", "作成");
  if (!name) return;
  await run(api.playlistCreate(name, ids));
  await refreshPlaylists();
  notify({ level: "info", text: `プレイリスト「${name}」を作成しました` });
}

export function trackMenu(opts: {
  tracks: Track[];
  /** 再生するときの並び（今の一覧）と、その中の位置 */
  playAll?: () => void;
  extra?: MenuItem[];
}): MenuItem[] {
  const ids = opts.tracks.map((t) => t.id);
  const one = opts.tracks.length === 1 ? opts.tracks[0] : null;
  const allFav = opts.tracks.every((t) => t.favorite);
  const pls = playlists.get();
  const items: MenuItem[] = [];
  if (opts.playAll) items.push({ label: "再生", icon: <IconPlay />, onClick: opts.playAll });
  items.push(
    { label: "次に再生", icon: <IconQueue />, onClick: () => void run(api.queuePlayNext(ids)) },
    { label: "キューに追加", icon: <IconQueue />, onClick: () => void run(api.queueAdd(ids)) },
    {
      label: "プレイリストに追加",
      icon: <IconList />,
      submenu: [
        { label: "新しいプレイリスト…", icon: <IconPlus />, onClick: () => void newPlaylistWith(ids) },
        ...(pls.length ? [{ separator: true } as MenuItem] : []),
        ...pls.map((p) => ({ label: p.name, onClick: () => void addToPlaylist(p.id, ids, p.name) })),
      ],
    },
    {
      label: allFav ? "お気に入りから外す" : "お気に入りに追加",
      icon: <IconStar filled={!allFav} />,
      onClick: () => void run(api.setFavorite(ids, !allFav)),
    },
    { separator: true },
  );
  if (one) {
    items.push(
      { label: "タグを編集…", icon: <IconEdit />, onClick: () => editorTrack.set(one.id) },
      { label: "エクスプローラーで表示", icon: <IconFolder />, onClick: () => void run(api.revealInExplorer(one.path)) },
    );
  }
  if (opts.extra?.length) items.push({ separator: true }, ...opts.extra);
  items.push(
    { separator: true },
    {
      label: "ライブラリから外す…",
      icon: <IconTrash />,
      danger: true,
      onClick: async () => {
        const ok = await confirmDialog(
          "ライブラリから外す",
          `${ids.length}曲をライブラリから外します。\n音声ファイルそのものは消しません。プレイリストとキューからも外れます。`,
          "外す",
          true,
        );
        if (ok) await run(api.libraryRemoveTracks(ids));
      },
    },
  );
  return items;
}
