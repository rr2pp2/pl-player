// 着せ替え（v3.2.0）：立ち絵。右側に専用の幅を取り（曲一覧などはその左で終わる）、下にそろえて出す。
// クリック・ドラッグ＆ドロップは下へ通す（pointer-events: none）。読み上げにも出さない（aria-hidden）。
// 出す・隠す・大きさの決まりは src\skins\character.ts。読み込めなければ隠す。
import { useState } from "react";
import { fileUrl } from "../ipc/events";
import type { CharacterLayout } from "../skins/character";

export function CharacterLayer(props: { layout: CharacterLayout }) {
  const l = props.layout;
  const url = fileUrl(l.src);
  const [bad, setBad] = useState<string | null>(null);
  if (!url || bad === url) return null;
  return (
    <div className="character-rail" aria-hidden="true">
      {/* 表示する状態の画像だけを読む（key で前の画像を捨てる） */}
      <img
        key={url}
        className="character-img"
        src={url}
        alt=""
        draggable={false}
        style={{ maxWidth: l.widthPx, maxHeight: `min(${l.maxHeightPx}px, 100%)` }}
        onError={() => setBad(url)}
      />
    </div>
  );
}
