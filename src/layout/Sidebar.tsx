// 左：ライブラリ・プレイリスト。折りたたむとアイコンだけ。
// プレイリストとキューは、曲のドラッグの受け手にもなる。
import { useDroppable } from "@dnd-kit/core";
import { api } from "../ipc/commands";
import type { ViewKind } from "../ipc/types";
import { navigate, playlists, refreshPlaylists, route } from "../stores/library";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { openMenu } from "../components/ContextMenu";
import { confirmDialog, promptText } from "../components/Modal";
import {
  IconClockPlus,
  IconDisc,
  IconEdit,
  IconHistory,
  IconList,
  IconMusic,
  IconPlay,
  IconPlus,
  IconQueue,
  IconSettings,
  IconShuffle,
  IconSidebar,
  IconStar,
  IconTag,
  IconTrash,
  IconUser,
} from "../components/Icons";
import { playback } from "../stores/playback";

const LIB: { view: ViewKind; label: string; icon: any }[] = [
  { view: "all", label: "すべての曲", icon: <IconMusic /> },
  { view: "artist", label: "アーティスト", icon: <IconUser /> },
  { view: "album", label: "アルバム", icon: <IconDisc /> },
  { view: "genre", label: "ジャンル", icon: <IconTag /> },
  { view: "recent_added", label: "最近追加", icon: <IconClockPlus /> },
  { view: "recent_played", label: "最近再生", icon: <IconHistory /> },
  { view: "favorites", label: "お気に入り", icon: <IconStar /> },
];

function DropNav(props: { id: string; active: boolean; onClick: () => void; onContextMenu?: (e: any) => void; icon: any; label: string; count?: number; title?: string }) {
  const d = useDroppable({ id: props.id });
  return (
    <button
      ref={d.setNodeRef}
      className={"nav-item" + (props.active ? " active" : "") + (d.isOver ? " drop-over" : "")}
      onClick={props.onClick}
      onContextMenu={props.onContextMenu}
      title={props.title ?? props.label}
    >
      {props.icon}
      <span className="label">{props.label}</span>
      {props.count !== undefined ? <span className="count">{props.count}</span> : null}
    </button>
  );
}

export async function createPlaylistFlow() {
  const name = await promptText("新しいプレイリスト", "名前", "", "作成");
  if (!name) return;
  try {
    const id = await api.playlistCreate(name);
    await refreshPlaylists();
    navigate({ view: "playlist", key: String(id), title: name });
  } catch (e) {
    notify({ level: "error", error: e });
  }
}

export async function renamePlaylistFlow(id: number, current: string) {
  const name = await promptText("プレイリストの名前を変更", "名前", current, "変更");
  if (!name || name === current) return;
  try {
    await api.playlistRename(id, name);
    if (route.get().view === "playlist" && route.get().key === String(id)) navigate({ view: "playlist", key: String(id), title: name });
  } catch (e) {
    notify({ level: "error", error: e });
  }
}

export async function deletePlaylistFlow(id: number, name: string) {
  const ok = await confirmDialog("プレイリストを削除", `「${name}」を削除します。曲そのものはライブラリに残ります。`, "削除", true);
  if (!ok) return;
  try {
    await api.playlistDelete(id);
    if (route.get().view === "playlist" && route.get().key === String(id)) navigate({ view: "all" });
  } catch (e) {
    notify({ level: "error", error: e });
  }
}

export function Sidebar(props: { onToggle: () => void }) {
  const r = useStore(route);
  const pls = useStore(playlists);
  const pb = useStore(playback);
  return (
    <nav className="sidebar panel" aria-label="ライブラリとプレイリスト">
      <div className="sidebar-head">
        <button className="icon-btn" onClick={props.onToggle} title="サイドバーの表示を切り替える">
          <IconSidebar />
        </button>
        <span className="app-name">PL Player</span>
      </div>
      <div className="sidebar-scroll">
        <div className="section-label">
          <span>LIBRARY</span>
        </div>
        {LIB.map((l) => (
          <button
            key={l.view}
            className={"nav-item" + (r.view === l.view && !r.key ? " active" : "")}
            onClick={() => navigate({ view: l.view })}
            title={l.label}
          >
            {l.icon}
            <span className="label">{l.label}</span>
          </button>
        ))}
        <DropNav
          id="drop:queue"
          active={r.view === "queue"}
          onClick={() => navigate({ view: "queue" })}
          icon={<IconQueue />}
          label="キュー"
          count={pb.queueLen || undefined}
          title="キュー（曲をここへドラッグすると追加）"
        />
        <div className="section-label">
          <span>PLAYLISTS</span>
          <button className="icon-btn small" onClick={createPlaylistFlow} title="プレイリストを作成">
            <IconPlus />
          </button>
        </div>
        {pls.map((p) => (
          <DropNav
            key={p.id}
            id={`drop:playlist:${p.id}`}
            active={r.view === "playlist" && r.key === String(p.id)}
            onClick={() => navigate({ view: "playlist", key: String(p.id), title: p.name })}
            onContextMenu={(e) =>
              openMenu(e, [
                { label: "再生", icon: <IconPlay />, onClick: () => void api.playlistPlay(p.id, 0, false) },
                { label: "シャッフル再生", icon: <IconShuffle />, onClick: () => void api.playlistPlay(p.id, null, true) },
                { separator: true },
                { label: "名前を変更…", icon: <IconEdit />, onClick: () => void renamePlaylistFlow(p.id, p.name) },
                { label: "削除…", icon: <IconTrash />, danger: true, onClick: () => void deletePlaylistFlow(p.id, p.name) },
              ])
            }
            icon={<IconList />}
            label={p.name}
            count={p.count}
          />
        ))}
        {pls.length === 0 ? (
          <button className="nav-item" onClick={createPlaylistFlow} title="プレイリストを作成">
            <IconPlus />
            <span className="label" style={{ color: "var(--c-secondary)" }}>
              プレイリストを作成
            </span>
          </button>
        ) : null}
      </div>
      <button className={"nav-item" + (r.view === "settings" ? " active" : "")} onClick={() => navigate({ view: "settings" })} title="設定">
        <IconSettings />
        <span className="label">設定</span>
      </button>
    </nav>
  );
}
