// v3.8.0 段2-2：同時再生の画面の側（出力ボタン・出力のプルダウン・上の帯の「詳細」・詳細の配置・ミキサー・メーター・設定画面）を確かめる。
//   - 窓の幅 480〜2560px・高さ 480〜1440px で、はみ出し・重なり・再生ボタンのずれが無いこと（仕様 10章。レイアウトの計算を純粋な関数にして当てる）
//   - 詳細の左右／上下の切り替わりと、曲一覧の行の数・流れる歌詞の列を自動で隠す決まり
//   - 寸法の変数（CSS）と JS の値が同じこと・スキンから変えさせない変数（Rust と画面の LOCKED_VARS）
//   - メーターの値（dB・色の帯・直前の最大の線 20 秒・音割れの印 2 秒）、出力ボタンに出すもの、デバイスの選び方
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/ui-detail.test.mts
// 例の名前は架空。実在の曲名・アーティスト名・機器の製品名・ユーザー名は入れない。
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  COMPACT_AUTO_BELOW,
  compactListOn,
  currentViewMode,
  listLayoutFor,
  pressMode,
  TOPBAR,
  trackRowHeight,
  VIEW_MODES,
  viewModeLabelsFit,
  type DensityUiLike,
} from "../src/layout/density.ts";
import { PLAYER, playerLayout, type MovableButton } from "../src/layout/playerLayout.ts";
import {
  APP,
  barWidth,
  centerWidth,
  DETAIL,
  detailLayout,
  LIST_CHROME,
  MIXER,
  MIXER_ROW_H,
  mixerContentHeight,
  mixerMinHeight,
  mixerWidth,
} from "../src/layout/detailLayout.ts";
import { BAND_AT, dbFraction, dbText, IDLE_VIEW, isClip, METER, meterStep, newMeterState, toDb } from "../src/layout/meter.ts";
import {
  canAddOutput,
  deviceChoices,
  outputButtonModel,
  pct,
  slotName,
  slotTitle,
  STATUS_TEXT,
  statusHint,
  UNKNOWN_DEVICE_NAME,
} from "../src/layout/outputs.ts";
import { PLAYER_BUTTONS, playerButtonShown } from "../src/layout/visibility.ts";

/** AppShell と同じ渡し方（曲一覧の行の高さ・再生バーの高さ） */
function DL(o: { winW: number; winH: number; sidebarCollapsed: boolean; slots: number; lyricsRow: boolean; ui: DensityUiLike; outputsShown: boolean }) {
  return detailLayout({
    winW: o.winW,
    winH: o.winH,
    sidebarCollapsed: o.sidebarCollapsed,
    slots: o.slots,
    lyricsRow: o.lyricsRow,
    rowHeight: (w) => trackRowHeight(compactListOn(o.ui, w), listLayoutFor(w).rowH),
    playerH: playerLayout(barWidth(o.winW), () => true, o.outputsShown ? PLAYER.output3 : 0).mode === "one" ? PLAYER.h1 : PLAYER.h3,
  });
}

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");
const css = read("src/theme/base.css");
const tokens = read("src/theme/tokens.css");
const rule = (sel: string) => {
  const at = css.indexOf("\n" + sel + " {");
  assert.ok(at >= 0, sel);
  return css.slice(at, css.indexOf("}", at));
};
const pxOf = (src: string, name: string) => {
  const m = src.match(new RegExp(`${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}: (\\d+)px;`));
  assert.ok(m, name);
  return Number(m![1]);
};

// ---------- 寸法：CSS と JS が同じ値 ----------

test("骨組みの寸法（APP）は base.css の .app・599px の段と tokens.css の値と同じ", () => {
  const app = rule(".app");
  assert.equal(Number(app.match(/gap: (\d+)px;/)![1]), APP.gap);
  assert.equal(Number(app.match(/padding: (\d+)px;/)![1]), APP.pad);
  const at = css.indexOf("@media (max-width: 599px) {");
  const block = css.slice(at, css.indexOf("\n}\n", at));
  assert.match(block, new RegExp(`\\.app \\{\\s*gap: ${APP.gapNarrow}px;\\s*padding: ${APP.padNarrow}px;`));
  assert.equal(APP.narrowBelow, 600);
  assert.equal(pxOf(tokens, "--sidebar-w"), APP.sidebarW);
  assert.equal(pxOf(tokens, "--sidebar-w-collapsed"), APP.sidebarCollapsedW);
  assert.equal(pxOf(tokens, "--lyrics-row-h"), APP.lyricsRowH);
  // サイドバーを自動でたたむ幅（useLayoutSize）
  assert.match(read("src/components/useLayoutSize.ts"), new RegExp(`sidebarAuto: w < ${APP.sidebarAutoBelow},`));
});

test("寸法の変数（仕様 9章）：--player-output-w・--output-popup-w・--mixer-w・--mixer-strip-min-w・--meter-h が tokens.css にあり、JS と同じ値", () => {
  assert.equal(pxOf(tokens, "--player-output-w"), PLAYER.output3);
  assert.match(rule(":root:has(.player.one-row)"), new RegExp(`--player-output-w: ${PLAYER.output1}px;`));
  assert.equal(pxOf(tokens, "--mixer-w"), DETAIL.mixerMin);
  assert.equal(pxOf(tokens, "--mixer-strip-min-w"), MIXER.stripMin);
  assert.ok(pxOf(tokens, "--output-popup-w") >= 300);
  const meterH = pxOf(tokens, "--meter-h");
  // メーターの塊（左右2本 ＋ 間 2 ＋ 目盛り 12）が、ミキサーの行の下の段（24px）にちょうど入る
  assert.match(rule(".meter"), /gap: 2px;/);
  assert.match(rule(".meter-bar"), /height: var\(--meter-h\);/);
  assert.match(rule(".meter-scale"), /height: 12px;/);
  assert.equal(meterH * 2 + 2 + 12, MIXER.rowBottom);
});

test("ミキサーの寸法：base.css の .mixer の節と detailLayout.ts の MIXER が同じ値（行 68px・見出し 32px）", () => {
  assert.match(rule(".mixer"), new RegExp(`gap: ${MIXER.gap}px;\\s*padding: ${MIXER.padY}px ${MIXER.padX}px;`));
  assert.match(rule(".mixer"), /overflow-y: auto;/);
  assert.match(rule(".mixer-head"), new RegExp(`height: ${MIXER.headH}px;`));
  assert.match(rule(".mixer-rows"), new RegExp(`gap: ${MIXER.rowGap}px;`));
  assert.match(rule(".mixer-empty"), new RegExp(`height: ${MIXER.emptyH}px;`));
  const row = rule(".mixer-row");
  assert.match(row, new RegExp(`grid-template-rows: ${MIXER.rowTop}px ${MIXER.rowBottom}px;`));
  assert.match(row, new RegExp(`row-gap: ${MIXER.rowInnerGap}px;`));
  assert.match(row, new RegExp(`padding: ${MIXER.rowPadY}px ${MIXER.rowPadX}px;`));
  assert.match(row, /min-width: var\(--mixer-strip-min-w\);/);
  assert.equal(MIXER_ROW_H, 68);
  // パネルの枠（1px）
  assert.equal(MIXER.border, 2);
  assert.equal(mixerContentHeight(0), 2 + 16 + 32 + 6 + 40);
  assert.equal(mixerContentHeight(1), 2 + 16 + 32 + 6 + 68);
  assert.equal(mixerContentHeight(3), 2 + 16 + 32 + 6 + 68 * 3 + 4 * 2);
  assert.equal(mixerMinHeight(3), mixerContentHeight(1));
  assert.equal(mixerMinHeight(0), mixerContentHeight(0));
  // 行の中身の最低幅は、いちばん狭いミキサー（380px）の中に入る
  assert.ok(MIXER.stripMin <= DETAIL.mixerMin - MIXER.border - MIXER.padX * 2);
  // 行の上の段のボタン（.icon-btn.small 28px）が 28px の段に入る
  assert.match(rule(".icon-btn.small"), /height: 28px;/);
});

test("出力ボタン：高さ 40px・幅は --player-output-w（3列 112px・1段 80px）。中身（アイコン 20・間・名前）が幅に入る", () => {
  const b = rule(".output-btn");
  assert.match(b, /width: var\(--player-output-w\);/);
  assert.match(b, new RegExp(`height: ${PLAYER.outputH}px;`));
  assert.match(b, /padding: 0 8px;/);
  assert.match(b, /gap: 6px;/);
  assert.match(b, /border: 1px solid var\(--c-outline\);/);
  assert.match(rule(".output-icon"), /width: 20px;\s*height: 20px;/);
  assert.match(rule(".output-icons"), /gap: 2px;/);
  // 3列：枠 2 ＋ 余白 16 ＋ アイコン 20 ＋ 間 6 ＋ 名前 → 名前に 68px（14px の字で全角4字以上）
  const nameW = PLAYER.output3 - 2 - 16 - 20 - 6;
  assert.equal(nameW, 68);
  assert.ok(nameW >= 4 * 14);
  // 3つのアイコン（20×3 ＋ 間 2×2）は、3列でも1段（余白 6）でも入る
  assert.ok(2 + 16 + 64 <= PLAYER.output3);
  assert.match(rule(".output-btn.one"), /padding: 0 6px;/);
  assert.ok(2 + 12 + 64 <= PLAYER.output1);
  // 再生バーの高さ（1段 76・3列 80）に入る
  assert.ok(PLAYER.outputH <= PLAYER.h1 - 2);
  // 名前は「…」で切る（中身で幅を変えない）
  assert.match(rule(".output-btn-text"), /text-overflow: ellipsis;/);
});

test("スキンから変えさせない変数：Rust（skins\\tokens.rs）と画面（src\\skins\\tokens.ts）の両方に、段2-2 の5つが同じ並びで入り、着せ替えの仕様にも書いてある", () => {
  const rs = read("src-tauri/src/skins/tokens.rs");
  const m = rs.match(/pub const LOCKED_VARS: \[&str; (\d+)\] = \[([\s\S]*?)\];/);
  assert.ok(m);
  const rsNames = [...m![2].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  const t = read("src/skins/tokens.ts").match(/export const LOCKED_VARS = \[([\s\S]*?)\];/);
  const tsNames = [...t![1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  assert.deepEqual(tsNames, rsNames);
  assert.equal(Number(m![1]), rsNames.length);
  const doc = read("docs/着せ替え_仕様.md");
  for (const v of ["--player-output-w", "--output-popup-w", "--mixer-w", "--mixer-strip-min-w", "--meter-h"]) {
    assert.ok(rsNames.includes(v), v);
    assert.ok(tokens.includes(`${v}:`), v);
    assert.ok(doc.includes("`" + v + "`"), v);
  }
  // アイコンの枠は大きさを固定（スキンで消音のアイコンを画像にしても崩れない）
  assert.match(rule(".output-icon .icon"), /width: 20px;\s*height: 20px;\s*object-fit: contain;/);
});

// ---------- 再生バー（出力ボタンあり） ----------

const MOV: MovableButton[] = ["shuffle", "repeat", "volume", "lyricsRow", "lyrics"];
const winBar = (W: number) => W - (W < APP.narrowBelow ? APP.padNarrow : APP.pad) * 2;

test("再生バー（出力ボタンあり）：窓の幅 480〜2560px の1px ごと・ボタンの出し入れ 64 通りで、3列は再生ボタンが中央・右の列に出力ボタンと中身が入り、1段は曲名に 120px が残る", () => {
  let firstThree = -1;
  for (let W = 480; W <= 2560; W++) {
    const barW = winBar(W);
    for (let mask = 0; mask < 64; mask++) {
      const on = MOV.filter((_, i) => mask & (1 << i));
      const outputs = (mask & 32) === 0;
      const shown = (b: MovableButton) => on.includes(b);
      const L = playerLayout(barW, shown, outputs ? PLAYER.output3 : 0);
      if (L.mode === "three") {
        const inner = barW - PLAYER.border - PLAYER.pad3 * 2;
        assert.ok(Math.abs(PLAYER.border + PLAYER.pad3 * 2 + L.sideW * 2 + PLAYER.gap3 * 2 + L.centerW - barW) < 1e-9);
        assert.ok(Math.abs(L.centerW - Math.min(560, Math.max(240, inner * 0.4))) < 1e-9);
        const items = [
          ...(L.output > 0 ? [L.output] : []),
          ...L.right.map((b) => (b === "volume" ? PLAYER.volume : PLAYER.btn)),
          ...(L.more.length ? [PLAYER.more] : []),
        ];
        const rightW = items.reduce((a, b) => a + b, 0) + Math.max(0, items.length - 1) * PLAYER.btnGap;
        assert.ok(rightW <= L.sideW + 1e-9, `right ${W} ${mask}`);
        assert.equal(L.output, outputs ? PLAYER.output3 : 0);
        // 出しているボタンは、真ん中・右・「···」のどれか1か所だけ（全体の音量が「···」へ移っても）。隠したボタンはどこにも出さない
        const placed = [...(L.centerShuffle ? ["shuffle"] : []), ...(L.centerRepeat ? ["repeat"] : []), ...L.right, ...L.more];
        assert.deepEqual([...placed].sort(), [...on].sort(), `${W} ${mask}`);
        if (outputs && mask === 31 && firstThree < 0) firstThree = barW;
      } else {
        assert.equal(L.output, outputs ? PLAYER.output1 : 0);
        // 1段：左 ＝ 中の幅 − 中（前・再生・次）− 右（出力ボタン・「···」）− 間×2。ジャケット 48 ＋ 間 10 のあとに 120px
        const inner1 = barW - PLAYER.border - PLAYER.pad1 * 2;
        const center1 = PLAYER.btn * 2 + PLAYER.play + PLAYER.btnGap * 2;
        const rightParts = [...(L.output ? [L.output] : []), ...(L.more.length ? [PLAYER.more] : [])];
        const right1 = rightParts.reduce((a, b) => a + b, 0) + Math.max(0, rightParts.length - 1) * PLAYER.btnGap;
        const left1 = inner1 - center1 - right1 - PLAYER.gap1 * 2;
        assert.ok(left1 - PLAYER.art1 - 10 >= 120, `one ${W} ${mask} ${left1}`);
      }
    }
  }
  // 作者の決定：出力ボタン（112px）を出していても、全体の音量を「···」へ移して3列を保つので、3列は段1 と同じ再生バーの幅 728px（窓 748px）から。
  // 全体の音量が右の列に並ぶのは 1068px（窓 1088px）から（scripts\ui-layout.test.mts）
  assert.equal(firstThree, 728);
  assert.equal(playerLayout(727, () => true, PLAYER.output3).mode, "one");
  assert.equal(playerLayout(728, () => true, PLAYER.output3).mode, "three");
  assert.deepEqual(playerLayout(1067, () => true, PLAYER.output3).more[0], "volume");
  assert.deepEqual(playerLayout(1068, () => true, PLAYER.output3).right[0], "volume");
  assert.equal(playerLayout(728, () => true, 0).mode, "three");
});

test("出力ボタンは設定の「再生バーのボタン」で隠せる（全体の音量の前。古い設定では出す）。隠したら幅も取らない", () => {
  const ids = PLAYER_BUTTONS.map((b) => b.id) as string[];
  assert.equal(ids.indexOf("outputs") + 1, ids.indexOf("volume"));
  assert.equal(playerButtonShown({}, "outputs"), true);
  assert.equal(playerButtonShown({ playerButtons: { shuffle: false } }, "outputs"), true);
  assert.equal(playerButtonShown({ playerButtons: { outputs: false } }, "outputs"), false);
  const pb = read("src/layout/PlayerBar.tsx");
  assert.match(pb, /const outputsOn = shown\("outputs"\);/);
  assert.match(pb, /const L = playerLayout\(barW, shown, outputsOn \? PLAYER\.output3 : 0\);/);
  const right = pb.slice(pb.indexOf('<div className="player-right">'));
  assert.ok(right.indexOf("<OutputButton one={one} />") >= 0 && right.indexOf("<OutputButton") < right.indexOf("L.right.map"), "全体の音量の左");
  assert.match(pb, /\{L\.output > 0 \? <OutputButton one=\{one\} \/> : null\}/);
});

// ---------- 詳細の配置 ----------

const UIS = [{ compactList: "off", compactAuto: false }, { compactList: "always" }, { compactList: "off", compactAuto: true }];

test("詳細の配置：窓の幅 480〜2560px（10px ごと）・高さ 480〜1440px（10px ごと）・出力先 0〜3・サイドバー・流れる歌詞の列・コンパクトの全部で、はみ出し・重なりが無い", () => {
  let n = 0;
  for (let W = 480; W <= 2560; W += 10) {
    for (let H = 480; H <= 1440; H += 10) {
      for (const collapsed of [false, true]) {
        if (W < APP.sidebarAutoBelow && !collapsed) continue;
        for (let slots = 0; slots <= 3; slots++) {
          for (const lyricsRow of [true, false]) {
            for (const ui of UIS) {
              const D = DL({ winW: W, winH: H, sidebarCollapsed: collapsed, slots, lyricsRow, ui, outputsShown: true });
              const key = `${W}x${H} c${collapsed} n${slots} l${lyricsRow} ${JSON.stringify(ui)}`;
              const pad = W < 600 ? APP.padNarrow : APP.pad;
              const gap = W < 600 ? APP.gapNarrow : APP.gap;
              const side = collapsed || W < 1100 ? APP.sidebarCollapsedW : APP.sidebarW;
              assert.equal(D.centerW, W - pad * 2 - side - gap, key);
              const rowLyrics = lyricsRow && !D.hideLyricsRow;
              const area = H - pad * 2 - gap - D.playerH - (rowLyrics ? APP.lyricsRowH : 0);
              if (D.side) {
                // 左右：横の和が窓の幅にちょうど入る。曲一覧 360px 以上・ミキサー 35%（380〜520px）・縦長ではない
                assert.equal(pad * 2 + side + gap + D.listW + gap + D.mixerW, W, key);
                assert.ok(D.listW >= DETAIL.listMin, key);
                assert.ok(D.mixerW >= DETAIL.mixerMin && D.mixerW <= DETAIL.mixerMax, key);
                assert.equal(D.mixerW, mixerWidth(D.centerW), key);
                assert.ok(!(H > W), key);
                assert.equal(D.listH, area, key);
                assert.equal(D.mixerH, area, key);
              } else {
                // 上下：曲一覧とミキサーの縦の和が、再生バーより上の行にちょうど入る。曲一覧は隠さない（1行以上）。
                // ミキサーは見出しと1行（音量つまみと「…」）がいつも見える高さ以上、中身の高さ以下
                assert.ok(H > W || D.centerW - gap - mixerWidth(D.centerW) < DETAIL.listMin, key);
                assert.equal(D.listW, D.centerW, key);
                assert.equal(D.mixerW, D.centerW, key);
                assert.equal(D.listH + gap + D.mixerH, area, key);
                assert.ok(D.mixerH >= mixerMinHeight(slots), key);
                assert.ok(D.mixerH <= mixerContentHeight(slots), key);
                assert.ok(D.listRows >= 1, key);
              }
              // ミキサーの行の中身は、ミキサーの中に入る（横にはみ出さない）
              assert.ok(MIXER.stripMin <= D.mixerW - MIXER.border - MIXER.padX * 2, key);
              // 曲一覧の側の行の高さは、曲一覧の側の幅とコンパクトの設定で決まる（36・48・60）
              assert.ok([36, 48, 60].includes(D.rowH), key);
              assert.equal(D.listRows, Math.max(0, Math.floor((D.listH - LIST_CHROME) / D.rowH)), key);
              // 流れる歌詞の列：曲一覧が2行を切るときだけ隠す（設定を出していないときは隠すと言わない）
              if (D.hideLyricsRow) {
                assert.ok(lyricsRow, key);
              } else if (lyricsRow) {
                assert.ok(D.listRows >= 2, key);
              }
              n++;
            }
          }
        }
      }
    }
  }
  assert.ok(n > 300_000, String(n));
});

test("詳細：窓の幅を広げていくと、上下 → 左右へ1回だけ切り替わる（行ったり来たりしない）。縦長の窓では上下", () => {
  for (let H = 480; H <= 1440; H += 20) {
    for (const collapsed of [false, true]) {
      let seenSide = false;
      for (let W = 480; W <= 2560; W++) {
        const D = DL({ winW: W, winH: H, sidebarCollapsed: collapsed, slots: 3, lyricsRow: true, ui: {}, outputsShown: true });
        if (seenSide) assert.ok(D.side, `${W}x${H} ${collapsed}`);
        if (D.side) seenSide = true;
        if (H > W) assert.equal(D.side, false, `${W}x${H}`);
      }
    }
  }
  // サイドバーをたたんだ窓：中央の幅 ＝ 窓 − 94。曲一覧 360 ＋ 間 10 ＋ ミキサー 380 ＝ 750 から左右 → 窓の幅 844px から（高さが幅以下なら）
  assert.equal(DL({ winW: 843, winH: 600, sidebarCollapsed: true, slots: 1, lyricsRow: false, ui: {}, outputsShown: true }).side, false);
  assert.equal(DL({ winW: 844, winH: 600, sidebarCollapsed: true, slots: 1, lyricsRow: false, ui: {}, outputsShown: true }).side, true);
  // ミキサーの幅：35%、380〜520px
  assert.equal(mixerWidth(1000), 380);
  assert.equal(mixerWidth(1200), 420);
  assert.equal(mixerWidth(2000), 520);
  assert.equal(centerWidth(1920, false), 1920 - 20 - 236 - 10);
});

test("詳細：最小の窓（480×480）では上下。曲一覧に2行が残り、ミキサーは見出しと1行以上。流れる歌詞の列は自動で隠す（設定は変えない）", () => {
  const rows: string[] = [];
  for (let slots = 0; slots <= 3; slots++) {
    for (const lyricsRow of [true, false]) {
      const D = DL({ winW: 480, winH: 480, sidebarCollapsed: true, slots, lyricsRow, ui: {}, outputsShown: true });
      assert.equal(D.side, false);
      assert.equal(D.listW, 398);
      assert.equal(D.playerH, PLAYER.h1);
      assert.ok(D.listRows >= 2 && D.listRows <= 3, `${slots} ${lyricsRow} ${D.listRows}`);
      assert.ok(D.mixerH >= mixerMinHeight(slots));
      assert.equal(D.hideLyricsRow, lyricsRow, "480×480 では、流れる歌詞の列を出すと曲一覧が2行を切る");
      rows.push(`${slots}:${D.listRows}:${D.mixerH}:${D.mixerScrolls}`);
    }
  }
  // 出力先 3つ：ミキサーは 136px（見出しと1行半。残りの行はミキサーの中で送る）、曲一覧は2行
  const D3 = DL({ winW: 480, winH: 480, sidebarCollapsed: true, slots: 3, lyricsRow: true, ui: {}, outputsShown: true });
  assert.deepEqual([D3.mixerH, D3.listRows, D3.mixerScrolls], [136, 2, true]);
  // 出力先 1つ：ミキサーは中身の 124px（送らない）
  const D1 = DL({ winW: 480, winH: 480, sidebarCollapsed: true, slots: 1, lyricsRow: true, ui: {}, outputsShown: true });
  assert.deepEqual([D1.mixerH, D1.listRows, D1.mixerScrolls], [124, 2, false]);
  // 広い窓では流れる歌詞の列は隠さない
  assert.equal(DL({ winW: 1920, winH: 1040, sidebarCollapsed: false, slots: 3, lyricsRow: true, ui: {}, outputsShown: true }).hideLyricsRow, false);
});

test("詳細の画面（AppShell）：詳細のときだけミキサーを出し、.app に左右・上下の印とミキサーの幅・高さを入れる。流れる歌詞の列は detailLayout の決まりで隠す", () => {
  const sh = read("src/layout/AppShell.tsx");
  assert.match(sh, /const showRow = rowWanted && !\(D\?\.hideLyricsRow \?\? false\);/);
  assert.match(sh, /\(D \? \(D\.side \? " detail detail-side" : " detail detail-stack"\) : ""\)/);
  assert.match(sh, /\["--mixer-w" as any\]: `\$\{D\.mixerW\}px`, \["--mixer-h" as any\]: `\$\{D\.mixerH\}px`/);
  assert.match(sh, /\{D \? <Mixer scrolls=\{D\.mixerScrolls\} \/> : null\}/);
  assert.match(sh, /listPaneExitWidth\.set\(exitW\)/);
  // 格子：左右はミキサーを右の列、上下は曲一覧の下の行（サイドバーは2行ぶん、再生バーと流れる歌詞の列は3行目）
  assert.match(rule(".app.detail-side"), /grid-template-columns: var\(--sidebar-w\) minmax\(0, 1fr\) var\(--mixer-w\);/);
  assert.match(rule(".app.detail-side .mixer"), /grid-row: 1;\s*grid-column: 3;/);
  assert.match(rule(".app.detail-stack"), /grid-template-rows: minmax\(0, 1fr\) var\(--mixer-h\) var\(--player-h\);/);
  assert.match(rule(".app.detail-stack.lyrics-row-on"), /calc\(var\(--player-h\) \+ var\(--lyrics-row-h\)\)/);
  assert.match(rule(".app.detail-stack .sidebar"), /grid-row: 1 \/ 3;/);
  assert.match(css, /\n\.app\.detail-stack \.player,\n\.app\.detail-stack \.lyrics-row \{\s*grid-row: 3;/);
  // 再生バーと流れる歌詞の列は、3列のミキサーのときも左右いっぱい（1 / -1）
  assert.match(rule(".player"), /grid-column: 1 \/ -1;/);
});

// ---------- 上の帯のモード切替 ----------

test("モード切替：通常／コンパクト／詳細。詳細なら詳細に見える。詳細で「通常」「コンパクト」を押すと抜けて、詳細を抜けたあとの曲一覧の側の幅で設定を決める（仕様 5章・13.5 の11）", () => {
  assert.deepEqual(VIEW_MODES.map((m) => [m.id, m.label]), [["normal", "通常"], ["compact", "コンパクト"], ["detail", "詳細"]]);
  assert.equal(currentViewMode({ compactList: "always" }, 2000, true), "detail");
  assert.equal(currentViewMode({ compactList: "always" }, 2000, false), "compact");
  // 「詳細」を押す：表示の設定は変えない
  assert.deepEqual(pressMode({ compactList: "always" }, 800, 800, false, "detail"), { settings: null, detail: true });
  // 自動のコンパクトがオンで、詳細の左右で曲一覧だけが狭い（500px）。抜けたあとの幅（900px）では通常に見える → 「通常」を押しても自動はオフにしない
  const ui = { compactList: "off", compactAuto: true };
  assert.equal(currentViewMode(ui, 500, false), "compact");
  assert.deepEqual(pressMode(ui, 500, 900, true, "normal"), { settings: { compactList: "off", compactAuto: true }, detail: false });
  // 詳細でないとき（今の幅＝抜けたあとの幅）は、段1 と同じ（自動でコンパクトになっている幅で「通常」→ 自動をオフ）
  assert.deepEqual(pressMode(ui, 500, 500, false, "normal"), { settings: { compactList: "off", compactAuto: false }, detail: false });
  // どの組み合わせでも、押したあと（詳細なら抜けたあとの幅で）押したモードに見える
  for (const cl of ["off", "always", "narrow", undefined]) {
    for (const auto of [true, false, undefined]) {
      for (const detail of [true, false]) {
        for (const [lw, ew] of [
          [400, 400],
          [400, 900],
          [COMPACT_AUTO_BELOW - 1, COMPACT_AUTO_BELOW],
          [1500, 1900],
        ]) {
          for (const m of VIEW_MODES) {
            const u = { compactList: cl, compactAuto: auto };
            const r = pressMode(u, lw, ew, detail, m.id);
            const after = r.settings ? { ...u, ...r.settings } : u;
            const width = r.detail ? lw : detail ? ew : lw;
            assert.equal(currentViewMode(after, width, r.detail), m.id, JSON.stringify({ u, lw, ew, detail, m: m.id }));
          }
        }
      }
    }
  }
});

test("モード切替の画面（TopBar）：詳細かは Rust（viewModeSet）。押すと pressMode の値を保存し、詳細でなくなるなら viewModeSet(\"normal\")。詳細では歌詞の画面でも検索欄を出す", () => {
  const top = read("src/layout/TopBar.tsx");
  assert.match(top, /const cur = currentViewMode\(ui, props\.width, detail\);/);
  assert.match(top, /const next = pressMode\(ui, props\.width, listPaneExitWidth\.get\(\), detail, m\);/);
  assert.match(top, /if \(next\.detail !== detail\) api\.viewModeSet\(next\.detail \? "detail" : "normal"\)/);
  assert.match(top, /detail: \(\) => <IconViewDetail \/>,/);
  assert.match(top, /const searchable = r\.view !== "settings" && \(r\.view !== "lyrics" \|\| detail\);/);
  // 歌詞の画面で打ち始めたら「すべての曲」へ移って探す（打った語は持っていく）
  assert.match(top, /if \(route\.get\(\)\.view === "lyrics" && v\.trim\(\) !== ""\) \{\s*carry\.current = v;/);
  assert.match(top, /navigate\(\{ view: "all" \}\);/);
  // 3つ目のボタン（詳細 2字）の幅
  assert.equal(TOPBAR.switchLabeled, 2 + (20 + 16 + 6 + 2 * 16) + (20 + 16 + 6 + 5 * 16) + (20 + 16 + 6 + 2 * 16));
  assert.equal(TOPBAR.switchIcons, 2 + 3 * 36);
});

test("上の帯：曲一覧の側のいちばん狭い幅（詳細の左右の 360px）〜2600px のどこでも、検索欄の最低幅とファイル・フォルダ・3つのモード切替が入る", () => {
  const searchMin = Number(rule(".search").match(/min-width: (\d+)px;/)![1]);
  const fileFolderIcons = 2 * (12 * 2 + 2 + 16) + TOPBAR.actionsGap;
  for (let w = DETAIL.listMin; w <= 2600; w++) {
    const narrow = listLayoutFor(w).topbarNarrow;
    const labels = viewModeLabelsFit(w, narrow);
    const actions = (narrow ? fileFolderIcons : TOPBAR.fileFolderLabeled) + TOPBAR.actionsGap + (labels ? TOPBAR.switchLabeled : TOPBAR.switchIcons);
    const need = TOPBAR.frame + searchMin + TOPBAR.gap * 2 + actions;
    assert.ok(need <= w, `${w}: ${need}`);
  }
});

// ---------- メーター ----------

test("メーター：dB への換算と -60〜0 dB の位置。色の帯は -20 dB（黄）と -9 dB（赤）で切り替わる", () => {
  assert.equal(toDb(1), 0);
  assert.ok(Math.abs(toDb(0.1) - -20) < 1e-12);
  assert.equal(toDb(0), -Infinity);
  assert.equal(toDb(Number.NaN), -Infinity);
  assert.equal(toDb(-1), -Infinity);
  assert.equal(dbFraction(-Infinity), 0);
  assert.equal(dbFraction(-60), 0);
  assert.equal(dbFraction(-80), 0);
  assert.equal(dbFraction(0), 1);
  assert.equal(dbFraction(6), 1);
  assert.ok(Math.abs(BAND_AT.yellow - 40 / 60) < 1e-12);
  assert.ok(Math.abs(BAND_AT.red - 51 / 60) < 1e-12);
  assert.deepEqual([METER.greenTo, METER.yellowTo, METER.holdMs, METER.clipMs], [-20, -9, 20_000, 2_000]);
  assert.deepEqual([dbText(-Infinity), dbText(-12.34), dbText(-0.04), dbText(0.5)], ["-∞", "-12.3", "0.0", "+0.5"]);
  assert.equal(isClip(1), true);
  assert.equal(isClip(1.5), true);
  assert.equal(isClip(0.999), false);
  assert.equal(isClip(Number.NaN), false);
  // 画面（Meter.tsx）の帯の色は --c-ok・--c-favorite・--c-danger（新しい色の変数は足さない）
  const mt = read("src/components/Meter.tsx");
  assert.match(mt, /var\(--c-ok\) 0 \$\{BAND_AT\.yellow \* 100\}%, var\(--c-favorite\) \$\{BAND_AT\.yellow \* 100\}% \$\{BAND_AT\.red \* 100\}%, var\(--c-danger\)/);
  assert.match(rule(".meter-bar"), /box-shadow: inset 0 0 0 1px var\(--c-border\);/);
});

test("メーター：直前の最大の線は 20 秒残り、より大きい値で上がる。音割れの印は 0 dB に届いてから 2 秒。dB の数字は 0.1 秒ごとにその間の最大。鳴っていない出力は止める", () => {
  const st = newMeterState();
  let v = meterStep(st, [0.5, 0.1], true, 0);
  assert.ok(Math.abs(v.level[0] - dbFraction(toDb(0.5))) < 1e-12);
  assert.ok(Math.abs(v.hold[1] - dbFraction(-20)) < 1e-12);
  assert.equal(v.clip, false);
  assert.equal(v.text, dbText(toDb(0.5)));
  // 小さくなっても線は残る（20 秒まで）
  v = meterStep(st, [0.01, 0.01], true, 19_000);
  assert.ok(Math.abs(v.hold[0] - dbFraction(toDb(0.5))) < 1e-12);
  // 20 秒を過ぎたら、今の値へ下がる
  v = meterStep(st, [0.01, 0.01], true, 20_001);
  assert.ok(Math.abs(v.hold[0] - dbFraction(-40)) < 1e-9);
  // 音割れ：届いた時刻から 2 秒
  v = meterStep(st, [1, 0.2], true, 30_000);
  assert.equal(v.clip, true);
  assert.equal(meterStep(st, [0.1, 0.1], true, 31_999).clip, true);
  assert.equal(meterStep(st, [0.1, 0.1], true, 32_000).clip, false);
  // dB の数字：0.1 秒の間は書き換えず、その間の最大を次に出す
  const s2 = newMeterState();
  assert.equal(meterStep(s2, [0.1, 0.1], true, 0).text, "-20.0");
  assert.equal(meterStep(s2, [1, 0.1], true, 33).text, "-20.0");
  assert.equal(meterStep(s2, [0.01, 0.01], true, 66).text, "-20.0");
  assert.equal(meterStep(s2, [0.01, 0.01], true, 100).text, "0.0");
  // 鳴っていない（オフ・切断中など）：止めて線・印・数字も消す。値が来なくても同じ
  assert.deepEqual(meterStep(st, [0.9, 0.9], false, 40_000), IDLE_VIEW);
  assert.deepEqual(meterStep(st, undefined, true, 40_001), IDLE_VIEW);
  assert.equal(meterStep(st, [0, 0], true, 40_002).hold[0], -1, "止めたあとは線が無いところから");
});

test("メーターの帯（作者の決定・OBS と同じ動き）：上がるときはすぐ、下がるときは 40 dB / 1.7 秒（≒23.5 dB/秒）でゆっくり。今の値より下へは行かない。線と音割れは実際の値で決める", () => {
  assert.ok(Math.abs(METER.decayDbPerSec - 23.529) < 0.001);
  const st = newMeterState();
  assert.equal(meterStep(st, [1, 1], true, 0).level[0], 1);
  // 1 秒後に急に静かになっても、帯は 0 − 23.5 dB までしか下がらない
  let v = meterStep(st, [0.001, 0.001], true, 1000);
  assert.ok(Math.abs(v.level[0] - dbFraction(-METER.decayDbPerSec)) < 1e-9);
  // 1.7 秒で 40 dB 下がる
  v = meterStep(st, [0.001, 0.001], true, 1700);
  assert.ok(Math.abs(v.level[0] - dbFraction(-40)) < 1e-9);
  // 今の値（-60 dB）より下へは行かない
  v = meterStep(st, [0.001, 0.001], true, 5000);
  assert.ok(Math.abs(v.level[0] - dbFraction(-60)) < 1e-9);
  // 直前の最大の線は帯と関係なく実際の最大（0 dB）のまま。音割れは 0 から 2 秒で消えている
  assert.equal(v.hold[0], 1);
  assert.equal(v.clip, false);
  // 上がるときはすぐ
  v = meterStep(st, [0.5, 0.25], true, 5033);
  assert.ok(Math.abs(v.level[0] - dbFraction(toDb(0.5))) < 1e-9);
  assert.ok(Math.abs(v.level[1] - dbFraction(toDb(0.25))) < 1e-9);
  // 左右は別々に下がる
  v = meterStep(st, [0.001, 0.25], true, 5533);
  assert.ok(Math.abs(v.level[0] - dbFraction(toDb(0.5) - METER.decayDbPerSec * 0.5)) < 1e-9);
  assert.ok(Math.abs(v.level[1] - dbFraction(toDb(0.25))) < 1e-9);
  // 止めたら帯も消え、次は今の値から（前の値から下がってこない）
  meterStep(st, undefined, false, 6000);
  assert.ok(Math.abs(meterStep(st, [0.01, 0.01], true, 6033).level[0] - dbFraction(-40)) < 1e-9);
  // 描く側は値をそのまま書く（移り変わりの動きを足さない。動きを減らす設定でも動く）
  assert.ok(!/transition/.test(rule(".meter-fill")));
});

test("メーター（画面）：React の再描画を起こさない作り。形は1回だけ描き、受けるたびに参照から style と文字を直接書く。見ている間だけ meterStart、いなくなったら meterStop", () => {
  const mt = read("src/components/Meter.tsx");
  assert.ok(!/useState|useStore/.test(mt), "no React state per frame");
  assert.match(mt, /return subscribeMeter\(/);
  assert.match(mt, /\(el\.style as any\)\[key\] = v;/);
  assert.match(mt, /num\.current\.textContent = v\.text;/);
  assert.match(mt, /clip\.current\.hidden = !v\.clip;/);
  // 動きを減らす設定でも動かし続ける：メーターに移り変わりの動き（transition）を付けない
  for (const sel of [".meter-bar", ".meter-fill", ".meter-hold"]) assert.ok(!/transition/.test(rule(sel)), sel);
  const st = read("src/stores/outputs.ts");
  assert.match(st, /const want = listeners\.size > 0 && document\.visibilityState === "visible";/);
  // 開始・停止には通し番号（段2-2 検査1回目の指摘3）。開始の失敗は、あとから別の開始・停止をしていなければ試し直す
  assert.match(st, /api\.meterStart\(my, onFrame\)\.catch\(\(\) => \{\s*\/\/[^\n]*\n\s*if \(startSeq !== my\) return;/);
  assert.match(st, /retryTimer = window\.setTimeout\(sync, METER_RETRY_MS\);/);
  assert.match(st, /void api\.meterStop\(nextSeq\(\)\)/);
  assert.match(st, /const nextSeq = \(\) => \(seq = Math\.max\(seq \+ 1, Date\.now\(\) \* 1000\)\);/);
  assert.match(read("src/ipc/commands.ts"), /invoke<void>\("meter_start", \{ seq, onFrame: ch \}\)/);
  assert.match(read("src/ipc/commands.ts"), /invoke<void>\("meter_stop", \{ seq \}\)/);
  // Rust：いちばん大きい番号だけが効く（遅れて着いた古い開始・停止は何もしない。テストは commands\outputs.rs の meter_start_stop_order）
  const rs = read("src-tauri/src/commands/outputs.rs");
  assert.match(rs, /self\.gen\.fetch_max\(seq, Ordering::SeqCst\) < seq/);
  assert.match(rs, /if !state\.meter\.begin\(my\) \{\s*return Ok\(\(\)\);/);
  assert.match(rs, /pub async fn meter_stop\(state: State<'_, AppState>, seq: u64\)/);
  assert.match(st, /document\.addEventListener\("visibilitychange", sync\)/);
  // 値は枠ごとに1つだけ持ち、どの部品にも同じものを渡す（同じ出力のメーターは同じ値）
  assert.match(st, /const views = \[0, 1, 2\]\.map\(\(i\) => meterStep\(states\[i\], f\.peaks\[i\], slotSounding\(slots\[i\]\), now\)\);/);
  // メーターを出す場所：ミキサー（目盛りあり）とプルダウン（目盛りなし）
  assert.match(read("src/layout/Mixer.tsx"), /<MeterBars slot=\{s\.slot\} scale /);
  assert.match(read("src/layout/OutputButton.tsx"), /<MeterBars slot=\{s\.slot\} label=/);
});

// ---------- 出力ボタン・名前・デバイスの選び方 ----------

const slot = (n: number, o: Partial<Record<string, unknown>> = {}) =>
  ({
    slot: n,
    deviceId: `wasapi:{dev-${n}}`,
    savedName: `出力${n}の機器`,
    deviceName: `出力${n}の機器`,
    on: true,
    delayMs: 0,
    icon: null,
    autoIcon: "speaker",
    volume: 1,
    volumes: { normal: 1, detail: 1 },
    followMaster: true,
    status: "ok",
    reason: null,
    ...o,
  }) as any;

test("出力ボタン（4.3）：0（無い・全部オフ）は消音のアイコン＋「出力なし」、1 はアイコン＋名前、2〜3 はアイコンだけ。吹き出しと読み上げにはオンの名前を全部", () => {
  assert.deepEqual(outputButtonModel({ slots: [null, null, null] }), {
    kind: "none",
    icons: ["muted"],
    text: "出力なし",
    label: "出力先：出力先がありません",
    tip: "出力先がありません",
    alert: false,
    iconAlerts: [false],
  });
  const off = outputButtonModel({ slots: [slot(0, { on: false, status: "off" }), null, null] });
  assert.deepEqual([off.kind, off.icons, off.text], ["off", ["muted"], "出力なし"]);
  assert.match(off.label, /全部オフ/);
  const one = outputButtonModel({ slots: [slot(0, { autoIcon: "headphones" }), slot(1, { on: false, status: "off" }), null] });
  assert.deepEqual([one.kind, one.icons, one.text, one.label], ["one", ["headphones"], "出力0の機器", "出力先：出力0の機器"]);
  const many = outputButtonModel({ slots: [slot(0), null, slot(2, { icon: "stream", status: "disconnected" })] });
  assert.deepEqual([many.kind, many.icons, many.text], ["many", ["speaker", "stream"], null]);
  assert.equal(many.label, "出力先：出力0の機器、出力2の機器（切断中）");
  assert.equal(many.tip, "出力0の機器\n出力2の機器（切断中）");
  // 画面：固定の幅・1段は文字を出さない・吹き出しは上・読み上げは aria-label で（吹き出しの文を二重に読ませない）
  const ob = read("src/layout/OutputButton.tsx");
  assert.match(ob, /const showText = !props\.one && m\.text !== null;/);
  assert.match(ob, /<Tip text=\{m\.tip\} delay=\{300\} place="above" className="inline output-btn-tip" describe=\{false\}>/);
  assert.match(ob, /aria-label=\{m\.label\}\s*aria-haspopup="dialog"\s*aria-expanded=\{open\}/);
  // プルダウン：上に開く。出力先が無いときは「出力先がありません」。下の段（詳細を開く・設定を開く）は置かない
  assert.match(ob, /<Popover anchor=\{btn\} place="above" label="出力先"/);
  assert.match(ob, /出力先がありません/);
  assert.ok(!/詳細を開く|配信モードを開く|設定を開く/.test(ob));
});

test("名前と状態（6.4）：既定のデバイス・前に使っていたデバイス・切断中・開けません（理由3つ）・重複。色だけで伝えず文字を出す", () => {
  assert.equal(slotName(slot(0, { deviceId: null, deviceName: "どれかの機器" })), "既定のデバイス");
  assert.match(slotTitle(slot(0, { deviceId: null, deviceName: "どれかの機器" })), /今は どれかの機器/);
  assert.equal(slotName(slot(0, { deviceName: null, savedName: "" })), UNKNOWN_DEVICE_NAME);
  assert.equal(slotName(slot(0, { deviceName: null, savedName: "覚えていた名前" })), "覚えていた名前");
  assert.deepEqual(STATUS_TEXT, { disconnected: "切断中", unavailable: "開けません", duplicate: "重複" });
  assert.equal(statusHint({ status: "unavailable", reason: "format" }), "このデバイスの形式は使えません");
  assert.equal(statusHint({ status: "unavailable", reason: "busy" }), "ほかのアプリが独り占めしています");
  assert.equal(statusHint({ status: "unavailable", reason: "other" }), "このデバイスを開けませんでした");
  assert.match(statusHint({ status: "duplicate", reason: null })!, /ほかの出力先と同じデバイス/);
  assert.equal(statusHint({ status: "ok", reason: null }), null);
  assert.equal(pct(0.456), "46%");
  // オフは名前を薄い文字色に（opacity で薄くしない）。状態の文字は危険の色
  assert.match(css, /\n\.mixer-row\.off \.output-name,\n\.output-pop-row\.off \.output-name,\n\.mixer-row \.icon-btn\.is-off \{\s*color: var\(--c-secondary\);/);
  assert.ok(!/opacity/.test(rule(".mixer-row")));
  assert.match(css, /\n\.output-status,\n\.meter-clip \{[^}]*color: var\(--c-danger\);/);
});

test("デバイスの選び方（2.1・6.2・7章）：先頭は既定のデバイス。ほかの出力先で使っているものは出さない（比べるのは ID）。差し替えでは自分の今のデバイスは出す", () => {
  const devs = [
    { id: "wasapi:{dev-0}", name: "機器A", isDefault: true, kind: "speaker" },
    { id: "wasapi:{dev-1}", name: "機器B", isDefault: false, kind: "headphones" },
    { id: "wasapi:{dev-9}", name: "機器A", isDefault: false, kind: "speaker" },
  ] as any;
  const v = { slots: [slot(0), slot(1, { deviceId: null }), null] };
  // 足す：dev-0（枠0）と既定（枠1）は出さない。同じ名前の別の機器（dev-9）は出す
  assert.deepEqual(deviceChoices(devs, v, null).map((c) => c.id), ["wasapi:{dev-1}", "wasapi:{dev-9}"]);
  // 枠0を差し替える：自分の dev-0 は出す。既定（枠1が使用）は出さない
  assert.deepEqual(deviceChoices(devs, v, 0).map((c) => c.id), ["wasapi:{dev-0}", "wasapi:{dev-1}", "wasapi:{dev-9}"]);
  // 枠1を差し替える：既定（自分）は出す。dev-0 は出さない
  assert.deepEqual(deviceChoices(devs, v, 1).map((c) => c.id), [null, "wasapi:{dev-1}", "wasapi:{dev-9}"]);
  assert.equal(canAddOutput(v), true);
  assert.equal(canAddOutput({ slots: [slot(0), slot(1), slot(2)] }), false);
  // ミキサー：3つあるときは押せない（理由の吹き出し「出力先は3つまで」）
  const mx = read("src/layout/Mixer.tsx");
  assert.match(mx, /<Tip text=\{can \? null : "出力先は3つまで"\}/);
  assert.match(mx, /aria-disabled=\{can \? undefined : true\}/);
  // 「…」の中身：遅らせ量（0〜1000）・全体の音量に従う・アイコン・デバイスの差し替え・外す（確認なし）
  for (const s of ["遅らせ量", "ミリ秒（0〜1000）", "映像に合わせるものではありません", "全体の音量に従う", "アイコン", "デバイスを差し替える…", "この出力先を外す"]) {
    assert.ok(mx.includes(s), s);
  }
  assert.ok(!/confirmDialog/.test(mx), "外すときは確認なし（6.3）");
  // オン/オフのスピーカーの印の吹き出しは「この出力をオフにする」（2.4）
  assert.match(mx, /title=\{s\.on \? "この出力をオフにする" : "この出力をオンにする"\}/);
});

test("設定画面（7章）：再生デバイスの欄は、出力先のデバイスを選ぶだけ（outputSetDevice）。既定へ移る説明と device_fallback の知らせは無い。ミュートのキーは設定から（2.4）", () => {
  const sv = read("src/views/SettingsView.tsx");
  assert.match(sv, /function DeviceField\(\)/);
  assert.match(sv, /api\.outputSetDevice\(cur\.slot, d\.id\)/);
  assert.match(sv, /deviceChoices\(list, v, cur\.slot\)/);
  assert.ok(!/切断されたら既定のデバイスで続けます/.test(sv));
  assert.match(sv, /既定のデバイスへは移りません/);
  assert.ok(!/api\.setDevice\(/.test(sv));
  const ja = read("src/messages/ja.ts");
  assert.ok(!/case "device_fallback"/.test(ja));
  for (const code of ["output_returned", "outputs_all_lost", "sync_unavailable", "outputs_imported"]) assert.ok(ja.includes(`case "${code}"`), code);
  const vc = read("src/components/VolumeControl.tsx");
  assert.ok(!/\(M\)/.test(vc));
  assert.match(vc, /comboLabel\(useStore\(settings\)\.shortcuts\.mute \?\? ""\)/);
  // ショートカット（7章）：詳細表示の切替・出力1〜3 のオン/オフ（既定のキーは空）
  const sc = read("src/shortcuts/shortcuts.ts");
  for (const a of ["toggleDetail", "toggleOutput1", "toggleOutput2", "toggleOutput3"]) {
    assert.match(sc, new RegExp(`${a}: "${a}",`));
    assert.match(read("src/stores/settings.ts"), new RegExp(`${a}: "",`));
  }
  assert.match(ja, /toggleDetail: "詳細表示の切替",/);
});

// ---------- 段2-2 の直し1回目（検査1回目の指摘） ----------

test("指摘2：出力先が1つで切断中・開けません・重複なら、ボタンの見える文字が状態になる（名前は吹き出しと読み上げ）。鳴らせない出力があれば「!」の印と危険の色の縁", () => {
  for (const [st, word] of [
    ["disconnected", "切断中"],
    ["unavailable", "開けません"],
    ["duplicate", "重複"],
  ] as const) {
    const m = outputButtonModel({ slots: [slot(0, { status: st }), null, null] });
    assert.deepEqual([m.kind, m.text, m.alert], ["one", word, true], st);
    assert.equal(m.label, `出力先：出力0の機器（${word}）`);
  }
  assert.equal(outputButtonModel({ slots: [slot(0), null, null] }).alert, false);
  assert.equal(outputButtonModel({ slots: [slot(0), slot(1, { status: "duplicate" }), null] }).alert, true);
  const ob = read("src/layout/OutputButton.tsx");
  assert.match(ob, /\(m\.alert \? " alert" : ""\)/);
  // 危険の色は縁だけ（文字は --c-primary の太字。ボタンの面の上の --c-danger の文字は Dark の明るい背景画像で 4.5 に届かない）
  assert.match(rule(".output-btn.alert"), /border-color: var\(--c-danger\);/);
  assert.ok(!/color: var\(--c-danger\)/.test(rule(".output-btn.alert .output-btn-text")));
});

// ---------- 段2-2 の直し2回目（検査2回目の N1・N2） ----------

test("N1：鳴らせない出力のアイコンの角に「!」を重ねる（1段でアイコンが3つ並ぶときも。幅は増やさない）。読み上げ・吹き出しは今のまま", () => {
  const many = outputButtonModel({ slots: [slot(0), slot(1, { status: "unavailable" }), slot(2, { status: "disconnected" })] });
  assert.deepEqual([many.icons.length, many.iconAlerts], [3, [false, true, true]]);
  assert.equal(many.label, "出力先：出力0の機器、出力1の機器（開けません）、出力2の機器（切断中）");
  assert.deepEqual(outputButtonModel({ slots: [slot(0), slot(1), null] }).iconAlerts, [false, false]);
  assert.deepEqual(outputButtonModel({ slots: [slot(0, { status: "duplicate" }), null, null] }).iconAlerts, [true]);
  assert.deepEqual(outputButtonModel({ slots: [slot(0, { on: false, status: "off" }), null, null] }).iconAlerts, [false]);
  const ob = read("src/layout/OutputButton.tsx");
  assert.match(ob, /\{m\.iconAlerts\[i\] \? <span className="output-icon-mark">!<\/span> : null\}/);
  assert.ok(!/showMark|output-btn-mark/.test(ob), "別の場所に置く「!」（幅の都合で出せない形があった）はやめた");
  // 印はアイコンの枠（20px・position: relative）の中に重ねる（絶対の位置。並びの幅を増やさない）
  assert.match(rule(".output-icon"), /position: relative;/);
  const mk = rule(".output-icon-mark");
  assert.match(mk, /position: absolute;/);
  assert.match(mk, /background: rgb\(var\(--c-panel-rgb\)\);/);
  assert.match(mk, /color: var\(--c-primary\);/);
  // 1段：アイコン3つ（20×3 ＋ 間 2×2）＋ 枠 2 ＋ 余白 6×2 ＝ 78px は、印を重ねても 80px のまま
  assert.ok(2 + 12 + 20 * 3 + 2 * 2 <= PLAYER.output1);
  // 印がボタンの外に出ない：はみ出しは 3px で、アイコンの外の余白（1段 6px・3列 8px、上下は (40 − 枠 2 − 20) / 2 ＝ 9px）より小さい
  const out = Number(mk.match(/right: -(\d+)px;/)![1]);
  assert.equal(Number(mk.match(/top: -(\d+)px;/)![1]), out);
  assert.ok(out <= 6 && out <= (PLAYER.outputH - 2 - 20) / 2);
  // 印の文字（--c-primary）と地（パネルの色）の組み合わせは scripts\theme-contrast.mts で 4.5:1 を確かめる
  assert.ok(read("scripts/theme-contrast.mts").includes("出力ボタンの「!」 --c-primary／印の地（パネルの色）"));
});

test("N2：3列で出力先が1つだけのとき、状態の文字（切断中・開けません・重複）は、アイコンを外したボタンの中の幅いっぱい（94px）に入る。スキンで文字 +2px・太字でも「…」で切れない", () => {
  const ob = read("src/layout/OutputButton.tsx");
  assert.match(ob, /const stateOnly = showText && m\.kind === "one" && m\.alert;/);
  assert.match(ob, /\{stateOnly \? null : \(\s*<span className="output-icons" aria-hidden="true">/);
  // 中の幅：112 − 枠 1×2 − 余白 8×2
  assert.match(rule(".output-btn"), /padding: 0 8px;/);
  const inner = PLAYER.output3 - 2 - 16;
  assert.equal(inner, 94);
  // 文字の大きさはボタンが受け継ぐ --fs-body（14px。スキンで ±2px まで）。日本語は太字でも1字が文字の大きさと同じ幅
  assert.ok(!/font-size/.test(rule(".output-btn")) && !/font-size/.test(rule(".output-btn-text")));
  assert.match(tokens, /--fs-body: 14px;/);
  const fs = 14 + 2;
  for (const word of Object.values(STATUS_TEXT)) {
    assert.ok([...word!].length * fs <= inner, `${word}: ${[...word!].length * fs} > ${inner}`);
  }
  // いちばん長い「開けません」は 5字 × 16 ＝ 80px（94px に 14px の余り）。アイコンを残すと 94 − 20 − 6 ＝ 68px で入らなかった（検査2回目の N2）
  assert.equal(Math.max(...Object.values(STATUS_TEXT).map((w) => [...w!].length)) * fs, 80);
  assert.ok(5 * fs > inner - 20 - 6);
  // 名前のとき（鳴っている・1つ）はアイコンを残す
  const okOne = outputButtonModel({ slots: [slot(0), null, null] });
  assert.deepEqual([okOne.kind, okOne.alert, okOne.text], ["one", false, "出力0の機器"]);
});

test("指摘1・6：出力ごとの音量つまみは、保存を待っている間に消えても保存を送る。ホイールは passive でない受け口で止めて、枠を送らない（全体の音量のつまみも消えるときに保存）", () => {
  const ob = read("src/layout/OutputButton.tsx");
  assert.match(ob, /\(\) => \(\) => \{\s*if \(timer\.current === undefined\) return;\s*window\.clearTimeout\(timer\.current\);\s*timer\.current = undefined;\s*void api\.outputCommitVolume\(\)/);
  assert.match(ob, /timer\.current = undefined;\s*setLocal\(null\);\s*void api\.outputCommitVolume\(\)/);
  assert.match(ob, /el\.addEventListener\("wheel", on, \{ passive: false \}\);/);
  assert.match(ob, /e\.preventDefault\(\);\s*wheel\.current\(e\.deltaY\);/);
  assert.ok(!/onWheel=/.test(ob));
  const vc = read("src/components/VolumeControl.tsx");
  assert.match(vc, /\(\) => \(\) => \{\s*if \(timer\.current === undefined\) return;\s*window\.clearTimeout\(timer\.current\);\s*timer\.current = undefined;\s*void api\.commitVolume\(\);/);
  assert.equal((vc.match(/timer\.current = undefined;\s*setLocal\(null\);/g) ?? []).length, 2);
});

test("指摘4・10：デバイスの変化の知らせは、一覧を見ている部品がある間だけ受ける。起動時は先に知らせを受け始めてから今の全部を引く（引いている間に来た変化を、古い結果で上書きしない）", () => {
  const st = read("src/stores/outputs.ts");
  assert.match(st, /export function watchDevices\(\): \(\) => void/);
  assert.match(st, /if \(deviceUsers > 0 \|\| !deviceUnlisten\) return;[\s\S]*void p\.then\(\(un\) => un\(\)\);/);
  assert.match(read("src/layout/Mixer.tsx"), /useEffect\(\(\) => watchDevices\(\), \[\]\);/);
  assert.match(read("src/views/SettingsView.tsx"), /useEffect\(\(\) => watchDevices\(\), \[\]\);/);
  const init = st.slice(st.indexOf("export async function initOutputs"), st.indexOf("// ---------- 再生デバイスの一覧"));
  assert.ok(init.indexOf('await on("outputs"') < init.indexOf("await api.outputsGet()"), "知らせを先に");
  assert.match(init, /if \(!pushed\) outputs\.set\(v\);/);
});

test("指摘5・7・11：遅らせ量の欄は打っている間に戻りの値で書き換えない。オン/オフの読み上げの名前は固定で、状態は aria-pressed。小窓の最初の操作で Shift+Tab を押すとボタンへ戻る", () => {
  const mx = read("src/layout/Mixer.tsx");
  assert.match(mx, /useEffect\(\(\) => \{\s*if \(document\.activeElement === input\.current\) return;\s*setText\(String\(props\.s\.delayMs\)\);\s*\}, \[props\.s\.delayMs\]\);/);
  assert.match(mx, /const go = \(\) => \{\s*if \(now\) setText\(String\(ms\)\);/);
  assert.match(mx, /aria-label=\{`\$\{name\}：オン`\}\s*aria-pressed=\{s\.on\}/);
  assert.ok(!/をオフにする`|をオンにする`/.test(mx));
  const pop = read("src/components/Popover.tsx");
  assert.match(pop, /if \(e\.key !== "Tab" \|\| !e\.shiftKey\) return;/);
  assert.match(pop, /if \(first && document\.activeElement === first\) \{\s*e\.preventDefault\(\);\s*close\.current\(true\);/);
});

test("指摘8：直前の最大の線は --c-primary にパネルの色の 1px の縁（帯の色の上でも溝の上でも見える。組み合わせは theme-contrast で計算）", () => {
  const h = rule(".meter-hold");
  assert.match(h, /background: var\(--c-primary\);/);
  assert.match(h, /box-shadow: 0 0 0 1px rgb\(var\(--c-panel-rgb\)\);/);
  const tc = read("scripts/theme-contrast.mts");
  for (const n of ["最大の線 --c-primary／溝の中（パネル）", "最大の線 --c-primary／線の縁（パネルの色）", "最大の線の縁／黄の帯 --c-favorite"]) assert.ok(tc.includes(n), n);
});

test("指摘9：ミキサーの上の段は、いちばん混む場合（380px のミキサー・「個別」・「開けません」・dB・スキンで文字 +2px）でも名前に 48px が残り、ボタンの列と重ならない", () => {
  assert.match(rule(".mixer-row-top"), /overflow: hidden;/);
  assert.match(rule(".mixer-row-top .output-name"), new RegExp(`min-width: ${MIXER.nameMin}px;`));
  assert.match(rule(".meter-db"), new RegExp(`width: ${MIXER.dbW}px;`));
  assert.match(rule(".mixer-readout"), /margin-left: auto;/);
  assert.match(rule(".mixer-row"), new RegExp(`column-gap: ${MIXER.colGap}px;`));
  assert.match(rule(".mixer-row-top"), new RegExp(`gap: ${MIXER.topGap}px;`));
  assert.match(rule(".mixer-row-buttons"), /gap: 2px;/);
  assert.ok(!read("src/layout/Mixer.tsx").includes("mixer-row-gap"), "間を埋める要素を置かない（間が1つ増える）");
  const fs = 12 + 2; // --fs-small ＋ スキンの上限 2px。日本語は1字が文字の大きさと同じ幅
  const badge = 2 * fs + 6 * 2 + 2; // 「個別」・余白 6×2・枠 1×2
  const status = 5 * fs; // 「開けません」（いちばん長い状態）
  const clip = 3 * fs; // 「音割れ」。鳴っている（状態の文字が無い）ときだけ出る
  const top = DETAIL.mixerMin - MIXER.border - MIXER.padX * 2 - MIXER.rowPadX * 2 - MIXER.buttons - MIXER.colGap;
  // 状態の文字があるとき：アイコン・名前・「個別」・状態・dB（間 4つ）
  assert.ok(top - (MIXER.icon + badge + status + MIXER.dbW + MIXER.topGap * 4) >= MIXER.nameMin);
  // 音割れの印があるとき：アイコン・名前・「個別」・（音割れ＋間＋dB）（間 3つ）
  assert.ok(top - (MIXER.icon + badge + clip + MIXER.topGap + MIXER.dbW + MIXER.topGap * 3) >= MIXER.nameMin);
  // dB の数字の欄に、いちばん長い「-60.0 dB」「+12.3 dB」（8字。数字と記号は文字の大きさの 0.6 倍ほど）が入る。-100 dB より下は「-∞」
  assert.equal(dbText(-99.94).length + " dB".length, 8);
  assert.equal(dbText(-100.5), "-∞");
  assert.ok(8 * fs * 0.6 <= MIXER.dbW);
});
