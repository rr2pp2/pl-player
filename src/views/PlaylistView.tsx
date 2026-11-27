// プレイリスト：曲の一覧（ドラッグで並び替え）・再生・シャッフル・リピート・名前変更・削除・曲削除。
import { useEffect, useState } from "react";
import { api } from "../ipc/commands";
import type { PlaylistEntry } from "../ipc/types";
import { libraryVersion, playlistsVersion, route } from "../stores/library";
import { playback } from "../stores/playback";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { TrackTable } from "../components/TrackTable";
import { totalDuration } from "../components/format";
import { IconEdit, IconPlay, IconRepeat, IconShufflePlay, IconTrash } from "../components/Icons";
import { deletePlaylistFlow, renamePlaylistFlow } from "../layout/Sidebar";

export function PlaylistView() {
  const r = useStore(route);
  const id = Number(r.key);
  const ver = useStore(playlistsVersion);
  const lver = useStore(libraryVersion);
  const pb = useStore(playback);
  const [entries, setEntries] = useState<PlaylistEntry[] | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .playlistEntries(id)
      .then((e) => alive && setEntries(e))
      .catch((e) => {
        if (alive) setEntries([]);
        notify({ level: "error", error: e });
      });
    return () => {
      alive = false;
    };
  }, [id, ver, lver]);

  const tracks = entries?.map((e) => e.track) ?? [];
  const sortIds = entries?.map((e) => `pli:${e.itemId}`) ?? [];
  const remove = (idx: number[]) => {
    if (!entries) return;
    const items = idx.map((i) => entries[i].itemId);
    api.playlistRemoveItems(id, items).catch((e) => notify({ level: "error", error: e }));
  };
  const total = tracks.reduce((a, t) => a + t.durationMs, 0);
  const name = r.title ?? "プレイリスト";
  return (
    <div className="content" key={`pl:${id}`}>
      <div className="view-head">
        <div style={{ minWidth: 0 }}>
          <h1 className="view-title">{name}</h1>
          <div className="view-sub">{entries ? `${tracks.length}曲・${totalDuration(total)}` : "読み込み中…"}</div>
        </div>
        <div className="view-actions">
          <button className="btn primary" disabled={!tracks.length} onClick={() => void api.playlistPlay(id, 0, false)}>
            <IconPlay /> 再生
          </button>
          <button className="btn" disabled={!tracks.length} onClick={() => void api.playlistPlay(id, null, true)}>
            <IconShufflePlay /> シャッフル
          </button>
          <button
            className={"btn" + (pb.repeat !== "off" ? " primary" : "")}
            onClick={() => void api.setRepeat(pb.repeat === "off" ? "all" : "off")}
            title="リピート（全曲）の切り替え"
            aria-pressed={pb.repeat !== "off"}
          >
            <IconRepeat one={pb.repeat === "one"} mode={pb.repeat} /> リピート
          </button>
          <button className="btn" onClick={() => void renamePlaylistFlow(id, name)}>
            <IconEdit /> 名前を変更
          </button>
          <button className="btn danger" onClick={() => void deletePlaylistFlow(id, name)}>
            <IconTrash /> 削除
          </button>
        </div>
      </div>
      {entries ? (
        <TrackTable
          tracks={tracks}
          list="playlist"
          sortable
          sortableIds={sortIds}
          sortableList="playlist"
          onPlay={(i) => void api.playlistPlay(id, i, null)}
          onDeleteKey={remove}
          extraMenu={(idx) => [{ label: "プレイリストから削除", icon: <IconTrash />, danger: true, onClick: () => remove(idx) }]}
          empty={<span>曲一覧から曲をここ（サイドバーのプレイリスト名）へドラッグするか、右クリックの「プレイリストに追加」で足せます</span>}
        />
      ) : (
        <div className="empty">読み込み中…</div>
      )}
    </div>
  );
}
