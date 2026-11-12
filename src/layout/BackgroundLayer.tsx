// 層1（背景画像）と層2（覆い＋ぼかし）。層3の UI は AppShell。
// 背景は Cover が既定。暗さ・ぼかし・UI の透明度は設定値を CSS 変数で渡す。
import { useEffect, useState } from "react";
import { fileUrl } from "../ipc/events";
import { settings } from "../stores/settings";
import { useStore } from "../stores/store";

export function BackgroundLayer() {
  const s = useStore(settings);
  const bg = s.background;
  const [shown, setShown] = useState<string | null>(null);
  const [visible, setVisible] = useState(false);

  // テーマと変数
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = s.theme || "dark";
    root.style.setProperty("--bg-dim", String(bg.path ? bg.dim : 0));
    root.style.setProperty("--bg-blur", `${bg.blur}px`);
    root.style.setProperty("--ui-alpha", String(bg.uiOpacity));
  }, [s.theme, bg.path, bg.dim, bg.blur, bg.uiOpacity]);

  // 画像の切り替え（読み込めてから、ゆっくり出す）
  useEffect(() => {
    const url = fileUrl(bg.path);
    setVisible(false);
    if (!url) {
      const t = setTimeout(() => setShown(null), 250);
      return () => clearTimeout(t);
    }
    let alive = true;
    const img = new Image();
    img.onload = () => {
      if (!alive) return;
      setShown(url);
      requestAnimationFrame(() => alive && setVisible(true));
    };
    img.onerror = () => alive && setShown(null);
    img.src = url;
    return () => {
      alive = false;
    };
  }, [bg.path]);

  return (
    <div className="bg-layer" aria-hidden>
      {shown ? (
        <div
          className={"bg-image" + (bg.fit === "contain" ? " contain" : "") + (visible ? " visible" : "")}
          style={{ backgroundImage: `url("${shown}")` }}
        />
      ) : null}
      <div className="bg-overlay" />
    </div>
  );
}
