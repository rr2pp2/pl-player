// アーティスト・アルバム・ジャンルの一覧。押すと、その曲一覧を開く。
import { useEffect, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";
import { api } from "../ipc/commands";
import type { Group } from "../ipc/types";
import { libraryVersion, navigate, route, search } from "../stores/library";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { Artwork } from "../components/Artwork";
import { IconTag, IconUser } from "../components/Icons";

const TITLE: Record<string, string> = { artist: "アーティスト", album: "アルバム", genre: "ジャンル" };

export function GroupView() {
  const r = useStore(route);
  const q = useStore(search);
  const ver = useStore(libraryVersion);
  const [groups, setGroups] = useState<Group[] | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .libraryGroups(r.view, q || null)
      .then((g) => alive && setGroups(g))
      .catch((e) => {
        if (alive) setGroups([]);
        notify({ level: "error", error: e });
      });
    return () => {
      alive = false;
    };
  }, [r.view, q, ver]);

  const open = (g: Group) => navigate({ view: r.view, key: g.key, title: g.name || unknownName(r.view) });

  return (
    <div className="content" key={r.view}>
      <div className="view-head">
        <div>
          <h1 className="view-title">{TITLE[r.view]}</h1>
          <div className="view-sub">{groups ? `${groups.length}件` : "読み込み中…"}</div>
        </div>
      </div>
      {!groups ? (
        <div className="empty">読み込み中…</div>
      ) : groups.length === 0 ? (
        <div className="empty">{q ? `「${q}」に当たるものはありません` : "まだ曲がありません"}</div>
      ) : r.view === "album" ? (
        <div className="group-grid">
          {groups.map((g) => (
            <button key={g.key} className="group-card" onClick={() => open(g)} title={g.name}>
              <Artwork path={g.artPath} />
              <div style={{ minWidth: 0 }}>
                <div className="group-name">{g.name || "不明なアルバム"}</div>
                <div className="group-sub">
                  {g.sub || "不明なアーティスト"}・{g.count}曲
                </div>
              </div>
            </button>
          ))}
        </div>
      ) : (
        <GroupList groups={groups} kind={r.view} onOpen={open} />
      )}
    </div>
  );
}

function unknownName(kind: string) {
  return kind === "artist" ? "不明なアーティスト" : kind === "genre" ? "ジャンルなし" : "不明なアルバム";
}

function GroupList(props: { groups: Group[]; kind: string; onOpen: (g: Group) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: props.groups.length, getScrollElement: () => ref.current, estimateSize: () => 52, overscan: 10 });
  return (
    <div className="group-list" ref={ref}>
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((it) => {
          const g = props.groups[it.index];
          return (
            <button
              key={g.key}
              className="group-row"
              style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${it.start}px)`, height: 48 }}
              onClick={() => props.onOpen(g)}
            >
              {props.kind === "artist" ? <IconUser className="nav-icon" /> : <IconTag className="nav-icon" />}
              <span className="group-name" style={{ flex: 1 }}>
                {g.name || unknownName(props.kind)}
              </span>
              <span className="group-sub">{g.count}曲</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
