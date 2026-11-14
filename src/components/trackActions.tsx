// 曲に対する操作（右クリックのメニュー・ドラッグの受け手で共用）。
import { api } from "../ipc/commands";
import type { Track } from "../ipc/types";
import { playlists, refreshPlaylists } from "../stores/library";
import { notify } from "../stores/notices";
import { createStore } from "../stores/store";
import { errorText } from "../messages/ja";
import { trashConfirmText, trashMenuShown, trashTooMany, type TrackListKind } from "../layout/visibility";
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

/** ごみ箱へ移している途中（二重に押させない） */
export const trashBusy = createStore(false);

/**
 * v3.3.0：曲ファイルをごみ箱へ移す（確認のあと）。Rust へは曲の id だけを渡す。
 * 移せた曲はライブラリ・プレイリスト・キューからも外れる。移せなかった曲は、種別ごとに知らせる（ファイルもライブラリもそのまま）
 */
export async function trashTracksFlow(tracks: Track[]) {
  const seen = new Set<number>();
  const list = tracks.filter((t) => !seen.has(t.id) && (seen.add(t.id), true));
  if (!list.length || trashBusy.get()) return;
  // 上限（1,000 曲）を超えたら、確認の前に断る（Rust も同じ数で断る）
  if (trashTooMany(list.length)) {
    notify({ level: "error", text: `${list.length}曲を選んでいます。${errorText.trash_too_many}` });
    return;
  }
  const c = trashConfirmText(list);
  const ok = await confirmDialog(c.title, c.body, c.ok, true);
  if (!ok) return;
  trashBusy.set(true);
  try {
    const r = await api.libraryTrashTracks(list.map((t) => t.id));
    if (r.moved.length) notify({ level: "info", text: `${r.moved.length}曲をごみ箱へ移しました（ごみ箱から戻せます）` });
    const byCode = new Map<string, number[]>();
    for (const f of r.failed) byCode.set(f.code, [...(byCode.get(f.code) ?? []), f.trackId]);
    const names = (ids: number[]) => {
      const n = ids.map((id) => list.find((t) => t.id === id)?.title ?? `#${id}`);
      return n.slice(0, 5).join("\n") + (n.length > 5 ? `\nほか ${n.length - 5} 曲` : "");
    };
    for (const [code, ids] of byCode) {
      notify({
        level: "error",
        text: `${ids.length}曲をごみ箱へ移せませんでした。${errorText[code as keyof typeof errorText] ?? errorText.other}`,
        detail: names(ids),
      });
    }
    // ファイルはごみ箱へ移せたのに、ライブラリから外せなかった曲（一覧に残る。「ライブラリから外す」で外せる）
    const left = r.leftInLibrary ?? [];
    if (left.length) {
      const code = left[0].code;
      notify({
        level: "error",
        text: `${left.length}曲はごみ箱へ移しましたが、ライブラリから外せませんでした。${errorText[code] ?? errorText.other}右クリックの「ライブラリから外す…」で外せます。`,
        detail: names(left.map((f) => f.trackId)),
      });
    }
  } catch (e) {
    notify({ level: "error", error: e });
  } finally {
    trashBusy.set(false);
  }
}

export function trackMenu(opts: {
  tracks: Track[];
  /** どこのメニューか（「ごみ箱へ移す」はライブラリの曲一覧だけ。src\layout\visibility.ts の TRASH_LISTS） */
  list: TrackListKind;
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
          // v3.7.0：消えるものを事実どおりに足した（お気に入りの印・歌詞・時刻の補正・年間まとめの記録も、曲と一緒に DB から消える。README「曲をごみ箱へ移す」「年間まとめ」）
          `${ids.length}曲をライブラリから外します。\n音声ファイルそのものは消しません。プレイリストとキューからも外れます。\nお気に入りの印・この曲の歌詞（手で入れたもの・取り込んだもの）と時刻の補正・年間まとめの再生の記録も消えます（取り込み直しても戻りません）。`,
          "外す",
          true,
        );
        if (ok) await run(api.libraryRemoveTracks(ids));
      },
    },
  );
  // v3.3.0：ライブラリの曲一覧なら、設定にかかわらずいつも「ライブラリから外す…」と並べて出す（作者の決定。確認は trashTracksFlow が必ず出す）。
  // 設定（ui.trashButton）が決めるのは行の右端のボタンだけ
  if (trashMenuShown(opts.list)) {
    items.push({
      label: ids.length > 1 ? `${ids.length}曲をごみ箱へ移す…` : "ごみ箱へ移す…",
      icon: <IconTrash />,
      danger: true,
      onClick: () => void trashTracksFlow(opts.tracks),
    });
  }
  return items;
}
