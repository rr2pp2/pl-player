// 年間まとめ（v3.6.0）：画面を開いた時点の今から 365 日さかのぼって、このプレイヤーで再生した回数の上位 100 曲。
// 数え方・期間・並びは Rust の db::history（yearly_top）が決める。画面は順位の順のまま出す（見出しで並べ替えない）。
// 一覧は曲一覧と同じ TrackTable（ダブルクリックで再生・右クリック・ドラッグ・お気に入り・ごみ箱は今までどおり）に、
// 順位と期間中の回数の列を足す（TrackTable の props.ranking）。検索欄の語で絞っても、順位は絞る前のまま。
// 記録は 3.6.0 から貯まるので、記録が無い・1年に満たないときは、その旨を短く出す（文は messages\ja.ts）。
import { useEffect, useMemo, useState } from "react";
import { api } from "../ipc/commands";
import type { YearlyTop } from "../ipc/types";
import { libraryVersion, search } from "../stores/library";
import { notify } from "../stores/notices";
import { useStore } from "../stores/store";
import { TrackTable } from "../components/TrackTable";
import { IconPlay, IconShuffle } from "../components/Icons";
import { yearlyEmptyText, yearlyPeriodText, yearlyStartNote } from "../messages/ja";

export function YearlyView() {
  const q = useStore(search);
  // ライブラリが変わったら（曲を外した・タグを直した など）引き直す。開くたびにも引き直す（期間の終わりは開いた時点の今）
  const ver = useStore(libraryVersion);
  const [top, setTop] = useState<YearlyTop | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let alive = true;
    api
      .libraryYearlyTop(q || null)
      .then((t) => {
        if (!alive) return;
        setTop(t);
        setFailed(false);
      })
      .catch((e) => {
        if (alive) setFailed(true);
        notify({ level: "error", error: e });
      });
    return () => {
      alive = false;
    };
  }, [q, ver]);
  const tracks = useMemo(() => top?.entries.map((e) => e.track) ?? [], [top]);
  const ranking = useMemo(() => top?.entries.map((e) => ({ rank: e.rank, plays: e.plays })) ?? [], [top]);
  const ids = tracks.map((t) => t.id);
  const play = (i: number) => void api.playContext(ids, i).catch((e) => notify({ level: "error", error: e }));
  const shuffle = async () => {
    if (!ids.length) return;
    await api.setShuffle(true);
    await api.playContext(ids, Math.floor(Math.random() * ids.length));
  };
  const note = top ? yearlyStartNote(top) : null;
  return (
    <div className="content">
      <div className="view-head">
        <div style={{ minWidth: 0 }}>
          <h1 className="view-title">年間まとめ</h1>
          <div className="view-sub">
            {top ? `${yearlyPeriodText(top)}・再生した回数の上位 ${top.ranked}曲` : failed ? "読み込めませんでした" : "読み込み中…"}
            {q ? `（「${q}」で絞り込み中）` : ""}
          </div>
          {note ? <div className="view-note">{note}</div> : null}
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
      {top ? (
        <TrackTable
          tracks={tracks}
          list="library"
          onPlay={play}
          ranking={ranking}
          fixedOrder
          empty={q && top.ranked > 0 ? <span>「{q}」に当たる曲はありません</span> : <span>{yearlyEmptyText}</span>}
        />
      ) : (
        <div className="empty">{failed ? "読み込めませんでした" : "読み込み中…"}</div>
      )}
    </div>
  );
}
