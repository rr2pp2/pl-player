// v3.8.0：遅れを測る（仕様「同時再生と配信モード」2.3 ）の画面の側を確かめる。
//   - 入口のボタンを押せるか（流している間・鳴らせる出力先が2つ未満は押せず、理由を出す）
//   - 結果の1行に出すもの（届くまでの遅れ・入れる値・測れませんでした：理由）、「この値を入れる」で送るもの
//   - 録る所の選び方（自動の印・選び直しだけを送る）
//   - 理由の文が Rust の種類と1対1（足し忘れが無い）・画面の組み立て（ボタン2か所・小窓・閉じたらやめる）
// 走らせ方（プロジェクトのフォルダで。依存は要らない。Node 24 の型の取り外しで .ts をそのまま読む）：
//   node --test scripts/ui-latency.test.mts
// 例の名前は架空。実在の曲名・アーティスト名・機器の製品名・ユーザー名は入れない。
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  ABORT_TEXT,
  applyItems,
  captureChoices,
  captureFromKey,
  captureKey,
  captureName,
  captureOptions,
  FAIL_TEXT,
  LATENCY_NOT_ENOUGH,
  LATENCY_PLAYING,
  LATENCY_TEXT,
  latencyBlocked,
  latencyRowModel,
  LOOPBACK_LABEL,
  msText,
  reportNotes,
  selectedCapture,
  skipText,
} from "../src/layout/latency.ts";
import type { LatencyReport, LatencyRow, OutputSlotView, OutputStatus } from "../src/ipc/types.ts";

const read = (rel: string) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");

function slot(n: number, o: Partial<OutputSlotView> = {}): OutputSlotView {
  return {
    slot: n,
    deviceId: `dev-${n}`,
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
  };
}

function row(o: Partial<LatencyRow> = {}): LatencyRow {
  return {
    slot: 0,
    deviceId: "dev-0",
    name: "架空のヘッドホン",
    status: "ok",
    measured: true,
    capture: { kind: "loopback" },
    arrivalMs: 0.4,
    spreadMs: 0.3,
    hits: 6,
    fillMs: 33,
    delayMs: 0,
    fail: null,
    ...o,
  };
}

test("入口を押せるか：流している間は押せない。鳴らせるオンの出力先が2つ未満でも押せない（オフ・切断中などは数えない）", () => {
  const two = { slots: [slot(0), slot(1), null] };
  assert.equal(latencyBlocked(two, false), null);
  assert.equal(latencyBlocked(two, true), LATENCY_PLAYING);
  assert.match(LATENCY_PLAYING, /流している間は測れません/);
  assert.equal(latencyBlocked({ slots: [slot(0), null, null] }, false), LATENCY_NOT_ENOUGH);
  for (const st of ["off", "disconnected", "unavailable", "duplicate"] as OutputStatus[]) {
    const v = { slots: [slot(0), slot(1, { status: st, on: st !== "off" }), null] };
    assert.equal(latencyBlocked(v, false), LATENCY_NOT_ENOUGH, st);
  }
  assert.equal(latencyBlocked({ slots: [null, null, null] }, false), LATENCY_NOT_ENOUGH);
  // 流しているときは、出力先の数より先に「流している」を出す
  assert.equal(latencyBlocked({ slots: [null, null, null] }, true), LATENCY_PLAYING);
});

test("結果の1行：届くまでの遅れ（小数1桁）と入れる値（今の値も）。今と同じなら「今と同じ」。ばらつきが 0.05ms 未満なら出さない", () => {
  const m = latencyRowModel(row({ arrivalMs: 0.42, spreadMs: 0.31, fillMs: 33, delayMs: 12 }));
  assert.equal(m.arrival, "0.4 ミリ秒（ばらつき 0.3 ミリ秒）");
  assert.equal(m.fill, "33 ミリ秒（今は 12 ミリ秒）");
  assert.equal(m.note, null);
  assert.equal(m.failed, false);
  assert.equal(latencyRowModel(row({ fillMs: 0, delayMs: 0, spreadMs: 0.01 })).fill, "0 ミリ秒（今と同じ）");
  assert.equal(latencyRowModel(row({ spreadMs: 0.01 })).arrival, "0.4 ミリ秒");
  assert.equal(msText(-0.04), "0.0 ミリ秒");
  assert.equal(msText(33.45), "33.5 ミリ秒");
});

test("測れなかった出力：「測れませんでした：理由」（色だけで伝えない）。値は出さない。測らない出力（オフなど）は理由の文だけ", () => {
  const f = latencyRowModel(row({ fail: "no_signal", arrivalMs: null, fillMs: null }));
  assert.equal(f.failed, true);
  assert.equal(f.fill, null);
  assert.equal(f.arrival, null);
  assert.equal(f.note, `測れませんでした：${FAIL_TEXT.no_signal}`);
  const cap = latencyRowModel(row({ fail: "capture_missing", arrivalMs: null, fillMs: null }));
  assert.match(cap.note!, /^測れませんでした：録る所のデバイスが見つかりません/);
  const off = latencyRowModel(row({ measured: false, status: "off", arrivalMs: null, fillMs: null, capture: null }));
  assert.deepEqual([off.note, off.failed, off.fill], [skipText("off"), false, null]);
  assert.equal(skipText("disconnected"), "切断中なので測れません");
  // 測れたが揃える相手がいない（もう片方が測れなかった）
  const alone = latencyRowModel(row({ fillMs: null }));
  assert.equal(alone.fill, null);
  assert.match(alone.note!, /揃える相手が測れなかった/);
});

test("「この値を入れる」で送るのは、値のある出力だけ（測ったときのデバイスの設定つき）。やめた結果では送らない。添える文", () => {
  const rep: LatencyReport = {
    abort: null,
    rows: [
      row({ slot: 0, deviceId: "dev-0", fillMs: 33 }),
      row({ slot: 1, deviceId: null, fillMs: 0 }),
      row({ slot: 2, deviceId: "dev-2", fail: "weak", arrivalMs: null, fillMs: null }),
    ],
  };
  assert.deepEqual(applyItems(rep), [
    { slot: 0, deviceId: "dev-0", delayMs: 33 },
    { slot: 1, deviceId: null, delayMs: 0 },
  ]);
  assert.deepEqual(reportNotes(rep), ["測れなかった出力先は揃える相手に入っていません。その出力先の遅らせ量は変えません。"]);
  assert.deepEqual(applyItems({ ...rep, abort: "playing" }), []);
  assert.deepEqual(reportNotes({ ...rep, abort: "playing" }), []);
  const none: LatencyReport = { abort: null, rows: [row({ fillMs: null }), row({ slot: 1, fail: "gap", arrivalMs: null, fillMs: null })] };
  assert.deepEqual(applyItems(none), []);
  assert.match(reportNotes(none).join(""), /入れる値はありません/);
});

test("録る所：名前から選んだものに「（自動）」。先頭はループバック。自動の録音デバイスが一覧に無ければ足す。選び直しだけを送る", () => {
  const inputs = [
    { id: "in-1", name: "架空のマイク" },
    { id: "in-2", name: "CABLE Output (仮想ケーブル)" },
  ];
  const opts = captureOptions({ kind: "input", id: "in-2" }, inputs);
  assert.deepEqual(
    opts.map((o) => o.label),
    [LOOPBACK_LABEL, "録音：架空のマイク", "録音：CABLE Output (仮想ケーブル)（自動）"],
  );
  assert.equal(captureOptions({ kind: "loopback" }, inputs)[0].label, `${LOOPBACK_LABEL}（自動）`);
  const gone = captureOptions({ kind: "input", id: "in-x" }, inputs);
  assert.equal(gone.length, 4);
  assert.match(gone[3].label, /見つかりません.*（自動）/);
  for (const c of [{ kind: "loopback" as const }, { kind: "input" as const, id: "in:with:colons" }]) assert.deepEqual(captureFromKey(captureKey(c)), c);
  assert.equal(captureName(null, inputs), LOOPBACK_LABEL);
  assert.equal(captureName({ kind: "input", id: "in-1" }, inputs), "架空のマイク");
  const rows = [
    { slot: 0, name: "架空のヘッドホン", status: "ok" as OutputStatus, measurable: true, autoCapture: { kind: "loopback" as const }, delayMs: 0 },
    { slot: 2, name: "CABLE Input (仮想ケーブル)", status: "ok" as OutputStatus, measurable: true, autoCapture: { kind: "input" as const, id: "in-2" }, delayMs: 0 },
    { slot: 1, name: "架空のスピーカー", status: "off" as OutputStatus, measurable: false, autoCapture: { kind: "loopback" as const }, delayMs: 0 },
  ];
  assert.deepEqual(captureChoices(rows, {}, inputs), []);
  assert.deepEqual(captureChoices(rows, { "架空のヘッドホン": "loopback", "CABLE Input (仮想ケーブル)": "loopback", "架空のスピーカー": "input:in-1" }, inputs), [
    { slot: 2, capture: { kind: "loopback" } },
  ]);
});

test("選び直した録音デバイスが今の一覧に無い（抜いた）なら、選び直しを捨てて自動に戻す。見た目と送る値が食い違わない", () => {
  const inputs = [{ id: "in-1", name: "架空のマイク" }];
  const auto = { kind: "loopback" as const };
  assert.equal(selectedCapture(auto, inputs, undefined), "loopback");
  assert.equal(selectedCapture(auto, inputs, "input:in-1"), "input:in-1");
  assert.equal(selectedCapture(auto, inputs, "input:gone"), "loopback");
  // 自動が録音デバイスなら、抜いた選び直しは自動（その録音デバイス）に戻る
  assert.equal(selectedCapture({ kind: "input", id: "in-1" }, inputs, "input:gone"), "input:in-1");
  const rows = [{ slot: 0, name: "架空のヘッドホン", status: "ok" as OutputStatus, measurable: true, autoCapture: auto, delayMs: 0 }];
  assert.deepEqual(captureChoices(rows, { "架空のヘッドホン": "input:gone" }, inputs), []);
  assert.deepEqual(captureChoices(rows, { "架空のヘッドホン": "input:in-1" }, inputs), [{ slot: 0, capture: { kind: "input", id: "in-1" } }]);
  // 小窓：一覧を引いたら、一覧に無い選び直しを覚えからも消す。select の値と送る値は同じ決まり（selectedCapture）
  const dlg = read("src/layout/LatencyDialog.tsx");
  assert.match(dlg, /selectedCapture\(r\.autoCapture, s\.inputs, k\) !== k\) delete picked\[r\.name\]/);
  assert.match(dlg, /value=\{selectedCapture\(r\.autoCapture, setup\.inputs, choice\[r\.name\]\)\}/);
  assert.match(dlg, /captureChoices\(setup\.rows, choice, setup\.inputs\)/);
});

test("録る所を開けないときは、Windows のマイクへのアクセスも確かめるよう添える。Windows が断ったとき（capture_denied）は専用の文", () => {
  assert.match(FAIL_TEXT.capture_open, /マイクへのアクセス/);
  assert.match(FAIL_TEXT.capture_denied, /マイクへのアクセス/);
  assert.match(read("src-tauri/src/audio/latency/run.rs"), /cpal::ErrorKind::PermissionDenied => FailCode::CaptureDenied/);
});

test("理由の文は Rust の種類（FailCode・AbortCode）と1対1。型（types.ts）とも同じ", () => {
  const snake = (s: string) => s.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
  const variants = (src: string, name: string) => {
    const at = src.indexOf(`pub enum ${name} {`);
    assert.ok(at >= 0, name);
    const body = src.slice(at, src.indexOf("\n}", at));
    return [...body.matchAll(/^\s{4}([A-Z][A-Za-z]+),?$/gm)].map((m) => snake(m[1])).sort();
  };
  const fail = variants(read("src-tauri/src/audio/latency/mod.rs"), "FailCode");
  assert.ok(fail.length >= 15);
  assert.deepEqual(Object.keys(FAIL_TEXT).sort(), fail);
  const abort = variants(read("src-tauri/src/commands/latency.rs"), "AbortCode");
  assert.deepEqual(Object.keys(ABORT_TEXT).sort(), abort);
  const types = read("src/ipc/types.ts");
  for (const k of fail) assert.ok(types.includes(`| "${k}"`), k);
  // 種類の名前（snake_case）をそのまま画面に出していない
  for (const t of Object.values(FAIL_TEXT)) assert.ok(t.length > 0 && !/[a-z]+_[a-z]+/.test(t), t);
});

test("文言：測る前の断り・測れる範囲・仮想ケーブルの注意・ボタンの名前", () => {
  assert.equal(LATENCY_TEXT.caution, "クリック音が配信・録画に乗るので、止めてから測ってください。");
  assert.match(LATENCY_TEXT.scope, /Windows がデバイスへ渡すところまで/);
  assert.match(LATENCY_TEXT.scope, /USB の先・HDMI の先・Bluetooth/);
  assert.match(LATENCY_TEXT.cable, /OBS を起動し直すと変わることがあります/);
  assert.equal(LATENCY_TEXT.apply, "この値を入れる");
  assert.match(LATENCY_TEXT.how, /いちばん遅く届く出力先に揃える/);
  assert.match(LATENCY_TEXT.how, /今の遅らせ量は、測るときには使いません/);
  // 時間は「通常は」。いちばん長いときの目安も書く
  assert.match(LATENCY_TEXT.how, /通常は約 6 秒/);
  assert.match(LATENCY_TEXT.how, /長くて 15 秒ほど/);
  assert.match(LATENCY_TEXT.measuring, /通常は約 6 秒/);
  assert.ok(!/（約 [0-9] 秒/.test(LATENCY_TEXT.how + LATENCY_TEXT.measuring));
});

test("画面の組み立て：入口はプルダウンとミキサーの2か所。小窓は App の直下。閉じたら測るのをやめる。流している間は「測る」も押せない", () => {
  const ob = read("src/layout/OutputButton.tsx");
  assert.match(ob, /<div className="output-pop-foot">\s*<LatencyButton showReason onOpen=\{props\.onLatency\}/);
  // プルダウンは開いたまま小窓を重ねない（押したら閉じる）。「詳細を開く」などの下の段は置かない（決まりのまま）
  assert.match(ob, /<OutputPopup onLatency=\{\(\) => close\(false\)\} \/>/);
  assert.ok(!/詳細を開く|配信モードを開く|設定を開く/.test(ob));
  const mx = read("src/layout/Mixer.tsx");
  assert.match(mx, /<h2 className="mixer-title">出力<\/h2>[\s\S]*?<LatencyButton \/>\s*<AddButton \/>/);
  const app = read("src/App.tsx");
  assert.match(app, /<LatencyHost \/>\s*<DialogHost \/>/);
  const dlg = read("src/layout/LatencyDialog.tsx");
  // 押せないボタンは aria-disabled（フォーカスでき、吹き出しで理由が出る）。押しても何もしない
  assert.match(dlg, /aria-disabled=\{blocked \? true : undefined\}/);
  assert.match(dlg, /if \(blocked\) return;\s*props\.onOpen\?\.\(\);\s*dialogOpen\.set\(true\);/);
  // 「やめる」は測定の番号で指す（閉じたとき・「やめる」のどちらも）
  assert.match(dlg, /if \(measuring\.current\) void api\.latencyCancel\(run\.current\)/);
  assert.match(dlg, /const my = nextRun\(\);\s*run\.current = my;/);
  assert.match(dlg, /\.latencyMeasure\(my,/);
  assert.match(dlg, /if \(blocked \|\| !setup\) return;/);
  // 閉じたあと・測り直したあとに届いた古い結果は使わない
  assert.match(dlg, /if \(!alive\.current \|\| my !== run\.current\) return;/);
});

test("Rust：流している間は始めない・測る間に流し始めたらやめる・同時に1つだけ・測るのは別スレッド。4つのコマンドを登録している", () => {
  const rs = read("src-tauri/src/commands/latency.rs");
  assert.match(rs, /if controller\.is_playing\(\) \{\s*return Ok\(aborted\(&slots, AbortCode::Playing\)\);/);
  assert.match(rs, /\} else if controller\.is_playing\(\) \{\s*Some\(Abort::Playing\)/);
  assert.match(rs, /let Some\(running\) = state\.latency\.begin\(\) else \{\s*return Ok\(aborted\(&slots, AbortCode::Busy\)\);/);
  assert.match(rs, /tauri::async_runtime::spawn_blocking\(move \|\| \{/);
  assert.match(rs, /catch_unwind\(AssertUnwindSafe\(\|\| latency::measure_with\(&targets, &stop, Options::default\(\)\)\)\)/);
  // 手元のログに残す回ごとの記録には、機器の名前を入れない（枠の番号と録る所の種類だけ）
  assert.match(rs, /tracing::info!\("latency slot\{slot\} \{kind\} \{:\.0\}Hz: \{\}", t\.freq_hz, diag_line\(d\)\);/);
  const lib = read("src-tauri/src/lib.rs");
  for (const c of ["latency_setup", "latency_measure", "latency_cancel", "latency_apply"]) assert.ok(lib.includes(`commands::latency::${c},`), c);
  const cmds = read("src/ipc/commands.ts");
  for (const c of ["latency_setup", "latency_measure", "latency_cancel", "latency_apply"]) assert.ok(cmds.includes(`"${c}"`), c);
});

test("見た目：測れませんでした・押せない理由は危険の色の太字（文でも伝える）。小窓の中は切らずに折り返す。色の変数は足さない", () => {
  const css = read("src/theme/base.css");
  const at = css.indexOf("v3.8.0：遅れを測る");
  assert.ok(at >= 0);
  const part = css.slice(at);
  assert.match(part, /\.latency-blocked,\n\.latency-fail \{\n  font-weight: 700;\n  color: var\(--c-danger\);/);
  assert.match(part, /\.latency-body \{[^}]*overflow-wrap: anywhere;/);
  assert.match(part, /\.latency-reason \{\s*overflow-wrap: anywhere;/);
  assert.ok(!/text-overflow: ellipsis/.test(part), "小窓の中は「…」で切らない");
  // 使う色は既存の変数だけ
  for (const m of part.matchAll(/var\((--c-[a-z-]+)\)/g)) assert.ok(["--c-danger", "--c-secondary", "--c-border"].includes(m[1]), m[1]);
  const contrast = read("scripts/theme-contrast.mts");
  for (const v of ["--c-primary", "--c-secondary", "--c-danger"]) assert.ok(contrast.includes(`"${v}／小窓（0.97）"`), v);
});

test("公開リポジトリ：遅れを測る部分に、作者の機器の製品名・利用者のパスを書いていない", () => {
  const files = [
    "src/layout/latency.ts",
    "src/layout/LatencyDialog.tsx",
    "src-tauri/src/audio/latency/mod.rs",
    "src-tauri/src/audio/latency/run.rs",
    "src-tauri/src/audio/latency/detect.rs",
    "src-tauri/src/commands/latency.rs",
    "scripts/ui-latency.test.mts",
  ];
  // 利用者のホームのパスを書いていないか（この検査の文そのものに当たらないよう、つないで作る）。
  // 機器の製品名などの手元だけの言葉は、公開するこのファイルには書かない。探したいときは手元で
  // 環境変数 PL_PRIVATE_WORDS に「|」区切りで渡す（例：PowerShell で $env:PL_PRIVATE_WORDS = "語1|語2"）
  const banned = ["C:" + "\\Users\\", "C:" + "/Users/", ...(process.env.PL_PRIVATE_WORDS ?? "").split("|").filter((w) => w !== "")];
  for (const f of files) {
    const src = read(f);
    for (const b of banned) assert.ok(!src.includes(b), `${f}: ${b}`);
  }
});

test("プルダウンで押せない理由を文字でも出すときは、その見える文を aria-describedby で指し、吹き出しの読み上げ用の文は付けない（2回読ませない）", () => {
  const dlg = read("src/layout/LatencyDialog.tsx");
  assert.match(dlg, /const visibleReason = props\.showReason === true && blocked !== null;/);
  assert.match(dlg, /<Tip text=\{blocked\} delay=\{300\} className="inline" describe=\{blocked !== null && !visibleReason\}>/);
  assert.match(dlg, /aria-describedby=\{visibleReason \? reasonId : undefined\}/);
  assert.match(dlg, /<span id=\{reasonId\} className="hint-text latency-reason">/);
  assert.ok(!/latency-reason" aria-hidden/.test(dlg));
});

test("実機の試し（2026-11-18）と診断（11-19）の直し：開いて 3 秒待ち（仮想ケーブルの出口の遅れが 1.3〜2.0 秒ごろに跳ぶことがある）、回ごとの狙いはその回の 200ms 前に決める。文は「通常は約 6 秒」", () => {
  const mod = read("src-tauri/src/audio/latency/mod.rs");
  assert.match(mod, /pub const WARMUP_MS: u64 = 3000;/);
  assert.match(mod, /pub const WARMUP_MAX_MS: u64 = 5000;/);
  assert.match(mod, /約 10ms 跳ぶ/);
  assert.match(read("README.md"), /開いてから 3 秒待ってからクリックを鳴らす/);
  // いちばん長い場合（開く 5 秒＋落ち着く待ちの上限＋クリック 6 回＋録り切れない待ち）が、画面の文の「長くて 15 秒ほど」に収まる
  const n = (name: string) => Number(mod.match(new RegExp(`pub const ${name}: (?:u64|usize) = (\\d+);`))![1]);
  const worst =
    n("OPEN_TIMEOUT_MS") +
    Math.max(n("WARMUP_MS") + 2000, n("WARMUP_MAX_MS")) +
    n("FIRST_CLICK_LEAD_MS") +
    (n("CLICK_COUNT") - 1) * n("CLICK_INTERVAL_MS") +
    n("LOOK_AFTER_MS") +
    n("CAPTURE_TAIL_MS") +
    10;
  assert.ok(worst <= 15_000, `${worst}ms`);
  assert.match(read("src-tauri/src/audio/latency/run.rs"), /Duration::from_millis\(\(opts\.warmup_ms \+ 2000\)\.max\(tuning::WARMUP_MAX_MS\)\)/);
  assert.match(mod, /pub const PLAN_AHEAD_MS: u64 = 200;/);
  const run = read("src-tauri/src/audio/latency/run.rs");
  assert.match(run, /l\.plan_due\(&targets_ns, now, plan_ahead\);/);
  assert.ok(!/fn plan\(&mut self, targets_ns/.test(run), "全部の回を始めに決める作りは残さない（診断では PLAN_AHEAD_MS を大きくして再現する）");
  assert.match(LATENCY_TEXT.how, /通常は約 6 秒/);
});
