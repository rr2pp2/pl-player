import { useState, useEffect } from "react";
import { fileUrl } from "../ipc/events";
import { IconMusic } from "./Icons";

/** アルバムアート。画像が無い・読めないときは音符の絵を出す */
export function Artwork(props: {
  path: string | null | undefined;
  className?: string;
  /** 画面全体のドロップ処理がこの印を見て、アートへの画像ドロップとして扱う */
  dropTarget?: string;
  dropActive?: boolean;
}) {
  const [broken, setBroken] = useState(false);
  useEffect(() => setBroken(false), [props.path]);
  // 画像はデータフォルダに中身の鍵（ハッシュ）の名前で置くので、差し替えるとパスも変わる
  const src = fileUrl(props.path);
  return (
    <div
      className={"art " + (props.className ?? "") + (props.dropActive ? " drop-target" : "")}
      data-drop={props.dropTarget}
    >
      {src && !broken ? (
        <img src={src} alt="" draggable={false} loading="lazy" decoding="async" onError={() => setBroken(true)} />
      ) : (
        <IconMusic />
      )}
    </div>
  );
}
