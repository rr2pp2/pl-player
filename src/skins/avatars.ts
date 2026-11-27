// 立ち絵（v3.9.0）：設定の「立ち絵」の欄に並べる行と、使う立ち絵の値。
// 並びは、頭に「スキンの立ち絵」「出さない」、続けて取り込んだスキンの立ち絵、登録した立ち絵。
// 値の形は Rust の skins::avatars::Choice と同じ（"skin"・"none"・"avatar:<id>"・"skin:<id>"）。正は Rust（形の確かめも Rust がする）。
// ほかに依存しない純粋な関数だけを置く（scripts\skins.test.mts から Node だけで確かめるため）。
import type { AvatarList, ErrorCode } from "../ipc/types";

export const CHOICE_SKIN = "skin";
export const CHOICE_NONE = "none";
export const avatarChoice = (id: string): string => `avatar:${id}`;
export const skinCharacterChoice = (skinId: string): string => `skin:${skinId}`;

/** 名前の長さ（Rust の skins::avatars::NAME_MAX と同じ。文字の決まりは Rust が確かめる） */
export const AVATAR_NAME_MAX = 40;

export type AvatarRowKind = "skin" | "none" | "skinCharacter" | "avatar";

export interface AvatarRow {
  /** 「使う」で送る値 */
  choice: string;
  kind: AvatarRowKind;
  /** 画面に出す名前（「スキンの立ち絵」「出さない」は決まった文） */
  name: string;
  /** 小さな見本（静止画のパス。Rust が確かめた場所だけ）。無ければ null */
  preview: string | null;
  /** 補足（どのスキンの絵か・停止中の絵が無い など） */
  note: string;
  /**
   * v3.12.0：カードに付ける小さな印（文字）。取り込んだスキンの立ち絵だけ「スキン」（削除のボタンが無く、スキンを削除すると消える。
   * 登録した立ち絵と見分ける）。ほかは null
   */
  badge: string | null;
  /** 登録した立ち絵の id（名前を変える・削除に使う） */
  avatarId: string | null;
  /** 読めない立ち絵（使えない。消すことだけできる） */
  broken: ErrorCode | null;
}

/** v3.12.0：取り込んだスキンの立ち絵のカードの印 */
export const SKIN_CHARACTER_BADGE = "スキン";

/**
 * v3.12.0：取り込んだスキンの立ち絵のカードの説明。どのスキンの絵か・スキンを替えても変わらないこと・削除のボタンが無い理由
 * （スキンの中の絵を指すだけなので、スキンを削除すると一緒に消える）を書く。カードでは省略記号で切らずに折り返す
 */
export function skinCharacterNote(skinName: string): string {
  return `スキン「${skinName}」の立ち絵。スキンを替えてもこの絵のままです。スキンを削除すると消えます`;
}

/**
 * カードの絵の枠に、絵が無いときに出す文字（v3.12.0）。「出さない」と、立ち絵の無いスキンを当てているときの「スキンの立ち絵」は「なし」
 * （出す絵が無いことを同じ見え方で示す）。ほかは空（絵が読めないときも文字を出さない）
 */
export function avatarPreviewText(kind: AvatarRowKind): string {
  return kind === "none" || kind === "skin" ? "なし" : "";
}

/** カードの読み上げの名前（v3.12.0：取り込んだスキンの立ち絵は、スキンそのものと取り違えないよう「スキン『◯◯』の立ち絵」と読む） */
export function avatarCardLabel(row: Pick<AvatarRow, "kind" | "name" | "broken">, active: boolean): string {
  if (row.broken) return "読めない立ち絵";
  const name = row.kind === "skinCharacter" ? `スキン「${row.name}」の立ち絵` : row.name;
  return `${name}${active ? "（使用中）" : "を使う"}`;
}

/**
 * 一覧の行を作る。`skinName` は当てているスキンの名前（既定のスキンなら null）、`skinHasCharacter` はそのスキンに立ち絵があるか。
 * 取り込んだスキンの立ち絵は、当てているスキンのものも並べる（「skin:<id>」を選ぶと、スキンを替えてもその絵のまま）
 */
export function avatarRows(list: AvatarList | null, skinName: string | null, skinHasCharacter: boolean): AvatarRow[] {
  const rows: AvatarRow[] = [
    {
      choice: CHOICE_SKIN,
      kind: "skin",
      name: "スキンの立ち絵",
      preview: null,
      note: skinHasCharacter ? `いまは「${skinName ?? "既定のスキン"}」の立ち絵` : "当てているスキンに立ち絵があれば出します（いまは無し）",
      badge: null,
      avatarId: null,
      broken: null,
    },
    { choice: CHOICE_NONE, kind: "none", name: "出さない", preview: null, note: "立ち絵を出しません", badge: null, avatarId: null, broken: null },
  ];
  for (const s of list?.skins ?? []) {
    rows.push({
      choice: skinCharacterChoice(s.skinId),
      kind: "skinCharacter",
      name: s.name,
      preview: s.preview,
      note: skinCharacterNote(s.name),
      badge: SKIN_CHARACTER_BADGE,
      avatarId: null,
      broken: null,
    });
  }
  for (const a of list?.avatars ?? []) {
    rows.push({
      choice: avatarChoice(a.id),
      kind: "avatar",
      name: a.broken ? "（読めない立ち絵）" : a.name,
      preview: a.broken ? null : a.preview,
      note: a.broken ? "" : a.hasStopped ? "登録した立ち絵" : "登録した立ち絵（停止中も同じ絵）",
      badge: null,
      avatarId: a.id,
      broken: a.broken,
    });
  }
  return rows;
}

/** 「＋ 立ち絵を追加…」を押せるか（30 体まで。読めない立ち絵も数える） */
export function canCreateAvatar(list: AvatarList | null): boolean {
  if (!list) return false;
  return list.avatars.length < list.max;
}

/** 名前の入力の手前の確かめ（前後の空白を外して 1〜40 文字。文字の決まりは Rust が確かめて、断るときは理由を返す） */
export function avatarNameReady(name: string): boolean {
  const n = [...name.trim()].length;
  return n >= 1 && n <= AVATAR_NAME_MAX;
}

/**
 * 「追加」「保存」を押せる名前か（v3.9.0）。空のままでもよい（追加では再生中の絵のファイル名から作った名前、
 * 変更では今の名前を Rust が使う）。入れたなら 1〜40 文字
 */
export function avatarNameOk(name: string): boolean {
  return name.trim() === "" || avatarNameReady(name);
}

/**
 * カードを押したときに、その立ち絵へ切り替えるか（v3.9.0）。カードの中のボタン（使う・変更・削除）から来たクリックは
 * ボタン自身が処理するので切り替えない（二重に走らせない）。読めない立ち絵・使っている立ち絵では何もしない
 */
export function cardClickSwitches(row: Pick<AvatarRow, "broken">, active: boolean, fromButton: boolean): boolean {
  return !fromButton && !row.broken && !active;
}

/** キーボードで選ぶ（Enter・Space）。カード自身にフォーカスがあるときだけ（中のボタンでの Enter・Space はボタンが処理する） */
export function cardKeySwitches(key: string, row: Pick<AvatarRow, "broken">, active: boolean, fromCardItself: boolean): boolean {
  return (key === "Enter" || key === " ") && fromCardItself && !row.broken && !active;
}
