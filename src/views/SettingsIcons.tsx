// 設定の「着せ替え」の中の「アイコン」の欄（v3.11.0）：今の見た目のアイコンを、実際の大きさで場所ごとに並べた見本の一覧。
// - 枠に PC から画像を落とすと、そのアイコンが替わる（受け手の印は data-drop="icon:<名前>:<状態>"。振り分けは src\components\FileDrop.tsx）。
//   枠を押す（キーボードでは Enter・Space）とファイルを選ぶ画面が出る。どちらも画面からパスは渡さない
// - 枠ごとの「戻す」・欄の下の「手で入れたアイコンを全部戻す」（確認あり）
// - 重なり順は 今の SVG < スキンのアイコン < 手で入れたアイコン（決まりは src\skins\icons.ts。見本は実物と同じ部品で描くので、同じ決まりで出る）
// - 枠の表と大きさは src\skins\iconSlots.ts（Rust の skins::icons と同じ枠）
import { useEffect } from "react";
import {
  IconBack,
  IconBars,
  IconChevron,
  IconClockPlus,
  IconDisc,
  IconEdit,
  IconFilePlus,
  IconFolder,
  IconFolderPlus,
  IconHistory,
  IconImage,
  IconList,
  IconLyrics,
  IconLyricsRow,
  IconMusic,
  IconNext,
  IconPause,
  IconPlay,
  IconPlus,
  IconPrev,
  IconQueue,
  IconRanking,
  IconRefresh,
  IconRepeat,
  IconSearch,
  IconSettings,
  IconShuffle,
  IconShufflePlay,
  IconSidebar,
  IconStar,
  IconTag,
  IconTrash,
  IconUser,
  IconVolume,
  IconX,
} from "../components/Icons";
import { dropState, registerDropZone } from "../components/FileDrop";
import { WrapText } from "../components/WrapText";
import { confirmDialog } from "../components/Modal";
import { sidebarIconPx } from "../layout/density";
import { hasManualIcon, ICON_GROUPS, ICON_SLOTS, iconZoneKey, isAppIcon, manualIconCount, slotPx, type IconSlot } from "../skins/iconSlots";
import { notify } from "../stores/notices";
import { settings } from "../stores/settings";
import { appIconPreviews, clearIcons, dropIcon, loadAppIconPreviews, pickIcon, resetIcon, skinState } from "../stores/skin";
import { useStore } from "../stores/store";

/** 状態の無いボタンのアイコン（名前 → 部品） */
const PLAIN: Record<string, (p: { className?: string }) => any> = {
  IconMusic,
  IconUser,
  IconDisc,
  IconTag,
  IconClockPlus,
  IconHistory,
  IconRanking,
  IconShufflePlay,
  IconList,
  IconQueue,
  IconSettings,
  IconSearch,
  IconPlus,
  IconFolderPlus,
  IconFilePlus,
  IconPlay,
  IconPause,
  IconNext,
  IconPrev,
  IconX,
  IconTrash,
  IconEdit,
  IconImage,
  IconSidebar,
  IconRefresh,
  IconFolder,
  IconChevron,
  IconBars,
  IconLyrics,
  IconBack,
  IconLyricsRow,
};

/** 見本：実物と同じ部品を、その状態で描く（手のアイコン → スキン → 今の SVG の順は部品が決める） */
function SlotArt(props: { slot: IconSlot }) {
  const { name, state } = props.slot;
  const previews = useStore(appIconPreviews);
  if (isAppIcon(name)) {
    const url = name === "AppWindow" ? previews?.window : previews?.tray;
    return url ? <img className="icon icon-slot-app" src={url} alt="" draggable={false} /> : <span className="icon icon-slot-app" />;
  }
  switch (name) {
    case "IconStar":
      return <IconStar filled={state === "filled"} />;
    case "IconVolume":
      return <IconVolume muted={state === "muted"} level={state === "zero" ? 0 : state === "low" ? 0.3 : 1} />;
    case "IconRepeat":
      return <IconRepeat mode={state} one={state === "one"} />;
    case "IconShuffle":
      return <IconShuffle on={state === "on"} />;
    default: {
      const C = PLAIN[name];
      return C ? <C /> : null;
    }
  }
}

function SlotCard(props: { slot: IconSlot; px: number; manual: boolean; safe: boolean; over: boolean }) {
  const { slot } = props;
  const key = iconZoneKey(slot.name, slot.state);
  const title = slot.stateLabel ? `${slot.label}（${slot.stateLabel}）` : slot.label;
  return (
    <div className={"icon-slot" + (props.over ? " drop-over" : "") + (props.manual ? " manual" : "")} data-drop={key} data-icon-slot={key}>
      <button
        className="icon-slot-frame"
        style={{ ["--slot-px" as any]: `${props.px}px` }}
        disabled={props.safe}
        aria-label={`${title}のアイコン。画像を選ぶ…（画像をここに落としても入れられます）`}
        title={props.safe ? undefined : "押して画像を選ぶか、画像をここに落とす"}
        onClick={() => void pickIcon(slot.name, slot.state)}
      >
        <SlotArt slot={slot} />
      </button>
      {/* 名前は切らずに折り返し、状態（入・切・オン・オフ・全曲・1曲など）は別の行に必ず出す（同じ名前の枠を見分けるため）。
          v3.11.1：「・」の後ろで折り返せるようにする（WrapText。折り返しの決まりは base.css） */}
      <span className="icon-slot-name">
        <span className="icon-slot-label">
          <WrapText text={slot.label} />
        </span>
        {slot.stateLabel ? <span className="icon-slot-state">{slot.stateLabel}</span> : null}
      </span>
      <button className="btn small-btn" disabled={props.safe || !props.manual} onClick={() => void resetIcon(slot.name, slot.state)} aria-label={`${title}のアイコンを戻す`}>
        戻す
      </button>
    </div>
  );
}

export function IconsField() {
  const s = useStore(settings);
  const st = useStore(skinState);
  const drop = useStore(dropState);
  const safe = !!st?.safeMode;
  const manual = s.skin?.icons ?? {};
  const count = manualIconCount(manual);
  const navPx = sidebarIconPx(s.ui);

  // 今のウィンドウ・トレイのアイコン（スキンを替えた・入れた・戻したときに読み直す）
  useEffect(() => {
    void loadAppIconPreviews();
  }, [st]);

  // 枠ごとの受け手。落とした位置だけを Rust へ渡す（パスは Rust が自分で控えたものを使う）
  useEffect(() => {
    const offs = ICON_SLOTS.map((slot) =>
      registerDropZone(iconZoneKey(slot.name, slot.state), (_paths, pos) => {
        if (safe) {
          notify({ level: "error", error: { code: "icon_safe_mode", detail: "" } });
          return;
        }
        void dropIcon(slot.name, slot.state, pos.x, pos.y);
      }),
    );
    return () => offs.forEach((f) => f());
  }, [safe]);

  const clearAll = async () => {
    const ok = await confirmDialog(
      "手で入れたアイコンを全部戻す",
      `手で入れたアイコン（${count} 個）を全部外します。どのアイコンも、スキンのアイコンか元のアイコンに戻ります（入れた画像はこの PC から消えます）。スキン・色・立ち絵は変わりません。元に戻せません。`,
      "全部戻す",
      true,
    );
    if (ok) await clearIcons();
  };

  return (
    <div className="icons-block">
      <h4 className="skin-subhead">アイコン</h4>
      <p className="hint-text">
        今のアイコンを、画面で出す大きさで並べています。PC から画像（PNG・WebP・GIF。1つ 15MB まで）を枠に落とすと、そのアイコンが替わります。枠を押して画像を選ぶこともできます。入れた画像はこの PC のデータのフォルダに写して置きます（元のファイルは消しても大丈夫です）。Dark・Light のどちらにも同じアイコンを使い、当てているスキンのアイコンより優先します（スキンを替えても残ります）。「既定の見た目に戻す」で、手で入れたアイコンも全部外れます。
      </p>
      {safe ? <div className="skin-callout warn">既定の見た目で動いている間（セーフモード）は、手で入れたアイコンも当てず、新しく入れることもできません。入れたアイコンは残っていて、セーフモードを抜けると戻ります。</div> : null}
      {ICON_GROUPS.map((g) => (
        <div key={g.id} className="icon-group" role="group" aria-label={`アイコン：${g.label}`}>
          <div className="icon-group-label">
            <WrapText text={g.label} />
          </div>
          <div className="icon-slots">
            {ICON_SLOTS.filter((x) => x.group === g.id).map((slot) => {
              const key = iconZoneKey(slot.name, slot.state);
              return (
                <SlotCard
                  key={key}
                  slot={slot}
                  px={slotPx(slot, navPx)}
                  manual={hasManualIcon(manual, slot.name, slot.state)}
                  safe={safe}
                  over={drop.active && drop.zone === key}
                />
              );
            })}
          </div>
          {g.id === "app" ? (
            <span className="hint">ウィンドウ・トレイのアイコンは動かない画像だけです。正方形でなければ、透明で埋めて正方形に整えます。exe・スタートメニュー・ショートカットのアイコンは変わりません。</span>
          ) : null}
        </div>
      ))}
      <div className="field">
        <span className="flabel">手で入れたアイコン</span>
        <span className="row">
          <button className="btn" disabled={count === 0} onClick={() => void clearAll()}>
            手で入れたアイコンを全部戻す
          </button>
        </span>
        <span className="hint">
          画像を落とせるのは、枠の上に1つだけです（複数のファイル・フォルダは受けません）。枠の外に落としたものは、今までどおり曲の取り込みになります。曲一覧・年間まとめの「シャッフル再生」とプレイリストの「シャッフル」のボタン、サイドバーのプレイリストの右クリックの「シャッフル再生」は、「曲一覧」の「シャッフル再生」の枠で替わります（再生バーの「シャッフル」の切・入の枠では替わりません）。設定のバックアップ（zip）には、どの枠に入れたかだけが入り、画像は入りません。
        </span>
      </div>
    </div>
  );
}
