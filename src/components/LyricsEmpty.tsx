// 歌詞の空の状態（歌詞なし・読み込み失敗・曲なし）。
import { lyricsWarning } from "../messages/ja";
import { importLyricsInbox, openLyricsEditor } from "../stores/lyrics";
import { fetchLyricsForTrack, lyricsFetch, lyricsFetchOne, lyricsFetchRunning } from "../stores/lyricsFetch";
import { lyricsOnlineOn, settings } from "../stores/settings";
import { useStore } from "../stores/store";

export function LyricsNone(props: { warnings: string[]; trackId: number }) {
  const s = useStore(settings);
  const batch = useStore(lyricsFetch);
  const one = useStore(lyricsFetchOne);
  // v3.1.0：「歌詞をネットから取る」がオンのときだけ出す（オフのときは Rust 側も入口で断る）
  const online = lyricsOnlineOn(s);
  const searching = one === props.trackId;
  return (
    <div className="empty lyrics-empty">
      <div>この曲の歌詞はありません</div>
      {props.warnings.map((w) => (
        <div className="hint-text" key={w}>
          {lyricsWarning(w)}
        </div>
      ))}
      <div className="row" style={{ justifyContent: "center" }}>
        <button className="btn primary" onClick={openLyricsEditor}>
          歌詞を貼り付ける
        </button>
        <button className="btn" onClick={() => void importLyricsInbox()}>
          外部歌詞を取り込む
        </button>
        {online ? (
          <button
            className="btn"
            disabled={one !== null || lyricsFetchRunning(batch)}
            aria-busy={searching}
            title="曲のタグの曲名・アーティスト名・アルバム名と曲の長さを lrclib.net へ送って探します（ファイル名・ファイルの場所は送りません）"
            onClick={() => void fetchLyricsForTrack(props.trackId)}
          >
            {searching ? "探しています…" : "ネットから探す"}
          </button>
        ) : null}
      </div>
      {online && lyricsFetchRunning(batch) ? <div className="hint-text">設定画面で歌詞を取ってきている途中です。終わってから押せます。</div> : null}
    </div>
  );
}

export function LyricsFailed(props: { text: string; detail: string; onRetry: () => void }) {
  return (
    <div className="empty lyrics-empty" role="alert">
      <div>歌詞を読み込めませんでした</div>
      <div className="error-text">{props.text}</div>
      {props.detail ? <div className="path-text">{props.detail}</div> : null}
      <div className="row" style={{ justifyContent: "center" }}>
        <button className="btn primary" onClick={props.onRetry}>
          再試行
        </button>
        <button className="btn" onClick={openLyricsEditor}>
          原文を編集
        </button>
      </div>
    </div>
  );
}

export function LyricsNoTrack() {
  return (
    <div className="empty lyrics-empty">
      <div>再生中の曲がありません</div>
      <div className="hint-text">曲を再生すると、ここに歌詞が出ます。</div>
    </div>
  );
}
