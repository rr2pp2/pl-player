// シンプルな線のアイコン（外部の素材は使わない。24x24、線幅 1.8）
// v3.2.0：着せ替え。スキンに画像があればそれを出し、無ければ（既定のスキンも）今の SVG をそのまま出す。
// 状態の切り替え点は src\skins\icons.ts（今の SVG と同じ）
import { useState } from "react";
import { useSkinIcon } from "../stores/skin";
import { repeatState, shuffleState, starState, volumeState } from "../skins/icons";

type P = { className?: string; title?: string };

/** スキンの画像か、今の SVG（children）。画像が読めなければ SVG に戻す */
function Skinned(props: { name: string; state?: string; p: P; children: any }) {
  const url = useSkinIcon(props.name, props.state ?? "default");
  const [bad, setBad] = useState<string | null>(null);
  if (!url || bad === url) return props.children;
  return (
    <img
      className={"icon skin-icon " + (props.p.className ?? "")}
      src={url}
      alt={props.p.title ?? ""}
      aria-hidden={props.p.title ? undefined : true}
      draggable={false}
      onError={() => setBad(url)}
    />
  );
}

const S = (props: P & { children: any; fill?: boolean }) => (
  <svg
    className={"icon " + (props.className ?? "")}
    viewBox="0 0 24 24"
    fill={props.fill ? "currentColor" : "none"}
    stroke={props.fill ? "none" : "currentColor"}
    strokeWidth={1.8}
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden={props.title ? undefined : true}
    role={props.title ? "img" : undefined}
  >
    {props.title ? <title>{props.title}</title> : null}
    {props.children}
  </svg>
);

export const IconMusic = (p: P) => (
  <Skinned name="IconMusic" p={p}>
    <S {...p}>
      <path d="M9 18V5l11-2v13" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="17" cy="16" r="3" />
    </S>
  </Skinned>
);
export const IconUser = (p: P) => (
  <Skinned name="IconUser" p={p}>
    <S {...p}>
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6" />
    </S>
  </Skinned>
);
export const IconDisc = (p: P) => (
  <Skinned name="IconDisc" p={p}>
    <S {...p}>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="2.5" />
    </S>
  </Skinned>
);
export const IconTag = (p: P) => (
  <Skinned name="IconTag" p={p}>
    <S {...p}>
      <path d="M3 12V4h8l10 10-8 8z" />
      <circle cx="7.5" cy="8.5" r="1.3" />
    </S>
  </Skinned>
);
export const IconClockPlus = (p: P) => (
  <Skinned name="IconClockPlus" p={p}>
    <S {...p}>
      <circle cx="11" cy="12" r="8" />
      <path d="M11 8v4l3 2" />
      <path d="M19 3v4M17 5h4" />
    </S>
  </Skinned>
);
export const IconHistory = (p: P) => (
  <Skinned name="IconHistory" p={p}>
    <S {...p}>
      <path d="M3 12a9 9 0 1 0 3-6.7" />
      <path d="M3 4v4h4" />
      <path d="M12 8v4l3 2" />
    </S>
  </Skinned>
);
/** 年間まとめ（v3.6.0）：表彰台（真ん中が1位）。再生中の印（IconBars の塗った棒）と見分けるため、線で描く */
export const IconRanking = (p: P) => (
  <Skinned name="IconRanking" p={p}>
    <S {...p}>
      <path d="M9 20V8h6v12" />
      <path d="M3 20v-7h6M15 20v-4h6v4" />
      <path d="M2 20h20" />
      <path d="M11.5 11l1-1v4" />
    </S>
  </Skinned>
);
export const IconStar = (p: P & { filled?: boolean }) => (
  <Skinned name="IconStar" state={starState(p.filled)} p={p}>
    <S {...p} fill={p.filled}>
      <path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" />
    </S>
  </Skinned>
);
export const IconList = (p: P) => (
  <Skinned name="IconList" p={p}>
    <S {...p}>
      <path d="M8 6h13M8 12h13M8 18h9" />
      <circle cx="4" cy="6" r="1" />
      <circle cx="4" cy="12" r="1" />
      <circle cx="4" cy="18" r="1" />
    </S>
  </Skinned>
);
export const IconQueue = (p: P) => (
  <Skinned name="IconQueue" p={p}>
    <S {...p}>
      <path d="M3 6h12M3 12h12M3 18h7" />
      <path d="M17 14v7l5-3.5z" />
    </S>
  </Skinned>
);
export const IconSettings = (p: P) => (
  <Skinned name="IconSettings" p={p}>
    <S {...p}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 0 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 0 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 0 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 0 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
    </S>
  </Skinned>
);
export const IconSearch = (p: P) => (
  <Skinned name="IconSearch" p={p}>
    <S {...p}>
      <circle cx="11" cy="11" r="7" />
      <path d="M21 21l-4.3-4.3" />
    </S>
  </Skinned>
);
export const IconPlus = (p: P) => (
  <Skinned name="IconPlus" p={p}>
    <S {...p}>
      <path d="M12 5v14M5 12h14" />
    </S>
  </Skinned>
);
export const IconFolderPlus = (p: P) => (
  <Skinned name="IconFolderPlus" p={p}>
    <S {...p}>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      <path d="M12 11v5M9.5 13.5h5" />
    </S>
  </Skinned>
);
export const IconFilePlus = (p: P) => (
  <Skinned name="IconFilePlus" p={p}>
    <S {...p}>
      <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <path d="M14 3v6h6M12 12v6M9 15h6" />
    </S>
  </Skinned>
);
export const IconPlay = (p: P) => (
  <Skinned name="IconPlay" p={p}>
    <S {...p} fill>
      <path d="M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.7l-12-7.5A1 1 0 0 0 7 4.5z" />
    </S>
  </Skinned>
);
export const IconPause = (p: P) => (
  <Skinned name="IconPause" p={p}>
    <S {...p} fill>
      <rect x="6" y="4.5" width="4.2" height="15" rx="1.2" />
      <rect x="13.8" y="4.5" width="4.2" height="15" rx="1.2" />
    </S>
  </Skinned>
);
export const IconNext = (p: P) => (
  <Skinned name="IconNext" p={p}>
    <S {...p} fill>
      <path d="M5 5.5v13a1 1 0 0 0 1.6.8l9-6.5a1 1 0 0 0 0-1.6l-9-6.5A1 1 0 0 0 5 5.5z" />
      <rect x="17" y="5" width="2.4" height="14" rx="1" />
    </S>
  </Skinned>
);
export const IconPrev = (p: P) => (
  <Skinned name="IconPrev" p={p}>
    <S {...p} fill>
      <path d="M19 5.5v13a1 1 0 0 1-1.6.8l-9-6.5a1 1 0 0 1 0-1.6l9-6.5A1 1 0 0 1 19 5.5z" />
      <rect x="4.6" y="5" width="2.4" height="14" rx="1" />
    </S>
  </Skinned>
);
export const IconShuffle = (p: P & { on?: boolean }) => (
  <Skinned name="IconShuffle" state={shuffleState(p.on)} p={p}>
    <S {...p}>
      <path d="M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5" />
    </S>
  </Skinned>
);
export const IconRepeat = (p: P & { one?: boolean; mode?: string }) => (
  <Skinned name="IconRepeat" state={repeatState(p.mode, p.one)} p={p}>
    <S {...p}>
      <path d="M17 1l4 4-4 4" />
      <path d="M3 11V9a4 4 0 0 1 4-4h14" />
      <path d="M7 23l-4-4 4-4" />
      <path d="M21 13v2a4 4 0 0 1-4 4H3" />
      {p.one ? <path d="M11.5 10.5 13 9.5v5" strokeWidth={1.6} /> : null}
    </S>
  </Skinned>
);
export const IconVolume = (p: P & { level?: number; muted?: boolean }) => (
  <Skinned name="IconVolume" state={volumeState(p.muted, p.level)} p={p}>
    <S {...p}>
      <path d="M11 5 6 9H2v6h4l5 4z" />
      {p.muted ? (
        <path d="M22 9l-6 6M16 9l6 6" />
      ) : (
        <>
          {(p.level ?? 1) > 0.01 ? <path d="M15.5 8.5a5 5 0 0 1 0 7" /> : null}
          {(p.level ?? 1) > 0.5 ? <path d="M19 5a10 10 0 0 1 0 14" /> : null}
        </>
      )}
    </S>
  </Skinned>
);
export const IconX = (p: P) => (
  <Skinned name="IconX" p={p}>
    <S {...p}>
      <path d="M18 6 6 18M6 6l12 12" />
    </S>
  </Skinned>
);
export const IconTrash = (p: P) => (
  <Skinned name="IconTrash" p={p}>
    <S {...p}>
      <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6" />
    </S>
  </Skinned>
);
export const IconEdit = (p: P) => (
  <Skinned name="IconEdit" p={p}>
    <S {...p}>
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.1 2.1 0 1 1 3 3L7 19l-4 1 1-4z" />
    </S>
  </Skinned>
);
export const IconImage = (p: P) => (
  <Skinned name="IconImage" p={p}>
    <S {...p}>
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <circle cx="8.5" cy="8.5" r="1.5" />
      <path d="M21 15l-5-5L5 21" />
    </S>
  </Skinned>
);
export const IconSidebar = (p: P) => (
  <Skinned name="IconSidebar" p={p}>
    <S {...p}>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M9 4v16" />
    </S>
  </Skinned>
);
export const IconRefresh = (p: P) => (
  <Skinned name="IconRefresh" p={p}>
    <S {...p}>
      <path d="M21 12a9 9 0 1 1-2.6-6.4L21 8" />
      <path d="M21 3v5h-5" />
    </S>
  </Skinned>
);
export const IconFolder = (p: P) => (
  <Skinned name="IconFolder" p={p}>
    <S {...p}>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </S>
  </Skinned>
);
export const IconChevron = (p: P) => (
  <Skinned name="IconChevron" p={p}>
    <S {...p}>
      <path d="M9 6l6 6-6 6" />
    </S>
  </Skinned>
);
export const IconBars = (p: P) => (
  <Skinned name="IconBars" p={p}>
    <S {...p} fill>
      <rect x="4" y="10" width="3" height="10" rx="1" />
      <rect x="10.5" y="5" width="3" height="15" rx="1" />
      <rect x="17" y="13" width="3" height="7" rx="1" />
    </S>
  </Skinned>
);
/** 歌詞（v3）：文字の行と音符 */
export const IconLyrics = (p: P) => (
  <Skinned name="IconLyrics" p={p}>
    <S {...p}>
      <path d="M4 6h11M4 11h11M4 16h6" />
      <path d="M19 17V8l2-1" />
      <circle cx="17" cy="17" r="2" />
    </S>
  </Skinned>
);
/** 戻る（v3：歌詞画面） */
export const IconBack = (p: P) => (
  <Skinned name="IconBack" p={p}>
    <S {...p}>
      <path d="M15 6l-6 6 6 6" />
    </S>
  </Skinned>
);
/** 流れる歌詞の列（v3.0.2）：左へ流れる矢印と、言葉の並ぶ帯 */
export const IconLyricsRow = (p: P) => (
  <Skinned name="IconLyricsRow" p={p}>
    <S {...p}>
      <path d="M8 4 5 7l3 3" />
      <path d="M5 7h14" />
      <rect x="3" y="13" width="18" height="7" rx="2" />
      <path d="M7 16.5h4M14 16.5h3" />
    </S>
  </Skinned>
);

// v3.8.0：上の帯のモード切替と、再生バーの「···」。着せ替えでは差し替えない（スキンの ICONS には足さない）。
// どれもボタンの枠（.seg .view-mode button・.icon-btn）の大きさが決まっているので、形を変えても並びは崩れない
/** 通常の表示：ジャケットの四角と2段の行 */
export const IconViewNormal = (p: P) => (
  <S {...p}>
    <rect x="3" y="4" width="6" height="6" rx="1.5" />
    <rect x="3" y="14" width="6" height="6" rx="1.5" />
    <path d="M12 6h9M12 8.5h6M12 16h9M12 18.5h6" />
  </S>
);
/** コンパクト表示：詰めた行 */
export const IconViewCompact = (p: P) => (
  <S {...p}>
    <path d="M3 5h18M3 9.67h18M3 14.33h18M3 19h18" />
  </S>
);
/** 詳細の表示（v3.8.0）：ミキサーのつまみ3本 */
export const IconViewDetail = (p: P) => (
  <S {...p}>
    <path d="M6 4v16M12 4v16M18 4v16" />
    <path d="M4 15h4M10 8h4M16 12h4" />
  </S>
);

// v3.8.0：出力先のアイコン（仕様 4.3。Windows が教える機器の種類から自動で決め、詳細の「…」で選び直せる）。
// 着せ替えでは差し替えない（スキンの ICONS には足さない）。どれも枠（.output-icon）の大きさが決まっているので、形を変えても並びは崩れない
/** ヘッドホン */
export const IconHeadphones = (p: P) => (
  <S {...p}>
    <path d="M4 15v-3a8 8 0 0 1 16 0v3" />
    <rect x="3" y="14" width="4.5" height="7" rx="1.5" />
    <rect x="16.5" y="14" width="4.5" height="7" rx="1.5" />
  </S>
);
/** スピーカー */
export const IconSpeaker = (p: P) => (
  <S {...p}>
    <rect x="5" y="2.5" width="14" height="19" rx="2.5" />
    <circle cx="12" cy="14.5" r="3.5" />
    <circle cx="12" cy="7" r="1.3" />
  </S>
);
/** 配信（仮想ケーブル）：電波 */
export const IconStream = (p: P) => (
  <S {...p}>
    <circle cx="12" cy="12" r="2" />
    <path d="M8.2 15.8a5.4 5.4 0 0 1 0-7.6M15.8 8.2a5.4 5.4 0 0 1 0 7.6" />
    <path d="M5.2 18.8a9.6 9.6 0 0 1 0-13.6M18.8 5.2a9.6 9.6 0 0 1 0 13.6" />
  </S>
);
/** モニター */
export const IconMonitor = (p: P) => (
  <S {...p}>
    <rect x="2.5" y="4" width="19" height="13" rx="2" />
    <path d="M9 21h6M12 17v4" />
  </S>
);
/** 出力先のアイコン（"muted" は消音。消音だけは IconVolume なので、着せ替えの画像があればそれを出す） */
export function OutputIconView(props: P & { icon: string }) {
  const { icon, ...p } = props;
  if (icon === "muted") return <IconVolume {...p} muted />;
  if (icon === "headphones") return <IconHeadphones {...p} />;
  if (icon === "stream") return <IconStream {...p} />;
  if (icon === "monitor") return <IconMonitor {...p} />;
  return <IconSpeaker {...p} />;
}
/** その他（···） */
export const IconMore = (p: P) => (
  <S {...p} fill>
    <circle cx="5" cy="12" r="1.8" />
    <circle cx="12" cy="12" r="1.8" />
    <circle cx="19" cy="12" r="1.8" />
  </S>
);
