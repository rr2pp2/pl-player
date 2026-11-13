// キュー：次に流れる曲。ドラッグで並び替え・削除・全削除。
import { useEffect, useState } from "react";
import { api } from "../ipc/commands";
import type { Track } from "../ipc/types";
import { libraryVersion, queueVersion } from "../stores/library";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { TrackTable } from "../components/TrackTable";
import { confirmDialog } from "../components/Modal";
import { totalDuration } from "../components/format";
import { IconPlay, IconTrash } from "../components/Icons";

export function QueueView() {
  const ver = useStore(queueVersion);
  const lver = useStore(libraryVersion);
  const [tracks, setTracks] = useState<Track[] | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .queueGet()
      .then((t) => alive && setTracks(t))
      .catch((e) => notify({ level: "error", error: e }));
    return () => {
      alive = false;
    };
  }, [ver, lver]);
  const list = tracks ?? [];
  const ids = list.map((_, i) => `q:${i}`);
  const remove = (idx: number[]) => api.queueRemove(idx).catch((e) => notify({ level: "error", error: e }));
  const total = list.reduce((a, t) => a + t.durationMs, 0);
  return (
    <div className="content">
      <div className="view-head">
        <div>
          <h1 className="view-title">キュー</h1>
          <div className="view-sub">
            {list.length}曲・{totalDuration(total)}（今の曲のあとに、上から順に流れます）
          </div>
        </div>
        <div className="view-actions">
          <button className="btn primary" disabled={!list.length} onClick={() => void api.queuePlayAt(0)}>
            <IconPlay /> 先頭から再生
          </button>
          <button
            className="btn danger"
            disabled={!list.length}
            onClick={async () => {
              if (await confirmDialog("キューを全削除", "キューの曲をすべて外します。", "全削除", true)) void api.queueClear();
            }}
          >
            <IconTrash /> 全削除
          </button>
        </div>
      </div>
      {tracks ? (
        <TrackTable
          tracks={list}
          list="queue"
          sortable
          sortableIds={ids}
          sortableList="queue"
          onPlay={(i) => void api.queuePlayAt(i)}
          onDeleteKey={remove}
          extraMenu={(idx) => [
            { label: "今すぐ再生", icon: <IconPlay />, onClick: () => void api.queuePlayAt(idx[0]) },
            { label: "キューから削除", icon: <IconTrash />, danger: true, onClick: () => void remove(idx) },
          ]}
          empty={<span>キューは空です。曲を右クリックして「次に再生」「キューに追加」を選ぶか、サイドバーの「キュー」へドラッグしてください。</span>}
        />
      ) : (
        <div className="empty">読み込み中…</div>
      )}
    </div>
  );
}
