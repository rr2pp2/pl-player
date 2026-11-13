// すべての曲・最近追加・最近再生・お気に入り、と、アーティスト／アルバム／ジャンルを開いたときの曲一覧。
import { useEffect, useState } from "react";
import { api } from "../ipc/commands";
import type { Track } from "../ipc/types";
import { libraryVersion, route, search, sort, type Route } from "../stores/library";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { TrackTable } from "../components/TrackTable";
import { Artwork } from "../components/Artwork";
import { dropState, IMAGE_RE, registerDropZone } from "../components/FileDrop";
import { totalDuration } from "../components/format";
import { IconFilePlus, IconFolderPlus, IconImage, IconPlay, IconShuffle, IconX } from "../components/Icons";

const TITLES: Record<string, string> = {
  all: "すべての曲",
  recent_added: "最近追加",
  recent_played: "最近再生",
  favorites: "お気に入り",
  artist: "アーティスト",
  album: "アルバム",
  genre: "ジャンル",
};

export function useTracks(r: Route) {
  const q = useStore(search);
  const so = useStore(sort);
  const ver = useStore(libraryVersion);
  const [tracks, setTracks] = useState<Track[] | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .libraryQuery({ view: r.view, key: r.key ?? null, search: q || null, sort: so.by, desc: so.desc })
      .then((t) => alive && setTracks(t))
      .catch((e) => {
        if (alive) setTracks([]);
        notify({ level: "error", error: e });
      });
    return () => {
      alive = false;
    };
  }, [r.view, r.key, q, so.by, so.desc, ver]);
  return tracks;
}

const ALBUM_ZONE = "album-header-art";

function AlbumHeaderArt(props: { albumKey: string; tracks: Track[] }) {
  const drop = useStore(dropState);
  const art = props.tracks.find((t) => t.artPath)?.artPath ?? null;
  const set = async (path: string) => {
    try {
      await api.albumArtSetExternal(props.albumKey, path);
      notify({ level: "info", text: "アルバムの画像を設定しました（音声ファイルは変更していません）" });
    } catch (e) {
      notify({ level: "error", error: e });
    }
  };
  useEffect(
    () =>
      registerDropZone(ALBUM_ZONE, (paths) => {
        const img = paths.find((p) => IMAGE_RE.test(p));
        if (img) void set(img);
        else notify({ level: "error", text: "画像ファイル（PNG・JPG・WebP）をドロップしてください。" });
      }),
    [props.albumKey],
  );
  return (
    <div className="row" style={{ alignItems: "flex-end" }}>
      <Artwork path={art} className="album-head-art" dropTarget={ALBUM_ZONE} dropActive={drop.active && drop.zone === ALBUM_ZONE} />
      <div className="row" style={{ flexDirection: "column", alignItems: "flex-start", gap: 6 }}>
        <button
          className="btn"
          onClick={async () => {
            const p = await api.pickImage("アルバムの画像を選ぶ");
            if (p) void set(p);
          }}
          title="画像をアートの枠へドラッグ＆ドロップしても設定できます"
        >
          <IconImage /> 画像を設定…
        </button>
        <button
          className="btn"
          onClick={() => api.albumArtClearExternal(props.albumKey).catch((e) => notify({ level: "error", error: e }))}
          title="外部画像の設定を外して、埋め込み画像に戻す"
        >
          <IconX /> 外部画像を外す
        </button>
      </div>
    </div>
  );
}

export function LibraryView() {
  const r = useStore(route);
  const tracks = useTracks(r);
  const ids = tracks?.map((t) => t.id) ?? [];
  const play = (i: number) => void api.playContext(ids, i).catch((e) => notify({ level: "error", error: e }));
  const shuffle = async () => {
    if (!ids.length) return;
    await api.setShuffle(true);
    await api.playContext(ids, Math.floor(Math.random() * ids.length));
  };
  const title = r.title ?? TITLES[r.view] ?? "";
  const total = tracks?.reduce((a, t) => a + t.durationMs, 0) ?? 0;
  const q = useStore(search);
  return (
    <div className="content" key={`${r.view}:${r.key ?? ""}`}>
      <div className="view-head">
        {r.view === "album" && r.key !== undefined && r.key !== null && tracks ? (
          <AlbumHeaderArt albumKey={r.key} tracks={tracks} />
        ) : null}
        <div style={{ minWidth: 0 }}>
          <h1 className="view-title">{title || "（名前なし）"}</h1>
          <div className="view-sub">
            {tracks ? `${tracks.length}曲・${totalDuration(total)}` : "読み込み中…"}
            {q ? `（「${q}」で絞り込み中）` : ""}
          </div>
        </div>
        <div className="view-actions">
          <button className="btn primary" disabled={!ids.length} onClick={() => play(0)}>
            <IconPlay /> 再生
          </button>
          <button className="btn" disabled={!ids.length} onClick={shuffle}>
            <IconShuffle /> シャッフル再生
          </button>
        </div>
      </div>
      {tracks ? (
        <TrackTable
          tracks={tracks}
          list="library"
          onPlay={play}
          empty={
            q ? (
              <span>「{q}」に当たる曲はありません</span>
            ) : r.view === "all" ? (
              <>
                <div>まだ曲がありません。</div>
                <div>ファイルやフォルダをここへドラッグ＆ドロップするか、次のボタンから取り込んでください。</div>
                <div className="row">
                  <button className="btn" onClick={() => void api.libraryPickFiles()}>
                    <IconFilePlus /> ファイルを選ぶ
                  </button>
                  <button className="btn primary" onClick={() => void api.libraryPickFolder()}>
                    <IconFolderPlus /> フォルダを追加
                  </button>
                </div>
              </>
            ) : (
              <span>曲がありません</span>
            )
          }
        />
      ) : (
        <div className="empty">読み込み中…</div>
      )}
    </div>
  );
}
