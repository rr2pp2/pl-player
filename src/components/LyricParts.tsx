// 歌詞の文を、括弧の中（掛け合い・合いの手）と本体に分けて描く（v3.0.2）。
// 分け方は stores/lyricsTime.ts の splitParens・rangeParts（画面に依存しない関数）。見た目は base.css の .lyric-sub
import type { TextPart } from "../stores/lyricsTime";

export function LyricParts(props: { parts: TextPart[] }) {
  return (
    <>
      {props.parts.map((p, i) =>
        p.sub ? (
          <span key={i} className="lyric-sub">
            {p.text}
          </span>
        ) : (
          <span key={i}>{p.text}</span>
        ),
      )}
    </>
  );
}
