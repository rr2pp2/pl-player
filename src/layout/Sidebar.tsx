// 左：ライブラリ・プレイリスト。折りたたむとアイコンだけ。
// プレイリストとキューは、曲のドラッグの受け手にもなる。
// v3.3.0：使う人が隠した項目は出さない（設定は隠せない。決まりは visibility.ts）。
// たたんでいるときは、項目にマウスを乗せる・キーボードで移ると項目名の吹き出しを出す（Tip）。
// ウィンドウが狭くて自動でたたんでいるときは、切り替えボタンを押せない見た目にし、理由の吹き出しを出す。
// v3.4.0：項目のアイコンの大きさは設定の ui.sidebarIconPx（--nav-icon。読み方は density.ts、当て方は base.css の .nav-item .icon）。
import { useDroppable } from "@dnd-kit/core";
import { api } from "../ipc/commands";
import type { ViewKind } from "../ipc/types";
import { navigate, playlists, refreshPlaylists, route } from "../stores/library";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { openMenu } from "../components/ContextMenu";
import { confirmDialog, promptText } from "../components/Modal";
import { Tip } from "../components/Tip";
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
  IconRanking,
  IconSettings,
  IconShufflePlay,
  IconSidebar,
  IconStar,
  IconTag,
  IconTrash,
  IconUser,
} from "../components/Icons";
import { playback } from "../stores/playback";
import { settings } from "../stores/settings";
import { sidebarItemShown, sidebarToggleInfo } from "./visibility";
import { sidebarIconPx } from "./density";

const LIB: { view: ViewKind; label: string; icon: any }[] = [
  { view: "all", label: "すべての曲", icon: <IconMusic /> },
  { view: "artist", label: "アーティスト", icon: <IconUser /> },
  { view: "album", label: "アルバム", icon: <IconDisc /> },
  { view: "genre", label: "ジャンル", icon: <IconTag /> },
  { view: "recent_added", label: "最近追加", icon: <IconClockPlus /> },
  { view: "recent_played", label: "最近再生", icon: <IconHistory /> },
  // v3.6.0：年間まとめ（src\views\YearlyView.tsx）。隠せる（visibility.ts の SIDEBAR_ITEMS）
  { view: "yearly_top", label: "年間まとめ", icon: <IconRanking /> },
  { view: "favorites", label: "お気に入り", icon: <IconStar /> },
];

/** たたんでいるときだけ、項目名の吹き出しを出す（広げているときは名前が見えるので、ブラウザの title だけ） */
function NavTip(props: { collapsed: boolean; text: string; children: any; inline?: boolean }) {
  return (
    <Tip text={props.collapsed ? props.text : null} delay={350} className={props.inline ? "inline" : ""}>
      {props.children}
    </Tip>
  );
}

function DropNav(props: {
  id: string;
  active: boolean;
  collapsed: boolean;
  onClick: () => void;
  onContextMenu?: (e: any) => void;
  icon: any;
  label: string;
  count?: number;
  title?: string;
}) {
  const d = useDroppable({ id: props.id });
  const text = props.title ?? props.label;
  return (
    <NavTip collapsed={props.collapsed} text={props.count !== undefined ? `${text}（${props.count}）` : text}>
      <button
        ref={d.setNodeRef}
        className={"nav-item" + (props.active ? " active" : "") + (d.isOver ? " drop-over" : "")}
        onClick={props.onClick}
        onContextMenu={props.onContextMenu}
        title={props.collapsed ? undefined : text}
        aria-label={props.label}
        aria-current={props.active ? "page" : undefined}
      >
        {props.icon}
        <span className="label">{props.label}</span>
        {props.count !== undefined ? <span className="count">{props.count}</span> : null}
      </button>
    </NavTip>
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

export function Sidebar(props: { onToggle: () => void; sidebarAuto: boolean; collapsed: boolean }) {
  const r = useStore(route);
  const pls = useStore(playlists);
  const pb = useStore(playback);
  const ui = useStore(settings).ui;
  const toggle = sidebarToggleInfo(props.sidebarAuto, props.collapsed);
  const c = props.collapsed;
  const lib = LIB.filter((l) => sidebarItemShown(ui, l.view));
  const showQueue = sidebarItemShown(ui, "queue");
  const showPlaylists = sidebarItemShown(ui, "playlists");
  return (
    <nav className="sidebar panel" aria-label="ライブラリとプレイリスト" style={{ ["--nav-icon" as any]: `${sidebarIconPx(ui)}px` }}>
      <div className="sidebar-head">
        {/* 狭いときは disabled にしない（disabled だとマウスもキーボードも届かず、理由の吹き出しを出せない）。
            aria-disabled で押せないことを伝え、押しても何もしない */}
        <Tip text={toggle.hint} delay={500} className="inline">
          <button
            className="icon-btn"
            onClick={toggle.disabled ? undefined : props.onToggle}
            aria-disabled={toggle.disabled || undefined}
            aria-label={toggle.label}
            aria-expanded={toggle.disabled ? undefined : !c}
            title={toggle.disabled ? undefined : toggle.label}
          >
            <IconSidebar />
          </button>
        </Tip>
        <span className="app-name">PL Player</span>
      </div>
      <div className="sidebar-scroll">
        {lib.length || showQueue ? (
          <div className="section-label">
            <span>LIBRARY</span>
          </div>
        ) : null}
        {lib.map((l) => (
          <NavTip key={l.view} collapsed={c} text={l.label}>
            <button
              className={"nav-item" + (r.view === l.view && !r.key ? " active" : "")}
              onClick={() => navigate({ view: l.view })}
              title={c ? undefined : l.label}
              aria-label={l.label}
              aria-current={r.view === l.view && !r.key ? "page" : undefined}
            >
              {l.icon}
              <span className="label">{l.label}</span>
            </button>
          </NavTip>
        ))}
        {showQueue ? (
          <DropNav
            id="drop:queue"
            active={r.view === "queue"}
            collapsed={c}
            onClick={() => navigate({ view: "queue" })}
            icon={<IconQueue />}
            label="キュー"
            count={pb.queueLen || undefined}
            title="キュー（曲をここへドラッグすると追加）"
          />
        ) : null}
        {showPlaylists ? (
          <div className="section-label">
            <span>PLAYLISTS</span>
            <NavTip collapsed={c} text="プレイリストを作成" inline>
              <button className="icon-btn small" onClick={createPlaylistFlow} title={c ? undefined : "プレイリストを作成"} aria-label="プレイリストを作成">
                <IconPlus />
              </button>
            </NavTip>
          </div>
        ) : null}
        {(showPlaylists ? pls : []).map((p) => (
          <DropNav
            key={p.id}
            id={`drop:playlist:${p.id}`}
            collapsed={c}
            active={r.view === "playlist" && r.key === String(p.id)}
            onClick={() => navigate({ view: "playlist", key: String(p.id), title: p.name })}
            onContextMenu={(e) =>
              openMenu(e, [
                { label: "再生", icon: <IconPlay />, onClick: () => void api.playlistPlay(p.id, 0, false) },
                { label: "シャッフル再生", icon: <IconShufflePlay />, onClick: () => void api.playlistPlay(p.id, null, true) },
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
        {showPlaylists && pls.length === 0 ? (
          <NavTip collapsed={c} text="プレイリストを作成">
            <button className="nav-item" onClick={createPlaylistFlow} title={c ? undefined : "プレイリストを作成"} aria-label="プレイリストを作成">
              <IconPlus />
              <span className="label" style={{ color: "var(--c-secondary)" }}>
                プレイリストを作成
              </span>
            </button>
          </NavTip>
        ) : null}
      </div>
      {/* 設定は隠せない（隠すと戻れなくなる） */}
      <NavTip collapsed={c} text="設定">
        <button
          className={"nav-item" + (r.view === "settings" ? " active" : "")}
          onClick={() => navigate({ view: "settings" })}
          title={c ? undefined : "設定"}
          aria-label="設定"
          aria-current={r.view === "settings" ? "page" : undefined}
        >
          <IconSettings />
          <span className="label">設定</span>
        </button>
      </NavTip>
    </nav>
  );
}
