// 着せ替え（v3.2.0）：立ち絵。右側に専用の幅を取り（曲一覧などはその左で終わる）、下にそろえて出す。
// クリック・ドラッグ＆ドロップは下へ通す（pointer-events: none）。読み上げにも出さない（aria-hidden）。
// 出す・隠す・大きさの決まりは src\skins\character.ts。読み込めなければ隠す。v3.9.0：高さに合わせて出し、横長の左右は欄で切る。
// v3.3.0：再生中と停止中の2枚を読み込んだまま重ね、見せる方だけを切り替える（src を差し替えると、動く画像が頭に戻るため）。
// 一時的に隠すとき（歌詞画面・狭い幅）は、読み込んだまま見えなくする（characterStage）。
import { useState } from "react";
import { fileUrl } from "../ipc/events";
import { characterImageStyle, type CharacterStage } from "../skins/character";

export function CharacterLayer(props: { stage: CharacterStage }) {
  const st = props.stage;
  const [bad, setBad] = useState<Set<string>>(() => new Set());
  const frames = st.frames
    .map((f) => ({ ...f, url: fileUrl(f.src) }))
    .filter((f): f is typeof f & { url: string } => !!f.url && !bad.has(f.url));
  if (frames.length === 0) return null;
  return (
    <div className={"character-rail" + (st.layout ? "" : " parked")} aria-hidden="true">
      {frames.map((f) => (
        // key は画像の場所だけ（characterFrameKey）。再生・一時停止で key が変わらないので、要素も src もそのまま（動きの続きが途切れない）
        <img
          key={f.key}
          className={"character-img" + (f.shown ? "" : " waiting")}
          src={f.url}
          alt=""
          draggable={false}
          // v3.9.0：高さに合わせて出し、左右のはみ出しは欄で切る（characterImageStyle）
          style={characterImageStyle(st.maxHeightPx)}
          onError={() => setBad((b) => new Set(b).add(f.url))}
        />
      ))}
    </div>
  );
}
