// 歌詞の表示時刻と「今の行」の計算（v3）。ほかに依存しない純粋な関数だけを置く
// （scripts/lyrics-time.test.mts から Node だけで確かめられるようにするため）。
//
// 表示時刻の式は仕様 4.2 の max(0, 元の時刻 − offset + 曲ごとの補正)。
// Rust は timeMs として「元の時刻 − offset」を返し、0 未満でも切らない。0 にそろえるのはここだけ。
import type { LyricsLine } from "../ipc/types";

/** 表示時刻：max(0, timeMs + 曲ごとの補正) */
export const shownTime = (l: Pick<LyricsLine, "timeMs">, delayMs: number): number => Math.max(0, (l.timeMs ?? 0) + delayMs);

/** 強調する行の範囲（lines の添字、両端を含む）。同じ表示時刻の行はまとめて強調する */
export interface ActiveRange {
  start: number;
  end: number;
}

/** 時刻つきの行の数。時刻つきの行は先頭にまとまっている（時刻なしの行は末尾） */
function timedCount(lines: Pick<LyricsLine, "timeMs">[]): number {
  let n = 0;
  while (n < lines.length && lines[n].timeMs !== null) n++;
  return n;
}

/** 表示時刻が今の位置以下の、最後の時刻つきの行の添字（無ければ -1）。「消す」合図の行も数える */
function lastStarted(lines: Pick<LyricsLine, "timeMs">[], n: number, delayMs: number, posMs: number): number {
  // 表示時刻は単調に増える（max(0, …) は順を崩さない）ので二分探索でよい
  let lo = 0;
  let hi = n - 1;
  let idx = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (shownTime(lines[mid], delayMs) <= posMs) {
      idx = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return idx;
}

/** 表示する行か（「消す」合図と、本文が空白だけの行は表示しない） */
export const isShown = (l: Pick<LyricsLine, "kind" | "text">): boolean => l.kind === "lyric" && l.text.trim() !== "";

/** 「表示時刻が今の位置以下の、最後の行」。「消す」合図の後は強調なし */
export function findActive(lines: Pick<LyricsLine, "timeMs" | "kind">[], delayMs: number, posMs: number): ActiveRange | null {
  const n = timedCount(lines);
  const idx = lastStarted(lines, n, delayMs, posMs);
  if (idx < 0 || lines[idx].kind !== "lyric") return null;
  const t = shownTime(lines[idx], delayMs);
  let start = idx;
  while (start > 0 && shownTime(lines[start - 1], delayMs) === t && lines[start - 1].kind === "lyric") start--;
  return { start, end: idx };
}

/**
 * 「次の行」（再生バーで今の行の右に出す）。今の位置ではまだ始まっていない、最初の表示する行のまとまり。
 * - 時刻の順で、今の行（同じ時刻のまとまりなら、その最後の行）より後ろを見る。表示時刻は findActive と同じ式
 * - 「消す」合図と、本文が空白だけの行は飛ばす
 * - 次の行と同じ表示時刻の、続く表示する行もまとめて返す（今の行と同じく「 ／ 」でつないで出すため）
 * - 最後の行のとき・時刻つきの行が無いときは null。最初の行より前と「消す」合図の後（今の行が無いとき）も、次の行は返す
 */
export function findNext(lines: Pick<LyricsLine, "timeMs" | "kind" | "text">[], delayMs: number, posMs: number): ActiveRange | null {
  const n = timedCount(lines);
  let j = lastStarted(lines, n, delayMs, posMs) + 1;
  while (j < n && !isShown(lines[j])) j++;
  if (j >= n) return null;
  const t = shownTime(lines[j], delayMs);
  let end = j;
  for (let k = j + 1; k < n && shownTime(lines[k], delayMs) === t; k++) {
    if (isShown(lines[k])) end = k;
  }
  return { start: j, end };
}

// ---- 流れる歌詞の列（v3.0.2）。再生バーの下に、歌詞を横一列に並べる ----
//
// 並べる単位は「同じ表示時刻の、表示する行のまとまり」（再生バーの rangeText と同じく1つにまとめる）。
// 単位の一覧は歌詞か補正が変わったときだけ作る（O(行数)）。今の行・次の行が変わったときだけ並びを作り直し、
// 並べるのは今の単位の前後の数個だけ（何万行あっても、画面に置く要素の数は変わらない）。

/** 列の1つの表示単位（lines の添字、両端を含む。間に「消す」合図や空白だけの行が挟まることがある） */
export interface RowUnit {
  start: number;
  end: number;
  timeMs: number;
}

/** 歌い終わった単位を、今の単位の左に何個置くか（行が変わったとき、左へ流れて見えるようにするため） */
export const ROW_PAST = 3;
/** 次の単位の右に、その先の単位を何個置くか */
export const ROW_FUTURE = 10;
/** 1つの単位の文字数の上限（表示は省略記号で切れるので、極端に長い行で描くのが重くならないよう先に切る） */
export const ROW_TEXT_MAX = 300;

type RowLine = Pick<LyricsLine, "timeMs" | "kind" | "text">;

/** 時刻つきの行を表示単位に分ける。「消す」合図と空白だけの行は単位にしない。表示時刻は今の行と同じ式 */
export function buildRowUnits(lines: RowLine[], delayMs: number): RowUnit[] {
  const n = timedCount(lines);
  const units: RowUnit[] = [];
  for (let i = 0; i < n; i++) {
    if (!isShown(lines[i])) continue;
    const t = shownTime(lines[i], delayMs);
    const last = units.length > 0 ? units[units.length - 1] : undefined;
    if (last && last.timeMs === t) last.end = i;
    else units.push({ start: i, end: i, timeMs: t });
  }
  return units;
}

/** 行の添字を含む単位の番号（無ければ -1）。二分探索 */
export function unitIndexOf(units: RowUnit[], line: number): number {
  let lo = 0;
  let hi = units.length - 1;
  let idx = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (units[mid].start <= line) {
      idx = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return idx >= 0 && line <= units[idx].end ? idx : -1;
}

// ---- 括弧の中（掛け合い・合いの手）を本体と見分ける（v3.0.2） ----

/** 文の一片。sub は括弧とその中身（本体より一段薄く・小さく出す） */
export interface TextPart {
  text: string;
  sub: boolean;
}

const OPEN_PAREN = new Set(["(", "（"]);
const CLOSE_PAREN = new Set([")", "）"]);
/** 開きの無い閉じ括弧の直前がこれなら、括弧として扱わない（顔文字・番号） */
const NOT_PAREN_BEFORE_CLOSE = /^[:;=0-9０-９]$/;
/** 前の文字とひとまとまりで描かれるもの（結合文字・異体字セレクタ・ゼロ幅接合子・肌の色の修飾） */
const JOINS_PREV = /^[\p{M}\u200D\u{1F3FB}-\u{1F3FF}]/u;
const isHighSurrogate = (c: number) => c >= 0xd800 && c <= 0xdbff;

/**
 * 文を k 文字目の手前で切るとき、文字を壊さない位置まで戻す（検査1回目の指摘）。
 * - 切る位置の直前が上位サロゲート（絵文字などの前半）なら1つ戻す
 * - 切る位置の先頭が結合文字・異体字セレクタ・ゼロ幅接合子・肌の色の修飾なら、前の文字ごと落とす。直前がゼロ幅接合子でも同じ
 */
export function safeCut(t: string, k: number): number {
  let n = Math.max(0, Math.min(k, t.length));
  const unpair = () => {
    if (n > 0 && isHighSurrogate(t.charCodeAt(n - 1))) n--;
  };
  unpair();
  while (n > 0 && n < t.length && (JOINS_PREV.test(t.slice(n, n + 2)) || t.charCodeAt(n - 1) === 0x200d)) {
    n--;
    unpair();
  }
  return n;
}

function pushPart(out: TextPart[], text: string, sub: boolean) {
  if (!text) return;
  const last = out.length > 0 ? out[out.length - 1] : undefined;
  if (last && last.sub === sub) last.text += text;
  else out.push({ text, sub });
}

/**
 * 1行の文を、括弧（半角・全角の丸括弧）の中と外に分ける。括弧そのものも「中」に入れる。
 * - 半角と全角は区別せず、開きと閉じの数で数える（「（…)」も閉じる）
 * - 入れ子は一番外の括弧が閉じるまで「中」
 * - 開いたまま閉じていない：行末までを「中」とする（前の行から続く掛け合いが、行をまたいで書かれるため）
 * - 開きの無い閉じ括弧：行頭（か、前の括弧の閉じ）からその閉じ括弧までを「中」とする（同じ理由。前の行で開いた括弧の続き）
 *   ただし直前が「:」「;」「=」か数字（半角・全角）のものは括弧として扱わず、本体のまま出す（顔文字「:)」・番号「1)」。検査1回目の指摘）
 * 文字は1つも足し引きしない（つなぐと元の文に戻る）
 */
export function splitParens(text: string): TextPart[] {
  const out: TextPart[] = [];
  let depth = 0;
  let runStart = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (OPEN_PAREN.has(c)) {
      if (depth === 0) {
        pushPart(out, text.slice(runStart, i), false);
        runStart = i;
      }
      depth++;
    } else if (CLOSE_PAREN.has(c)) {
      // 開きの無い閉じ括弧のうち、顔文字「:)」「;)」「=)」と番号「1)」「１）」は括弧にしない（本体のまま）
      if (depth === 0 && i > 0 && NOT_PAREN_BEFORE_CLOSE.test(text[i - 1])) continue;
      if (depth > 1) {
        depth--;
      } else {
        // depth が 1 なら外の括弧が閉じた。0 なら開きの無い閉じ括弧（ここまでを前の行の括弧の続きとみなす）
        depth = 0;
        pushPart(out, text.slice(runStart, i + 1), true);
        runStart = i + 1;
      }
    }
  }
  pushPart(out, text.slice(runStart), depth > 0);
  return out;
}

/**
 * 行の範囲の文を、括弧の中と外に分けた一片の並びにする。表示する行だけを「 ／ 」でつなぐ（rangeText と同じ）。
 * 括弧は行ごとに数える（閉じていない括弧が「 ／ 」の先の行へ漏れない）。上限を超えたら切って「…」
 */
export function rangeParts(lines: RowLine[], start: number, end: number, max = ROW_TEXT_MAX): TextPart[] {
  const out: TextPart[] = [];
  let len = 0;
  let first = true;
  for (let i = Math.max(0, start); i <= end && i < lines.length; i++) {
    if (!isShown(lines[i])) continue;
    if (len >= max) {
      pushPart(out, "…", false);
      break;
    }
    if (!first) {
      pushPart(out, " ／ ", false);
      len += 3;
    }
    first = false;
    let t = lines[i].text;
    const cut = len + t.length > max;
    if (cut) t = t.slice(0, safeCut(t, max - len));
    for (const p of splitParens(t)) pushPart(out, p.text, p.sub);
    len += t.length;
    if (cut) {
      pushPart(out, "…", out.length > 0 && out[out.length - 1].sub);
      break;
    }
  }
  return out;
}

/** 一片の並びを1つの文に戻す */
export const partsText = (parts: TextPart[]): string => parts.map((p) => p.text).join("");

/** 列の状態の印（文は画面側。messages/ja.ts の lyricsRowText） */
export type RowStatus = "opening" | "interlude" | "ended" | "untimed" | "none" | "empty" | "error";

export interface RowItem {
  /** 行が変わっても同じ単位は同じ key（要素を作り直さないので、色の変化が動いて見える） */
  key: string;
  kind: "past" | "current" | "next" | "future" | "status" | "hint";
  /** 歌詞の文（status・hint のときは空） */
  text: string;
  /** 歌詞の文を括弧の中と外に分けたもの（status・hint のときは空） */
  parts: TextPart[];
  status?: RowStatus;
  /** 右に区切り「›」を付けるか（曲の最後の単位と、最後に置く印には付けない） */
  sep: boolean;
}

export interface RowModel {
  items: RowItem[];
  /** 列の左寄りにそろえる要素の key（今の行か、状態の印）。null なら何も並べない */
  anchorKey: string | null;
}

/** 歌詞の読み込み状態と中身（stores/lyrics.ts の LyricsState と、今の曲かどうかから作る） */
export interface RowSource {
  /** idle は曲が無い。loading は読み込み中（「歌詞がありません」と区別する） */
  status: "idle" | "loading" | "ready" | "error";
  source: string | null;
  format: "timed" | "plain" | null;
  lines: RowLine[];
}

const EMPTY_ROW: RowModel = { items: [], anchorKey: null };

function statusOnly(status: RowStatus, key: string): RowModel {
  return { items: [{ key, kind: "status", text: "", parts: [], status, sep: false }], anchorKey: key };
}

/** 今の行の範囲から、今の単位を引く（範囲に表示する行が無ければ -1。古い範囲が残っていても落ちない） */
function currentUnit(lines: RowLine[], units: RowUnit[], active: ActiveRange | null): number {
  if (!active || active.start < 0 || active.end >= lines.length) return -1;
  for (let i = active.end; i >= active.start; i--) {
    if (isShown(lines[i])) return unitIndexOf(units, i);
  }
  return -1;
}

/**
 * 列に並べるもの。今の行（findActive）と次の行（findNext）から作る。
 * - 今の行がある：歌い終わった単位・今・次・その先。最後の行なら次は無い
 * - 今の行が無く、次の単位が最初：「まもなく歌詞が始まります」＋最初の単位から（曲の冒頭）
 * - 今の行が無く、次の単位がある：「♪ 間奏」＋次の単位から（「消す」合図の後・空白だけの行）
 * - 今の行も次の単位も無い：歌い終わった単位のあとに「♪ 後奏」（最後の「消す」合図の後）
 * - 時刻なし：「時刻同期なし」＋案内。歌詞なし：「この曲には歌詞がありません」。読み込み中・曲なし：何も並べない
 * key の頭には prefix（曲 ID）を付ける。曲が変わったら同じ key が無くなり、動かさずに置き直す
 */
export function buildRow(src: RowSource, units: RowUnit[], active: ActiveRange | null, next: ActiveRange | null, prefix = ""): RowModel {
  if (src.status === "error") return statusOnly("error", prefix + "s");
  if (src.status !== "ready") return EMPTY_ROW;
  if (!src.source) return statusOnly("none", prefix + "s");
  if (src.format !== "timed") {
    return {
      items: [
        { key: prefix + "s", kind: "status", text: "", parts: [], status: "untimed", sep: true },
        { key: prefix + "h", kind: "hint", text: "", parts: [], sep: false },
      ],
      anchorKey: prefix + "s",
    };
  }
  if (units.length === 0) return statusOnly("empty", prefix + "s");
  const lines = src.lines;
  const last = units.length - 1;
  const unitItem = (i: number, kind: RowItem["kind"]): RowItem => {
    const parts = rangeParts(lines, units[i].start, units[i].end);
    return { key: prefix + "u" + i, kind, text: partsText(parts), parts, sep: i !== last };
  };
  const items: RowItem[] = [];
  const cur = currentUnit(lines, units, active);
  if (cur >= 0) {
    for (let i = Math.max(0, cur - ROW_PAST); i < cur; i++) items.push(unitItem(i, "past"));
    items.push(unitItem(cur, "current"));
    const to = Math.min(last, cur + 1 + ROW_FUTURE);
    for (let i = cur + 1; i <= to; i++) items.push(unitItem(i, i === cur + 1 ? "next" : "future"));
    return { items, anchorKey: prefix + "u" + cur };
  }
  let nx = -1;
  if (next && next.start >= 0 && next.start < lines.length) nx = unitIndexOf(units, next.start);
  const statusKey = prefix + "s";
  if (nx >= 0) {
    for (let i = Math.max(0, nx - ROW_PAST); i < nx; i++) items.push(unitItem(i, "past"));
    items.push({ key: statusKey, kind: "status", text: "", parts: [], status: nx === 0 ? "opening" : "interlude", sep: true });
    const to = Math.min(last, nx + ROW_FUTURE);
    for (let i = nx; i <= to; i++) items.push(unitItem(i, i === nx ? "next" : "future"));
    return { items, anchorKey: statusKey };
  }
  for (let i = Math.max(0, units.length - ROW_PAST); i <= last; i++) items.push({ ...unitItem(i, "past"), sep: true });
  items.push({ key: statusKey, kind: "status", text: "", parts: [], status: "ended", sep: false });
  return { items, anchorKey: statusKey };
}

/** 位置から列を作る（テスト用のまとめ。画面は単位の一覧を覚えておき、今の行・次の行が変わったときだけ buildRow を呼ぶ） */
export function lyricsRowAt(src: RowSource, delayMs: number, posMs: number, prefix = ""): RowModel {
  const units = src.status === "ready" && src.source && src.format === "timed" ? buildRowUnits(src.lines, delayMs) : [];
  return buildRow(src, units, findActive(src.lines, delayMs, posMs), findNext(src.lines, delayMs, posMs), prefix);
}
