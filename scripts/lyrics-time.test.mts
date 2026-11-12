// 歌詞の表示時刻の式（仕様 4.2：max(0, 元 − offset + 曲ごとの補正)）と「今の行」を確かめる。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/lyrics-time.test.mts
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  buildRow,
  buildRowUnits,
  findActive,
  findNext,
  lyricsRowAt,
  partsText,
  rangeParts,
  ROW_FUTURE,
  ROW_PAST,
  ROW_TEXT_MAX,
  safeCut,
  shownTime,
  splitParens,
  type RowModel,
  type RowSource,
} from "../src/stores/lyricsTime.ts";

type L = { timeMs: number | null; kind: "lyric" | "clear" | "untimed" };
type T = L & { text: string };
/** 時刻と本文から行を作る（本文が "" なら「消す」合図） */
const at = (timeMs: number, text: string): T => ({ timeMs, text, kind: text === "" ? "clear" : "lyric" });

test("1回目の検査 3：補正は 0 にそろえる前に足す", () => {
  // 元 0.2秒・offset +500ms → Rust は timeMs = -300 を返す。補正 +300ms → max(0, 0) = 0（以前の実装は 300）
  assert.equal(shownTime({ timeMs: -300 }, 300), 0);
  assert.equal(shownTime({ timeMs: -300 }, 400), 100);
  assert.equal(shownTime({ timeMs: -300 }, 0), 0);
  assert.equal(shownTime({ timeMs: 9800 }, -100), 9700);
  assert.equal(shownTime({ timeMs: 1000 }, -5000), 0);
});

test("今の行：表示時刻が今の位置以下の最後の行・同じ時刻はまとめる・消す合図の後は無し", () => {
  const lines: L[] = [
    { timeMs: -300, kind: "lyric" },
    { timeMs: 1000, kind: "lyric" },
    { timeMs: 1000, kind: "lyric" },
    { timeMs: 2000, kind: "clear" },
    { timeMs: 3000, kind: "lyric" },
    { timeMs: null, kind: "untimed" },
  ];
  assert.equal(findActive(lines, 500, 100), null); // 先頭は 0+… = 200ms
  assert.deepEqual(findActive(lines, 500, 200), { start: 0, end: 0 });
  assert.deepEqual(findActive(lines, 0, 0), { start: 0, end: 0 });
  assert.deepEqual(findActive(lines, 0, 1500), { start: 1, end: 2 });
  assert.equal(findActive(lines, 0, 2500), null);
  assert.deepEqual(findActive(lines, 0, 99999), { start: 4, end: 4 });
  // 補正を足すと全体が後ろへずれる
  assert.deepEqual(findActive(lines, 1000, 1500), { start: 0, end: 0 });
});

// 再生バーで今の行の右に出す「次の行」（v3.0.1）。歌詞はダミーの文
test("次の行：今の行の前・途中・最後の行", () => {
  const lines: T[] = [at(1000, "いち"), at(2000, "に"), at(3000, "さん"), { timeMs: null, kind: "untimed", text: "時刻なし" }];
  // 最初の行より前（今の行が無い）：次の行だけ返す
  assert.equal(findActive(lines, 0, 500), null);
  assert.deepEqual(findNext(lines, 0, 500), { start: 0, end: 0 });
  // 途中：今の行のすぐ次
  assert.deepEqual(findNext(lines, 0, 1000), { start: 1, end: 1 });
  assert.deepEqual(findNext(lines, 0, 1999), { start: 1, end: 1 });
  assert.deepEqual(findNext(lines, 0, 2000), { start: 2, end: 2 });
  // 最後の行：次は無い（末尾の時刻なしの行は数えない）
  assert.deepEqual(findActive(lines, 0, 3000), { start: 2, end: 2 });
  assert.equal(findNext(lines, 0, 3000), null);
  assert.equal(findNext(lines, 0, 99999), null);
});

test("次の行：消す合図と空の行は飛ばす・消す合図の後も次を返す", () => {
  const lines: T[] = [at(1000, "いち"), at(2000, ""), at(2500, "   "), at(3000, "さん"), at(4000, "")];
  // 今の行の次が「消す」合図と空白だけの行 → その先の行
  assert.deepEqual(findNext(lines, 0, 1500), { start: 3, end: 3 });
  // 「消す」合図の後（今の行は無い）→ 次の行
  assert.equal(findActive(lines, 0, 2200), null);
  assert.deepEqual(findNext(lines, 0, 2200), { start: 3, end: 3 });
  // 最後の表示する行のあとは「消す」合図しか無い → 無し
  assert.equal(findNext(lines, 0, 3000), null);
  // 時刻つきの行が無い・行が無い
  assert.equal(findNext([], 0, 0), null);
  assert.equal(findNext([{ timeMs: null, kind: "untimed", text: "全文" }], 0, 0), null);
});

test("次の行：同じ時刻のまとまり", () => {
  const lines: T[] = [at(1000, "あ"), at(1000, "い"), at(2000, "う"), at(2000, ""), at(2000, "え"), at(3000, "お")];
  // 今の行がまとまり（0〜1）→ まとまりの次（2）。次もまとまりなら、まとめて返す（間の消す合図は描くときに外す）
  assert.deepEqual(findActive(lines, 0, 1500), { start: 0, end: 1 });
  assert.deepEqual(findNext(lines, 0, 1500), { start: 2, end: 4 });
  // 最初の行より前：最初のまとまりを返す
  assert.deepEqual(findNext(lines, 0, 0), { start: 0, end: 1 });
  // 次のまとまりが今のまとまりと重ならない
  assert.deepEqual(findNext(lines, 0, 2000), { start: 5, end: 5 });
});

test("次の行：曲ごとの補正を足した時刻で決める（今の行と同じ式）", () => {
  // 元 0.2秒・offset +500ms → timeMs = -300（Rust の返す値）
  const lines: T[] = [at(-300, "いち"), at(1000, "に"), at(2000, "さん")];
  // 補正 0：先頭は max(0, -300) = 0 → 0ms で始まっている。次は 2 行目
  assert.deepEqual(findNext(lines, 0, 0), { start: 1, end: 1 });
  // 補正 +500：先頭は 200ms → 100ms ではまだ始まっていない
  assert.deepEqual(findNext(lines, 500, 100), { start: 0, end: 0 });
  assert.deepEqual(findNext(lines, 500, 200), { start: 1, end: 1 });
  // 補正 +1000：2 行目は 2000ms → 1500ms では次がまだ 2 行目
  assert.deepEqual(findNext(lines, 1000, 1500), { start: 1, end: 1 });
  assert.deepEqual(findNext(lines, 1000, 2000), { start: 2, end: 2 });
  // 補正 -1500：先頭と 2 行目がどちらも 0 にそろい、同じ時刻のまとまりになる
  assert.deepEqual(findActive(lines, -1500, 0), { start: 0, end: 1 });
  assert.deepEqual(findNext(lines, -1500, 0), { start: 2, end: 2 });
  assert.equal(findNext(lines, -1500, 500), null);
});

// ---- 流れる歌詞の列（v3.0.2）。歌詞はダミーの文 ----

/** 時刻つきの歌詞の入れ物 */
const timed = (lines: T[]): RowSource => ({ status: "ready", source: "manual", format: "timed", lines });
/** 並びを「種類:文」の列にする（状態の印は「種類:印」） */
const shape = (m: RowModel): string[] => m.items.map((i) => `${i.kind}:${i.status ?? i.text}`);
const anchorKind = (m: RowModel): string | undefined => m.items.find((i) => i.key === m.anchorKey)?.kind;

test("列：通常（歌い終わった・今・次・その先）と最後の行", () => {
  const lines: T[] = [at(1000, "いち"), at(2000, "に"), at(3000, "さん"), at(4000, "し"), at(5000, "ご")];
  const m = lyricsRowAt(timed(lines), 0, 3500);
  assert.deepEqual(shape(m), ["past:いち", "past:に", "current:さん", "next:し", "future:ご"]);
  assert.equal(anchorKind(m), "current");
  // 区切りは曲の最後の単位にだけ付けない
  assert.deepEqual(
    m.items.map((i) => i.sep),
    [true, true, true, true, false],
  );
  // 最後の行：次を出さない
  const end = lyricsRowAt(timed(lines), 0, 99999);
  assert.deepEqual(shape(end).slice(-1), ["current:ご"]);
  assert.equal(
    end.items.some((i) => i.kind === "next" || i.kind === "future"),
    false,
  );
  assert.equal(end.items[end.items.length - 1].sep, false);
});

test("列：曲の冒頭・間奏（消す合図の後・空白だけの行）・最後の消す合図の後", () => {
  const lines: T[] = [at(1000, "いち"), at(2000, ""), at(3000, "さん"), at(4000, "   "), at(5000, "ご"), at(6000, "")];
  // 冒頭：最初の行より前
  const open = lyricsRowAt(timed(lines), 0, 500);
  assert.deepEqual(shape(open), ["status:opening", "next:いち", "future:さん", "future:ご"]);
  assert.equal(anchorKind(open), "status");
  // 消す合図の後：間奏＋次の行から（歌い終わった行は左に置く）
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 0, 2500)), ["past:いち", "status:interlude", "next:さん", "future:ご"]);
  // 空白だけの行が今の行：間奏と同じ
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 0, 4500)), ["past:いち", "past:さん", "status:interlude", "next:ご"]);
  // 最後の消す合図の後：後奏（次は無い）
  const after = lyricsRowAt(timed(lines), 0, 7000);
  assert.deepEqual(shape(after), ["past:いち", "past:さん", "past:ご", "status:ended"]);
  assert.equal(after.items[after.items.length - 1].sep, false);
  // 先頭が消す合図でも、まだ何も歌っていなければ冒頭
  assert.deepEqual(shape(lyricsRowAt(timed([at(0, ""), at(1000, "いち")]), 0, 500)), ["status:opening", "next:いち"]);
});

test("列：時刻なし・歌詞なし・読み込み中・読み込み失敗・表示する行が無い", () => {
  const plain = lyricsRowAt({ status: "ready", source: "external", format: "plain", lines: [{ timeMs: null, kind: "untimed", text: "全文" }] }, 0, 0);
  assert.deepEqual(shape(plain), ["status:untimed", "hint:"]);
  assert.equal(anchorKind(plain), "status");
  const none = lyricsRowAt({ status: "ready", source: null, format: null, lines: [] }, 0, 0);
  assert.deepEqual(shape(none), ["status:none"]);
  // 読み込み中と曲なしは何も並べない（「歌詞がありません」と区別する）
  assert.deepEqual(lyricsRowAt({ status: "loading", source: null, format: null, lines: [] }, 0, 0), { items: [], anchorKey: null });
  assert.deepEqual(lyricsRowAt({ status: "idle", source: null, format: null, lines: [] }, 0, 0), { items: [], anchorKey: null });
  assert.deepEqual(shape(lyricsRowAt({ status: "error", source: null, format: null, lines: [] }, 0, 0)), ["status:error"]);
  // 時刻つきだが、消す合図と空白だけ
  assert.deepEqual(shape(lyricsRowAt(timed([at(1000, ""), at(2000, "  ")]), 0, 1500)), ["status:empty"]);
});

test("列：同じ時刻の行のまとまりは1つの単位（間の消す合図・空白は外す）", () => {
  const lines: T[] = [at(1000, "あ"), at(1000, "い"), at(2000, "う"), at(2000, ""), at(2000, " "), at(2000, "え"), at(3000, "お")];
  const units = buildRowUnits(lines, 0);
  assert.deepEqual(
    units.map((u) => [u.start, u.end, u.timeMs]),
    [
      [0, 1, 1000],
      [2, 5, 2000],
      [6, 6, 3000],
    ],
  );
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 0, 1500)), ["current:あ ／ い", "next:う ／ え", "future:お"]);
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 0, 2000)), ["past:あ ／ い", "current:う ／ え", "next:お"]);
});

test("列：曲ごとの補正を足した時刻で決める（今の行と同じ式。0 未満は 0 にそろえてまとまる）", () => {
  const lines: T[] = [at(-300, "いち"), at(1000, "に"), at(2000, "さん")];
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 0, 0)), ["current:いち", "next:に", "future:さん"]);
  // +500：先頭は 200ms。100ms ではまだ冒頭
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 500, 100)), ["status:opening", "next:いち", "future:に", "future:さん"]);
  // +1000：1500ms では今はいち（2行目は 2000ms）
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 1000, 1500)), ["current:いち", "next:に", "future:さん"]);
  // -1500：先頭と2行目が 0 にそろい、1つの単位になる
  assert.deepEqual(shape(lyricsRowAt(timed(lines), -1500, 0)), ["current:いち ／ に", "next:さん"]);
  // 全部が 0 未満：全部 0 にそろって1つ
  assert.deepEqual(shape(lyricsRowAt(timed(lines), -600000, 0)), ["current:いち ／ に ／ さん"]);
});

test("列：消す合図の連続・古い範囲が残っていても落ちない", () => {
  const lines: T[] = [at(1000, "いち"), at(2000, ""), at(2001, ""), at(2002, ""), at(2003, ""), at(5000, "ご")];
  assert.deepEqual(shape(lyricsRowAt(timed(lines), 0, 1500)), ["current:いち", "next:ご"]);
  for (const pos of [2000, 2001, 2003, 4999]) {
    assert.deepEqual(shape(lyricsRowAt(timed(lines), 0, pos)), ["past:いち", "status:interlude", "next:ご"]);
  }
  // 範囲が歌詞より外（前の曲の今の行が一瞬残ったとき）
  const src = timed(lines);
  const units = buildRowUnits(lines, 0);
  assert.deepEqual(shape(buildRow(src, units, { start: 50, end: 60 }, { start: 70, end: 70 })), ["past:いち", "past:ご", "status:ended"]);
  assert.deepEqual(shape(buildRow(src, units, null, null)), ["past:いち", "past:ご", "status:ended"]);
});

test("列：何万行でも並べる数は一定・行が変わったときの作り直しは行数に依存しない", () => {
  const n = 100_000;
  const lines: T[] = [];
  for (let i = 0; i < n; i++) lines.push(at(i * 10, i % 7 === 3 ? "" : i % 11 === 5 ? "   " : `行${i}`));
  const t0 = performance.now();
  const units = buildRowUnits(lines, 0);
  const tUnits = performance.now() - t0;
  assert.ok(units.length > 70_000);
  const src = timed(lines);
  const t1 = performance.now();
  let m: RowModel = { items: [], anchorKey: null };
  for (let k = 0; k < 2000; k++) {
    const pos = (k * 4999) % (n * 10);
    m = buildRow(src, units, findActive(lines, 0, pos), findNext(lines, 0, pos), "7:");
    assert.ok(m.items.length <= ROW_PAST + 2 + ROW_FUTURE);
  }
  const tRows = performance.now() - t1;
  assert.ok(tUnits < 1000, `units ${tUnits}ms`);
  assert.ok(tRows < 1000, `rows ${tRows}ms`);
  assert.ok(m.anchorKey?.startsWith("7:"));
  // 同じ時刻に何万行（補正で全部 0 にそろったとき）：1つの単位。文は上限で切る
  const same = buildRowUnits(lines, -10_000_000);
  assert.equal(same.length, 1);
  const one = buildRow(src, same, findActive(lines, -10_000_000, 0), findNext(lines, -10_000_000, 0));
  assert.equal(one.items.length, 1);
  assert.ok(one.items[0].text.length <= ROW_TEXT_MAX + 1);
  assert.ok(one.items[0].text.endsWith("…"));
  // 極端に長い1行も上限で切る
  const long = lyricsRowAt(timed([at(0, "あ".repeat(1_000_000))]), 0, 0);
  assert.equal(long.items[0].text.length, ROW_TEXT_MAX + 1);
});

// ---- 括弧の中（掛け合い）を本体と見分ける（v3.0.2）。歌詞はダミーの文 ----

/** 括弧の中を [ ] で囲み、一片の境目を | で示す */
const sp = (s: string) =>
  splitParens(s)
    .map((p) => (p.sub ? `[${p.text}]` : p.text))
    .join("|");

test("括弧：なし・半角・全角・複数・入れ子", () => {
  assert.equal(sp("ただの一行"), "ただの一行");
  assert.equal(sp("本体 (合いの手) 続き"), "本体 |[(合いの手)]| 続き");
  assert.equal(sp("本体（合いの手）"), "本体|[（合いの手）]");
  assert.equal(sp("（先に）本体（後で）終わり"), "[（先に）]|本体|[（後で）]|終わり");
  // 隣り合う括弧は1つにつながる
  assert.equal(sp("本体(あ)(い)"), "本体|[(あ)(い)]");
  // 入れ子：一番外が閉じるまで中
  assert.equal(sp("本体（外 (内) 外）続き"), "本体|[（外 (内) 外）]|続き");
  // 半角で開いて全角で閉じても閉じる
  assert.equal(sp("本体(混ぜた）続き"), "本体|[(混ぜた）]|続き");
});

test("括弧：閉じていない・開きの無い閉じ・括弧だけの行・空", () => {
  // 閉じていない：行末まで中（次の行へ続く掛け合い）
  assert.equal(sp("本体（次の行へ続く"), "本体|[（次の行へ続く]");
  assert.equal(sp("本体（外 (内) まだ外"), "本体|[（外 (内) まだ外]");
  // 開きの無い閉じ括弧：行頭からそこまでが中（前の行の括弧の続き）
  assert.equal(sp("前から続く）本体"), "[前から続く）]|本体");
  assert.equal(sp("本体 (あ) 続き) 後"), "本体 |[(あ) 続き)]| 後");
  // 括弧だけの行
  assert.equal(sp("（ラララ）"), "[（ラララ）]");
  assert.equal(sp("()"), "[()]");
  assert.equal(sp(")"), "[)]");
  assert.equal(sp("("), "[(]");
  assert.deepEqual(splitParens(""), []);
  // 文字は足し引きしない
  for (const s of ["a(b(c)d", "))((", "（）)(（", "本体（外 (内) 外）続き"]) assert.equal(partsText(splitParens(s)), s);
});

test("括弧：まとまりでは行ごとに数える（閉じていない括弧が次の行へ漏れない）", () => {
  const lines: T[] = [at(1000, "本体（続く"), at(1000, "次の本体")];
  assert.deepEqual(rangeParts(lines, 0, 1), [
    { text: "本体", sub: false },
    { text: "（続く", sub: true },
    { text: " ／ 次の本体", sub: false },
  ]);
  // 列の項目にも同じものが入る
  const m = lyricsRowAt(timed([at(0, "本体 (声)"), at(1000, "（ラララ）")]), 0, 0);
  assert.deepEqual(m.items[0].parts, [
    { text: "本体 ", sub: false },
    { text: "(声)", sub: true },
  ]);
  assert.deepEqual(m.items[1].parts, [{ text: "（ラララ）", sub: true }]);
  // 上限で切ると、切れた括弧は閉じていない扱い（「…」も中）
  const cut = rangeParts([at(0, "あ".repeat(10) + "（" + "い".repeat(10))], 0, 0, 15);
  assert.equal(partsText(cut), "あ".repeat(10) + "（いいいい…");
  assert.equal(cut[cut.length - 1].sub, true);
});

// ---- 直し1回目（検査1回目の指摘）。歌詞はダミーの文 ----

test("括弧（直し1）：顔文字の「:)」と番号の「1)」は括弧にしない", () => {
  // 開きの無い閉じ括弧でも、直前が : ; = か数字なら本体のまま（行全体が薄くならない）
  assert.deepEqual(splitParens("hello :)"), [{ text: "hello :)", sub: false }]);
  assert.equal(sp("またね ;)"), "またね ;)");
  assert.equal(sp("いいね =)"), "いいね =)");
  assert.equal(sp("1) はじめに"), "1) はじめに");
  assert.equal(sp("１）はじめに"), "１）はじめに");
  assert.equal(sp("その 12) 番目"), "その 12) 番目");
  // それ以外の開きの無い閉じ括弧は、今までどおり行頭からそこまでが中
  assert.equal(sp("前から続く）本体 :)"), "[前から続く）]|本体 :)");
  // 開いた括弧の中の閉じ括弧は、直前が : でも数字でも閉じる
  assert.equal(sp("本体 (声 :)"), "本体 |[(声 :)]");
  assert.equal(sp("本体（2）"), "本体|[（2）]");
  assert.equal(sp("(笑) :)"), "[(笑)]| :)");
  // 行頭の閉じ括弧（直前が無い）は今までどおり中
  assert.equal(sp(")"), "[)]");
  for (const s of ["hello :)", "1) a", "a :) (b) c)"]) assert.equal(partsText(splitParens(s)), s);
});

/** 片割れのサロゲートがあるか */
const loneSurrogate = (s: string) => /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/.test(s);

test("上限で切る（直し1）：絵文字の途中・結合文字の途中で切らない", () => {
  // 3文字目で切ると絵文字の前半だけが残る位置 → 絵文字ごと落とす
  assert.equal(partsText(rangeParts([at(0, "ab😀c")], 0, 0, 3)), "ab…");
  assert.equal(partsText(rangeParts([at(0, "ab😀c")], 0, 0, 4)), "ab😀…");
  // どの位置で切っても片割れが残らない（通常の文字を何文字挟んでも）
  for (let pad = 0; pad < 4; pad++) {
    const text = "x".repeat(pad) + "😀🎵あ👍🏽か\u3099👨\u200D👩\u200D👧❤\uFE0Fend";
    for (let max = 1; max < text.length; max++) {
      const out = partsText(rangeParts([at(0, text)], 0, 0, max));
      assert.equal(loneSurrogate(out), false, `pad ${pad} max ${max}: ${JSON.stringify(out)}`);
      const body = out.endsWith("…") ? out.slice(0, -1) : out;
      // 切った位置の直前がゼロ幅接合子にならない・切った先が結合文字で始まらない
      assert.equal(body.endsWith("\u200D"), false, `pad ${pad} max ${max}`);
      assert.ok(text.startsWith(body));
      const rest = text.slice(body.length);
      assert.equal(/^[\p{M}\u200D\u{1F3FB}-\u{1F3FF}]/u.test(rest), false, `pad ${pad} max ${max}: ${JSON.stringify(rest)}`);
    }
  }
  // 結合の濁点（か + U+3099）の途中で切らない
  assert.equal(partsText(rangeParts([at(0, "か\u3099か\u3099")], 0, 0, 3)), "か\u3099…");
  // 列の項目：299字のあとに絵文字（300字目が前半）→ 絵文字ごと落として「…」
  const row = lyricsRowAt(timed([at(0, "あ".repeat(299) + "😀😀")]), 0, 0);
  assert.equal(loneSurrogate(row.items[0].text), false);
  assert.equal(row.items[0].text, "あ".repeat(299) + "…");
  // safeCut は範囲外の値でも落ちない
  assert.equal(safeCut("abc", -5), 0);
  assert.equal(safeCut("abc", 99), 3);
  assert.equal(safeCut("", 3), 0);
});

// ---- v3.1.0 直し1回目：歌詞をネットから取るときの打ち切りの文を、原因ごとに分ける（src/messages/ja.ts） ----
import { errorMessage, inboxReportText, lyricsFetchAbortMessage, lyricsFetchDoneText, lyricsFetchProgressText, lyricsWarning } from "../src/messages/ja.ts";

test("歌詞の取得（直し1）：エラーが続いたときの文は、最後のエラーで原因を分ける", () => {
  const net = lyricsFetchAbortMessage("too_many_errors", "timeout");
  assert.match(net, /ネットにつながっているか/);
  const http4 = lyricsFetchAbortMessage("too_many_errors", "http_404");
  assert.match(http4, /404/);
  assert.match(http4, /受け付けませんでした/);
  assert.doesNotMatch(http4, /ネットにつながっているか/);
  const http5 = lyricsFetchAbortMessage("too_many_errors", "http_500");
  assert.match(http5, /混み合っているか止まっています/);
  const bad = lyricsFetchAbortMessage("too_many_errors", "bad_response");
  assert.match(bad, /応答を読めませんでした/);
  assert.doesNotMatch(bad, /ネットにつながっているか/);
  assert.match(lyricsFetchAbortMessage("too_many_errors", "write_failed"), /データベースに保存できませんでした/);
  assert.match(lyricsFetchAbortMessage("too_many_errors", "tls_error"), /安全な接続/);
  // 原因が分からないとき・知らない記号
  assert.equal(lyricsFetchAbortMessage("too_many_errors", null), "エラーが続いたので止めました");
  assert.match(lyricsFetchAbortMessage("too_many_errors", "something_new"), /something_new/);
  // ほかの打ち切りは今までどおり
  assert.equal(lyricsFetchAbortMessage("cancelled", "timeout"), "途中でやめました");
  assert.match(lyricsFetchAbortMessage("internal_error", null), /問題が起きた/);
  // 終わりの知らせにも原因が入る。途中で外された曲の件数も出る
  const base = {
    running: false, finished: true, preparing: false, prepareTotal: 12, prepareDone: 12, mode: "missing" as const, total: 10, done: 6, synced: 0, plain: 0, instrumental: 0,
    notFound: 0, tooLarge: 0, errors: 5, kept: 0, hasTag: 0, noMetadata: 0, skippedKnown: 0, changedDuringRun: 1,
    lastError: "http_403", requests: 5, aborted: "too_many_errors",
  };
  const done = lyricsFetchDoneText(base);
  assert.match(done.detail, /403/);
  assert.match(done.detail, /途中でライブラリから外された/);
  assert.doesNotMatch(done.detail, /ネットにつながっているか/);
});

test("歌詞の取得（直し2）：送る曲を選んでいる間は「準備中」の進み具合を出す", () => {
  const p = {
    running: true, finished: false, preparing: true, prepareTotal: 5000, prepareDone: 1200, mode: "missing" as const, total: 0,
    done: 0, synced: 0, plain: 0, instrumental: 0, notFound: 0, tooLarge: 0, errors: 0, kept: 0, hasTag: 0, noMetadata: 0,
    skippedKnown: 0, changedDuringRun: 0, lastError: null, requests: 0, aborted: null,
  };
  assert.equal(lyricsFetchProgressText(p), "準備中 5000曲中 1200曲");
  assert.match(lyricsFetchProgressText({ ...p, preparing: false, total: 300, done: 10, plain: 4 }), /^300曲中 10曲・取れた 4/);
});

test("v3.1.1：行の上限・ネットワーク上の場所・ファイルでないものを、文で知らせる", () => {
  assert.match(lyricsWarning("lines_truncated"), /5,000行/);
  const r = {
    imported: 1, manualKept: 0, unknownTrack: 0, duplicate: 0, encoding: 0, emptyOrTooLarge: 0, keptInInbox: 0,
    ignored: 2, linesTruncated: 1, trackIds: [1],
  };
  const t = inboxReportText(r);
  assert.match(t.problems, /ファイルでないため読まなかったもの 2件/);
  assert.match(t.problems, /先頭の 5,000行だけを表示するもの 1件/);
  const e = errorMessage({ code: "network_path_unsupported", detail: "x" });
  assert.match(e.text, /ネットワーク上の場所/);
  assert.notEqual(e.text, errorMessage({ code: "other", detail: "" }).text);
});
