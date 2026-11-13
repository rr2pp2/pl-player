// 配布用ビルドの検査（scripts/release-scan.mts）と組み立て（scripts/release.mts・release-zip.mts）を確かめる。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。ビルドはしない）：
//   node --test scripts/release-scan.test.mts
// 例のユーザー名・フォルダはすべて架空（taro-test）。この PC の値は、最後のテスト（リポジトリのファイルの検査）だけが使う。
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { currentRoots, formatReport, isBad, maskText, needlesFor, nsisRuntimePathLines, pdbPaths, scanBuffer, scanFile, shortNameLike, type Roots } from "./release-scan.mts";
import {
  assetNames,
  buildInfoProblems,
  checkSums,
  cTrimFlags,
  encodedRustflags,
  isSafeAssetName,
  otherVersions,
  outDirProblem,
  remapPairs,
  renderReadme,
  resolveOutDir,
  sameExceptBundleMarker,
  sourceState,
  sumsText,
  type BuildInfo,
  windowsPowerShellEnv,
  type SourceState,
} from "./release.mts";
import { makeZip, readZip } from "./release-zip.mts";

const project = fileURLToPath(new URL("..", import.meta.url));
const scanCli = fileURLToPath(new URL("./release-scan.mts", import.meta.url));

const roots: Roots = {
  home: "C:\\Users\\taro-test",
  project: "C:\\Users\\taro-test\\work\\pl-player",
  cargoHome: "C:\\Users\\taro-test\\.cargo",
  rustupHome: "C:\\Users\\taro-test\\.rustup",
  temp: ["C:\\Users\\TAROTE~1\\AppData\\Local\\Temp"],
  user: "taro-test",
};
const needles = needlesFor(roots);

const ascii = (s: string) => Buffer.from(s, "latin1");
const u16 = (s: string) => Buffer.from(s, "utf16le");
const pad = (b: Buffer, before = 37, after = 41) => Buffer.concat([Buffer.alloc(before, 0x90), b, Buffer.alloc(after, 0xcc)]);
const count = (b: Buffer) => scanBuffer(b, needles).length;

test("見つける：ホームのパスを、\\ と / と \\\\・大文字小文字・前に文字が続く形で", () => {
  for (const s of [
    "C:\\Users\\taro-test\\.cargo\\registry\\src\\x\\lib.rs",
    "C:/Users/taro-test/AppData/Local/x",
    "c:\\users\\TARO-TEST\\x",
    "C:\\\\Users\\\\taro-test\\\\x",
    "overflowC:\\Users\\taro-test\\x.rs", // Rust の文字列は区切りなしで並ぶ
    "file:///C:/Users/taro-test/x",
  ]) {
    assert.equal(count(pad(ascii(s))), 1, s);
  }
});

test("見つける：UTF-16LE（偶数・奇数の位置）", () => {
  const s = "C:\\Users\\taro-test\\AppData\\Roaming\\x";
  assert.equal(count(pad(u16(s), 36)), 1);
  assert.equal(count(pad(u16(s), 37)), 1);
  const hit = scanBuffer(pad(u16(s), 37), needles)[0];
  assert.equal(hit.encoding, "utf-16le");
  assert.equal(hit.offset, 37);
});

test("見つける：別のドライブ・ドライブの無い形・home/<名前>・8.3 形式の一時フォルダ", () => {
  for (const s of ["D:\\Users\\taro-test\\x", "/c/Users/taro-test/x", "\\Users\\taro-test", "/home/taro-test/.cargo", "\\Documents and Settings\\taro-test\\x", "C:\\Users\\TAROTE~1\\AppData\\Local\\Temp\\cc123.obj"]) {
    assert.equal(count(pad(ascii(s))), 1, s);
  }
  // ユーザー名だけのフォルダ（3文字以上のとき）
  assert.equal(count(pad(ascii("E:\\backup\\taro-test\\x"))), 1);
});

test("見つけない：名前の続き・置き換えたあとの名前・ほかのユーザー", () => {
  for (const s of ["C:\\Users\\taro-tester\\x", "C:\\Users\\taro-test2", "/build/src/src/lib.rs", "/build/cargo\\registry\\src\\x\\lib.rs", "C:\\Users\\Public\\x", "taro-test", "Users taro-test"]) {
    assert.equal(count(pad(ascii(s))), 0, s);
  }
});

test("同じ場所は1件に数え、細かい名前（プロジェクト）で知らせる。前後の文では名前を伏せる", () => {
  const hits = scanBuffer(pad(ascii("C:\\Users\\taro-test\\work\\pl-player\\src-tauri\\src\\lib.rs")), needles);
  assert.equal(hits.length, 1);
  assert.equal(hits[0].label, "<プロジェクト>");
  assert.ok(!/taro/i.test(hits[0].context), hits[0].context);
  assert.ok(!/taro/i.test(maskText("x C:/Users/taro-test/y", needles)));
  // 2か所なら2件
  assert.equal(count(Buffer.concat([pad(ascii("C:\\Users\\taro-test\\a")), pad(ascii("C:\\Users\\taro-test\\b"))])), 2);
});

/** 小さな偽の PE（exe）。CodeView（RSDS）に PDB の名前を入れる */
function fakePe(pdb: string, extra = Buffer.alloc(0)): Buffer {
  const buf = Buffer.alloc(0x400);
  buf.write("MZ", 0, "latin1");
  buf.writeUInt32LE(0x40, 0x3c);
  const pe = 0x40;
  buf.writeUInt32LE(0x00004550, pe);
  buf.writeUInt16LE(0x8664, pe + 4);
  buf.writeUInt16LE(1, pe + 6); // 節 1 つ
  buf.writeUInt16LE(240, pe + 20); // PE32+ の任意ヘッダの大きさ
  const opt = pe + 24;
  buf.writeUInt16LE(0x20b, opt);
  buf.writeUInt32LE(16, opt + 108);
  buf.writeUInt32LE(0x1000, opt + 112 + 6 * 8); // デバッグディレクトリ
  buf.writeUInt32LE(28, opt + 112 + 6 * 8 + 4);
  const sec = opt + 240;
  buf.write(".rdata", sec, "latin1");
  buf.writeUInt32LE(0x200, sec + 8);
  buf.writeUInt32LE(0x1000, sec + 12);
  buf.writeUInt32LE(0x200, sec + 16);
  buf.writeUInt32LE(0x200, sec + 20);
  const dbg = 0x200;
  buf.writeUInt32LE(2, dbg + 12); // CODEVIEW
  buf.writeUInt32LE(24 + Buffer.byteLength(pdb) + 1, dbg + 16);
  buf.writeUInt32LE(0x1040, dbg + 20);
  buf.writeUInt32LE(0x240, dbg + 24);
  buf.write("RSDS", 0x240, "latin1");
  buf.write(pdb, 0x240 + 24, "utf8");
  return Buffer.concat([buf, extra]);
}

test("PDB の名前：ファイル名だけなら通し、フォルダが付いていたら失敗にする", () => {
  assert.deepEqual(pdbPaths(fakePe("pl_player.pdb")), ["pl_player.pdb"]);
  assert.deepEqual(pdbPaths(Buffer.from("not a pe")), null);
  const dir = mkdtempSync(join(tmpdir(), "pl-release-test-"));
  try {
    const ok = join(dir, "ok.exe");
    const bad = join(dir, "bad.exe");
    const bad2 = join(dir, "bad2.exe");
    writeFileSync(ok, fakePe("pl_player.pdb"));
    writeFileSync(bad, fakePe("D:\\a\\b\\pl_player.pdb")); // ユーザーの名前が無くても、フォルダが付いていたら失敗
    writeFileSync(bad2, fakePe("pl_player.pdb", u16("C:\\Users\\taro-test\\x")));
    assert.equal(isBad(scanFile(ok, needles)), false);
    assert.equal(isBad(scanFile(bad, needles)), true);
    assert.equal(isBad(scanFile(bad2, needles)), true);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("検査のコマンド：偽の exe にホームのパスを埋めると、終了コード 1 で止まる（埋めなければ 0）", () => {
  const dir = mkdtempSync(join(tmpdir(), "pl-release-test-"));
  try {
    const env = { ...process.env, USERPROFILE: "C:\\Users\\taro-test", USERNAME: "taro-test", CARGO_HOME: "", RUSTUP_HOME: "", TEMP: "C:\\Users\\taro-test\\AppData\\Local\\Temp", TMP: "" };
    const bad = join(dir, "bad.exe");
    const ok = join(dir, "ok.exe");
    writeFileSync(bad, fakePe("pl_player.pdb", ascii("called `Option::unwrap()`C:\\Users\\taro-test\\.cargo\\registry\\src\\x\\lib.rs")));
    writeFileSync(ok, fakePe("pl_player.pdb", ascii("called `Option::unwrap()`/build/cargo\\registry\\src\\x\\lib.rs")));
    const rb = spawnSync(process.execPath, [scanCli, bad], { env, encoding: "utf8" });
    assert.equal(rb.status, 1, rb.stdout + rb.stderr);
    assert.match(rb.stdout, /<cargo のホーム>/);
    assert.ok(!/taro/i.test(rb.stdout), "報告に名前を書かない");
    const ro = spawnSync(process.execPath, [scanCli, ok], { env, encoding: "utf8" });
    assert.equal(ro.status, 0, ro.stdout + ro.stderr);
    // 検査できなかったときは 2（0＝見つからなかった、1＝見つかった と分ける）
    const empty = join(dir, "empty");
    mkdirSync(empty);
    const re = spawnSync(process.execPath, [scanCli, empty], { env, encoding: "utf8" });
    assert.equal(re.status, 2, "空のフォルダ：" + re.stdout + re.stderr);
    const rm = spawnSync(process.execPath, [scanCli, join(dir, "no-such-file.exe")], { env, encoding: "utf8" });
    assert.equal(rm.status, 2, "無いパス：" + rm.stdout + rm.stderr);
    assert.ok(!/at .*\.mts:\d+/.test(rm.stderr), "スタックトレースを出さない");
    const rn = spawnSync(process.execPath, [scanCli], { env, encoding: "utf8" });
    assert.equal(rn.status, 2, "指定なし");
    // 無いパスが混ざっても、あるものは検査して結果を出す。見つかれば 1 が先（2 より）。無いパスもあわせて出す
    const rx = spawnSync(process.execPath, [scanCli, bad, join(dir, "no-such-file.exe")], { env, encoding: "utf8" });
    assert.equal(rx.status, 1, rx.stdout + rx.stderr);
    assert.match(rx.stdout, /NG bad\.exe|NG .*bad\.exe/);
    assert.match(rx.stdout, /見つかりました（1 ファイル）/);
    assert.match(rx.stderr, /見つからないパスがあります：.*no-such-file\.exe/);
    // 順番を入れ替えても同じ
    const rx2 = spawnSync(process.execPath, [scanCli, join(dir, "no-such-file.exe"), bad], { env, encoding: "utf8" });
    assert.equal(rx2.status, 1, rx2.stdout + rx2.stderr);
    // 見つからず、無いパスがあれば 2（結果は出す）
    const ry = spawnSync(process.execPath, [scanCli, ok, join(dir, "no-such-file.exe")], { env, encoding: "utf8" });
    assert.equal(ry.status, 2, ry.stdout + ry.stderr);
    assert.match(ry.stdout, /見つかりませんでした（1 ファイル）/);
    assert.match(ry.stderr, /見つからないパスがあります/);
    // 空のフォルダとファイル：ファイルがあれば 0
    const rz = spawnSync(process.execPath, [scanCli, ok, empty], { env, encoding: "utf8" });
    assert.equal(rz.status, 0, rz.stdout + rz.stderr);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("参考：8.3 形式の短い名前に見えるものを出す（止めない）。今のユーザー名から作られる形は伏せる", () => {
  const buf = pad(ascii("x C:\\PROGRA~1\\a.dll y D:\\Work\\ABCDEF~12.TXT z \\TAROTE~1\\b.obj"));
  const names = shortNameLike(buf, "taro-test");
  assert.deepEqual(names, ["<ユーザー名の短い形>", "ABCDEF~12", "PROGRA~1"]);
  // UTF-16LE でも
  assert.deepEqual(shortNameLike(pad(u16("C:\\ALICEB~1\\x"), 37), "taro-test"), ["ALICEB~1"]);
  // 区切りが前に無い・続きが名前の文字なら出さない（圧縮されたデータの偶然の並びを減らす）
  assert.deepEqual(shortNameLike(pad(ascii("xPROGRA~1\\a ABC~1x \\ABCDEFG~1\\"))), []);
  // 止めない
  const dir = mkdtempSync(join(tmpdir(), "pl-release-test-"));
  try {
    const f = join(dir, "a.bin");
    writeFileSync(f, buf);
    const r = scanFile(f, needles, "taro-test");
    assert.equal(r.shortNames.length, 3);
    const report = formatReport([r]);
    assert.match(report, /8\.3 形式の短い名前に見えるもの/);
    assert.ok(!/TAROTE/i.test(report), "ユーザー名から作られる形は伏せる");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("NSIS の台本：パスをビルドのときだけ使う行は通し、動かすときの文字に入る行を返す", () => {
  const script = [
    '!define MAINBINARYSRCPATH "C:\\Users\\taro-test\\work\\pl-player\\src-tauri\\target\\release\\pl-player.exe"',
    '!define PLUGINS "C:\\Users\\taro-test\\AppData\\Local\\tauri\\NSIS\\Plugins"',
    '!define COPY "${PLUGINS}\\x"',
    '!addplugindir "${PLUGINS}"',
    '  File "${MAINBINARYSRCPATH}"',
    '!include "C:\\Users\\taro-test\\work\\pl-player\\x.nsh"',
    '  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$INSTDIR\\x.exe"',
  ].join("\n");
  assert.deepEqual(nsisRuntimePathLines(script, needles), []);
  const bad = script + '\n  DetailPrint "${COPY}"\n  StrCpy $0 "C:\\Users\\taro-test\\x"';
  const lines = nsisRuntimePathLines(bad, needles);
  assert.equal(lines.length, 2);
  assert.ok(lines.every((l) => !/taro/i.test(l)));
});

test("置き換えの組：上のフォルダが先、下のフォルダが後（rustc は最後に当たったものを使う）", () => {
  const pairs = remapPairs(roots);
  // rustc と同じく「前から見て、最後に当たったもの」
  const remap = (p: string) => {
    let out = p;
    for (const [f, t] of pairs) if (p.startsWith(f)) out = t + p.slice(f.length);
    return out;
  };
  assert.equal(remap("C:\\Users\\taro-test\\work\\pl-player\\src-tauri\\src\\lib.rs"), "/build/src\\src-tauri\\src\\lib.rs");
  assert.equal(remap("C:\\Users\\taro-test\\.cargo\\registry\\src\\a\\lib.rs"), "/build/cargo\\registry\\src\\a\\lib.rs");
  assert.equal(remap("C:\\Users\\taro-test\\.rustup\\toolchains\\x\\lib\\rustlib\\src\\rust\\library\\core\\src\\lib.rs"), "/build/rustup\\toolchains\\x\\lib\\rustlib\\src\\rust\\library\\core\\src\\lib.rs");
  assert.equal(remap("c:/Users/taro-test/other/x.rs"), "/build/home/other/x.rs");
  assert.equal(remap("C:\\Users\\TAROTE~1\\AppData\\Local\\Temp\\x.rs"), "/build/tmp\\x.rs");
  // 置き換え先に名前を入れない
  assert.ok(pairs.every(([, t]) => /^\/build\/[a-z]+$/.test(t)));
});

test("C に渡す指定：/d1trimfile を長い順（cl.exe は先に書いたものを使う）、区切りは /、空白を含むものは渡さない", () => {
  const { flags, skipped } = cTrimFlags({ ...roots, temp: ["C:\\Users\\taro-test\\App Data\\Temp"] });
  assert.ok(flags.every((f) => f.startsWith("/d1trimfile:") && f.endsWith("/") && !f.includes("\\")));
  const lens = flags.map((f) => f.length);
  assert.deepEqual(lens, [...lens].sort((a, b) => b - a));
  // プロジェクトがホームより先
  assert.ok(flags.indexOf("/d1trimfile:C:/Users/taro-test/work/pl-player/") < flags.indexOf("/d1trimfile:C:/Users/taro-test/"));
  assert.deepEqual(skipped, ["C:/Users/taro-test/App Data/Temp/"]);
});

test("rustc に渡す指定：前からの指定を残し、0x1f で区切る（空白を含むフォルダでも切れない）", () => {
  const pairs: [string, string][] = [["C:\\Users\\a b\\x", "/build/src"]];
  const v = encodedRustflags({ RUSTFLAGS: "-C target-cpu=x86-64-v2" }, pairs);
  assert.deepEqual(v.split("\x1f"), ["-C", "target-cpu=x86-64-v2", "--remap-path-prefix=C:\\Users\\a b\\x=/build/src"]);
  const w = encodedRustflags({ CARGO_ENCODED_RUSTFLAGS: "-Cfoo\x1f-Cbar", RUSTFLAGS: "ignored" }, pairs);
  assert.deepEqual(w.split("\x1f"), ["-Cfoo", "-Cbar", "--remap-path-prefix=C:\\Users\\a b\\x=/build/src"]);
});

test("zip：日本語の名前を UTF-8 で書き、区切りは /、中身と CRC が合う", () => {
  const items = [
    { name: "PL-Player-9.9.9/PL Player 3（インストールせずに使う）.exe", data: Buffer.alloc(5000, 7), mtime: new Date(2026, 10, 13, 1, 2, 4) },
    { name: "PL-Player-9.9.9/README.md", data: Buffer.from("# 例\n", "utf8"), mtime: new Date(2026, 10, 13) },
    { name: "PL-Player-9.9.9/empty.txt", data: Buffer.alloc(0), mtime: new Date(2026, 10, 13) },
  ];
  const back = readZip(makeZip(items));
  assert.deepEqual(back.map((e) => e.name), items.map((i) => i.name));
  assert.ok(back.every((e) => e.utf8Flag));
  back.forEach((e, i) => assert.ok(e.data.equals(items[i].data)));
  assert.throws(() => makeZip([{ name: "a\\b.txt", data: Buffer.alloc(1), mtime: new Date() }]));
  assert.throws(() => makeZip([{ name: "../b.txt", data: Buffer.alloc(1), mtime: new Date() }]));
});

test("使う人向けの README：版を埋め、「<版> で変えたこと」が無ければ止める", () => {
  const tpl = readFileSync(join(project, "scripts", "release-README.md"), "utf8");
  const version = JSON.parse(readFileSync(join(project, "package.json"), "utf8")).version as string;
  const out = renderReadme(tpl, version);
  assert.ok(out.startsWith(`# PL Player ${version}\n`));
  assert.ok(out.includes(`PL Player 3_${version}_x64-setup.exe`));
  assert.ok(out.includes("詳細情報") && out.includes("実行"), "署名なしの警告の抜け方");
  assert.ok(!out.includes("{{"));
  assert.throws(() => renderReadme(tpl, "99.0.0"), /99\.0\.0 で変えたこと/);
  // 名乗りの版は型に直書きしない（見出しだけ書き換えた次の版で、前の版が残るため）
  assert.ok(!/PL-Player\/\d/.test(tpl), "名乗りの版は {{VERSION}} で書く");
  assert.ok(out.includes(`PL-Player/${version} (`));
  // 型が CRLF でも、配る README は LF
  assert.ok(!renderReadme(tpl.replace(/\r?\n/g, "\r\n"), version).includes("\r"));
});

test("使う人向けの README：今の版と違う版番号を、行の番号つきで知らせる", () => {
  const text = "# PL Player 9.9.9\n\n3.0.0 以降と同じアプリ\n## 9.9.9 で変えたこと\n- 名札 `PL-Player/3.1.2 (x)`、3.1.2 を最初に起動したとき\n- 1.2.3.4 と v10.0.1。\n";
  const got = otherVersions(text, "9.9.9");
  assert.deepEqual(got.map((o) => `${o.line}:${o.version}`), ["3:3.0.0", "5:3.1.2", "5:3.1.2", "6:10.0.1"]);
  // 型そのもの（今の版で埋めたもの）に、名乗りの古い版は残っていない
  const tpl = readFileSync(join(project, "scripts", "release-README.md"), "utf8");
  const version = JSON.parse(readFileSync(join(project, "package.json"), "utf8")).version as string;
  assert.ok(!otherVersions(renderReadme(tpl, version), version).some((o) => /PL-Player\//.test(o.text)));
});

test("Releases に置く名前：GitHub が変えない文字だけ（英数字と . _ -）。フォルダを付けない", () => {
  const a = assetNames("3.1.2");
  assert.deepEqual(a, { base: "PL-Player-3.1.2", zip: "PL-Player-3.1.2.zip", setup: "PL-Player-3.1.2-x64-setup.exe", portable: "PL-Player-3.1.2-x64-portable.exe", sums: "PL-Player-3.1.2.sha256.txt" });
  for (const n of [a.zip, a.setup, a.portable, a.sums]) assert.ok(isSafeAssetName(n), n);
  for (const n of ["PL Player 3_3.1.2_x64-setup.exe", "PL Player 3（インストールせずに使う）.exe", "PL-Player-3.1.2/README.md", "a\\b.exe", ".x.exe", "x.exe.", ""]) assert.ok(!isSafeAssetName(n), n);
});

test("SHA256 の一覧：フォルダの付かない名前で書き、落とした物を同じフォルダに置けば照らせる（sha256sum -c・PowerShell）", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "pl-release-test-"));
  try {
    const a = assetNames("9.9.9");
    const files: [string, Buffer][] = [
      [a.zip, Buffer.from("zip")],
      [a.setup, Buffer.alloc(3000, 1)],
      [a.portable, Buffer.alloc(2000, 2)],
    ];
    const h = (b: Buffer) => createHash("sha256").update(b).digest("hex");
    for (const [n, b] of files) writeFileSync(join(dir, n), b);
    const text = sumsText(files.map(([name, b]) => ({ name, hash: h(b) })));
    assert.equal(text.split("\n").filter((l) => l).length, 3);
    assert.ok(text.split("\n").filter((l) => l).every((l) => /^[0-9a-f]{64} \*[^/\\]+$/.test(l)), text);
    assert.ok(!text.includes("\r"));
    assert.throws(() => sumsText([{ name: "PL-Player-9.9.9/x.exe", hash: h(Buffer.from("x")) }]), /フォルダ付き/);
    writeFileSync(join(dir, a.sums), text);
    assert.deepEqual(checkSums(text, dir).map((c) => c.result), ["ok", "ok", "ok"]);
    // 1つだけ落とした人（ほかは missing。壊れていれば ng）
    rmSync(join(dir, a.zip));
    writeFileSync(join(dir, a.portable), Buffer.alloc(2000, 3));
    assert.deepEqual(checkSums(text, dir).map((c) => c.result), ["missing", "ok", "ng"]);
    writeFileSync(join(dir, a.portable), Buffer.alloc(2000, 2));

    // README の「入手」に書いた照らし方を、そのまま打つ
    const readme = readFileSync(join(project, "README.md"), "utf8");
    const block = (word: string) => {
      const m = [...readme.matchAll(/```\n([^`]+?)\n```/g)].map((x) => x[1]).find((b) => b.includes(word));
      assert.ok(m, `README に ${word} の照らし方が無い`);
      return m!.replaceAll("<版>", "9.9.9");
    };
    const sha = spawnSync("sha256sum", ["--version"], { encoding: "utf8" });
    if (sha.status === 0) {
      const cmd = block("sha256sum").split(/\s+/);
      const r = spawnSync(cmd[0], cmd.slice(1), { cwd: dir, encoding: "utf8" });
      assert.equal(r.status, 0, r.stdout + r.stderr);
      assert.match(r.stdout, new RegExp(`${a.setup}: OK`));
      assert.match(r.stdout, new RegExp(`${a.portable}: OK`));
    } else {
      t.diagnostic("sha256sum が無いので、sha256sum -c は飛ばした");
    }
    // PowerShell 7 の中から走らせても、5.1 が 7 用のモジュールを読まないように（release.mts の windowsPowerShellEnv と同じ扱い）
    const psEnv = windowsPowerShellEnv(process.env);
    const ps = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", block("Get-FileHash")], { cwd: dir, env: psEnv, encoding: "utf8", windowsHide: true });
    if (ps.error) {
      t.diagnostic("PowerShell が無いので飛ばした");
    } else {
      assert.equal(ps.status, 0, ps.stdout + ps.stderr);
      assert.deepEqual(ps.stdout.trim().split(/\r?\n/), [`OK  ${a.setup}`, `OK  ${a.portable}`]);
      writeFileSync(join(dir, a.setup), Buffer.alloc(3000, 9));
      const ps2 = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", block("Get-FileHash")], { cwd: dir, env: psEnv, encoding: "utf8", windowsHide: true });
      assert.deepEqual(ps2.stdout.trim().split(/\r?\n/), [`NG  ${a.setup}`, `OK  ${a.portable}`]);
      // 一覧と同じ名前のファイルが1つも無い（名前を変えて保存した）ときは、何も出ないのではなく、そう知らせる
      rmSync(join(dir, a.setup));
      rmSync(join(dir, a.portable));
      writeFileSync(join(dir, "renamed.exe"), Buffer.alloc(2000, 2));
      const utf8 = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; ";
      const ps3 = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", utf8 + block("Get-FileHash")], { cwd: dir, env: psEnv, encoding: "utf8", windowsHide: true });
      assert.equal(ps3.status, 0, ps3.stdout + ps3.stderr);
      assert.match(ps3.stdout.trim(), /^照合できるファイルがありません/, ps3.stdout);
    }
    // 大文字小文字：一覧は小文字、Get-FileHash は大文字。README の両方に「問わない」と書いてある
    assert.match(readme, /大文字小文字は問わない/);
    assert.match(readFileSync(join(project, "scripts", "release-README.md"), "utf8"), /大文字小文字は問いません/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("--out：src-tauri\\target\\ の下だけ。dist とその中・ほかの場所・使えない名前は断る（文字として見る）", () => {
  const p = "C:\\work\\proj";
  const same = (x: string) => x; // ディスクを見ない（8.3・ジャンクションは次のテスト）
  const prob = (o: string) => outDirProblem(p, o, same);
  const OUTSIDE = /src-tauri\\target\\ の下/;
  for (const ok of ["src-tauri\\target\\try", "src-tauri/target/try", "src-tauri\\target\\a\\b", "C:\\work\\proj\\src-tauri\\target\\try", "c:\\WORK\\PROJ\\SRC-TAURI\\Target\\x", "src-tauri\\target\\dist-try", "src-tauri\\target\\..foo", "src-tauri\\target\\x\\..\\y"]) {
    assert.equal(prob(ok), null, ok);
  }
  const r = resolveOutDir(p, "src-tauri/target/try", same);
  assert.deepEqual(r, { dir: "C:\\work\\proj\\src-tauri\\target\\try" });
  // プロジェクトの中でも、target の下でなければ断る（.git・src・node_modules・プロジェクトそのもの・target そのもの）
  for (const bad of [".git", ".git\\x", "src", "node_modules\\x", "release", "..foo", ".", "sub\\..", "src-tauri", "src-tauri\\target", "src-tauri\\target\\.", "src-tauri\\target\\x\\..\\.."]) {
    assert.match(prob(bad) ?? "", OUTSIDE, bad);
  }
  // 別のドライブ・外・UNC
  for (const bad of ["D:\\x", "D:x", "\\\\server\\share\\x", "..", "..\\x", "sub\\..\\..\\x", "C:\\work\\proj2", "C:\\work\\proj2\\src-tauri\\target\\x"]) {
    assert.match(prob(bad) ?? "", OUTSIDE, bad);
  }
  assert.match(prob("") ?? "", /空/);
  // dist そのものとその中（末尾の . と空白は落として比べる。Windows は落とした名前として扱う）
  for (const bad of ["src-tauri\\target\\dist", "src-tauri\\target\\dist\\x", "src-tauri\\target\\DIST\\x", "src-tauri\\target\\dist.\\x", "src-tauri\\target\\dist \\x", "src-tauri\\target\\dist. .\\x", "src-tauri\\target\\dist.", "src-tauri. \\target\\dist\\x"]) {
    assert.match(prob(bad) ?? "", /dist とその中は使えません/, bad);
  }
  // 末尾の . と空白：落とすと target の外になるものは外として断り、target の下でも名前が変わるので断る
  assert.match(prob(".git.\\x") ?? "", OUTSIDE);
  assert.match(prob("src-tauri\\target\\try.") ?? "", /末尾に \. か空白/);
  assert.match(prob("src-tauri\\target\\try \\x") ?? "", /末尾に \. か空白/);
  assert.match(prob("src-tauri\\target\\...") ?? "", /使えない名前/);
  // 予約された名前（拡張子・大文字小文字を問わない）
  for (const bad of ["src-tauri\\target\\CON", "src-tauri\\target\\nul\\x", "src-tauri\\target\\Aux.txt", "src-tauri\\target\\com1", "src-tauri\\target\\LPT9.log", "src-tauri\\target\\con .x", "src-tauri\\target\\COM¹", "src-tauri\\target\\conin$", "CON", "nul\\x"]) {
    assert.match(prob(bad) ?? "", /予約された名前/, bad);
  }
  for (const ok of ["src-tauri\\target\\console", "src-tauri\\target\\com10", "src-tauri\\target\\nulx", "src-tauri\\target\\my.con"]) assert.equal(prob(ok), null, ok);
  // 使えない文字
  for (const bad of ["src-tauri\\target\\x?", "src-tauri\\target\\a*b", "src-tauri\\target\\a<b", "src-tauri\\target\\a>b", 'src-tauri\\target\\a"b', "src-tauri\\target\\a|b", "src-tauri\\target\\a:b", "src-tauri\\target\\a\tb", "x?"]) {
    assert.match(prob(bad) ?? "", /使えない文字/, bad);
  }
});

test("--out：8.3 形式の短い名前・ジャンクションは、実際の場所に直してから比べる（ディスクで確かめる）", (t) => {
  if (process.platform !== "win32") return t.skip("Windows だけ");
  const dir = mkdtempSync(join(tmpdir(), "pl-release-test-"));
  try {
    const proj = join(dir, "project-folder");
    mkdirSync(join(proj, "src-tauri", "target", "dist"), { recursive: true });
    mkdirSync(join(proj, ".git"));
    assert.equal(outDirProblem(proj, "src-tauri\\target\\try"), null);
    const r = resolveOutDir(proj, "src-tauri\\target\\try");
    assert.ok("dir" in r && r.dir.toLowerCase().endsWith("\\project-folder\\src-tauri\\target\\try"), JSON.stringify(r));
    assert.match(outDirProblem(proj, ".git") ?? "", /target\\ の下/);
    // 短い名前（この PC で 8.3 形式の名前を作っていれば）
    const shortOf = (p: string) => {
      const s = spawnSync("cmd.exe", ["/d", "/c", `for %I in ("${p}") do @echo %~sI`], { encoding: "utf8", windowsVerbatimArguments: true, windowsHide: true });
      return s.status === 0 ? s.stdout.trim() : "";
    };
    const shortTauri = shortOf(join(proj, "src-tauri"));
    const shortName = shortTauri.split("\\").pop() ?? "";
    if (shortName.includes("~")) {
      assert.match(outDirProblem(proj, `${shortName}\\target\\dist\\x`) ?? "", /dist とその中は使えません/);
      assert.match(outDirProblem(proj, `${shortName}\\target\\dist`) ?? "", /dist とその中は使えません/);
      assert.equal(outDirProblem(proj, `${shortName}\\target\\try`), null);
      // プロジェクトを短い名前で渡しても同じ
      assert.match(outDirProblem(shortOf(proj), "src-tauri\\target\\dist\\x") ?? "", /dist とその中は使えません/);
      assert.equal(outDirProblem(shortOf(proj), "src-tauri\\target\\try"), null);
    } else {
      t.diagnostic("この PC では 8.3 形式の短い名前を作っていないので、短い名前は飛ばした");
    }
    // ジャンクション：target の下から dist や .git を指すものは、指す先で比べる
    const j1 = join(proj, "src-tauri", "target", "to-dist");
    const j2 = join(proj, "src-tauri", "target", "to-git");
    const mk = (link: string, to: string) => spawnSync("cmd.exe", ["/d", "/c", `mklink /J "${link}" "${to}"`], { windowsVerbatimArguments: true, windowsHide: true }).status === 0;
    if (mk(j1, join(proj, "src-tauri", "target", "dist")) && mk(j2, join(proj, ".git"))) {
      assert.match(outDirProblem(proj, "src-tauri\\target\\to-dist\\x") ?? "", /dist とその中は使えません/);
      assert.match(outDirProblem(proj, "src-tauri\\target\\to-git") ?? "", /target\\ の下/);
    } else {
      t.diagnostic("ジャンクションを作れなかったので飛ばした");
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("--skip-build：ビルドしたときの記録と今を比べ、中身・版・exe が違えば、どれが違うかを出して止める。コミットしただけなら通す", () => {
  const src: SourceState = { git: true, head: "a".repeat(40), dirty: false, changedFiles: 0, content: "c".repeat(64) };
  const rec: BuildInfo = { version: "9.9.9", builtAt: "2026-11-13T00:00:00.000Z", clean: true, source: src, exeSha256: "e".repeat(64), setupSha256: "s".repeat(64) };
  const now = { version: "9.9.9", source: { ...src }, exeSha256: "e".repeat(64), setupSha256: "s".repeat(64) };
  assert.deepEqual(buildInfoProblems(rec, now, false), []);
  const one = (n: Partial<typeof now>, trial = false) => buildInfoProblems(rec, { ...now, ...n }, trial);
  // コミットが変わっても、中身が同じなら通す
  assert.deepEqual(one({ source: { ...src, head: "b".repeat(40) } }), []);
  assert.deepEqual(buildInfoProblems({ ...rec, source: { ...src, dirty: true, changedFiles: 3 } }, now, false), [], "変更ありでビルド → コミット（中身は同じ）");
  // 違うときは、何が違うかが分かる文にする
  const v = one({ version: "9.9.10" });
  assert.equal(v.length, 1);
  assert.match(v[0], /^版：.*9\.9\.9.*9\.9\.10/);
  const c = one({ source: { ...src, head: "b".repeat(40), dirty: true, changedFiles: 2, content: "d".repeat(64) } });
  assert.equal(c.length, 1);
  assert.match(c[0], /^ソースの中身が、ビルドしたときと違います/);
  assert.match(c[0], /ビルドしたとき：コミット aaaaaaaaaa・まだコミットしていない変更 なし／今：コミット bbbbbbbbbb・まだコミットしていない変更 2 ファイル/);
  assert.match(one({ source: { git: false, head: null, dirty: null, changedFiles: 0, content: null } }).join(), /^ソースの中身：git が使えない/);
  // 前の形の記録（中身の指紋が無い）は止める
  const old = { ...rec, source: { git: true, head: "a".repeat(40), dirty: true, changes: "x", changedFiles: 7 } as unknown as SourceState };
  assert.match(buildInfoProblems(old, now, false).join(), /記録が古い形/);
  assert.match(one({ exeSha256: "f".repeat(64) }).join(), /^exe：記録したあとに差し替え/);
  assert.match(one({ setupSha256: "f".repeat(64) }).join(), /^インストーラー：記録したあとに差し替え/);
  assert.equal(one({ version: "1.0.0", exeSha256: "f".repeat(64), source: { ...src, content: "d".repeat(64) } }).length, 3, "違うところは全部出す");
  const noClean = { ...rec, clean: false };
  assert.match(buildInfoProblems(noClean, now, false).join(), /^作り方：--no-clean/);
  assert.deepEqual(buildInfoProblems(noClean, now, true), []);
});

test("--skip-build：ソースの中身（追跡しているファイル・まだ足していないファイル）が変われば指紋が変わり、コミットしただけでは変わらない", (t) => {
  const g0 = spawnSync("git", ["--version"]);
  if (g0.error || g0.status !== 0) return t.skip("git が使えない");
  const dir = mkdtempSync(join(tmpdir(), "pl-release-test-"));
  const git = (...args: string[]) => {
    const r = spawnSync("git", ["-c", "user.name=test", "-c", "user.email=test@example.invalid", "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false", ...args], { cwd: dir, encoding: "utf8" });
    assert.equal(r.status, 0, r.stderr);
  };
  const problems = (built: SourceState, now: SourceState) => buildInfoProblems({ version: "1.0.0", builtAt: "", clean: true, source: built, exeSha256: "", setupSha256: "" }, { version: "1.0.0", source: now, exeSha256: "", setupSha256: "" }, false);
  try {
    assert.equal(sourceState(dir).git, false, "git の管理下でなければ git: false");
    git("init", "-q");
    writeFileSync(join(dir, ".gitignore"), "ignored/\n");
    writeFileSync(join(dir, "a.rs"), "fn a() {}\n");
    git("add", ".");
    git("commit", "-q", "-m", "1");
    const s1 = sourceState(dir);
    assert.equal(s1.git, true);
    assert.equal(s1.dirty, false);
    assert.match(s1.content ?? "", /^[0-9a-f]{64}$/);
    assert.deepEqual(sourceState(dir), s1, "何も変えなければ同じ");
    // 無視しているフォルダは数えない
    mkdirSync(join(dir, "ignored"));
    writeFileSync(join(dir, "ignored", "x"), "x");
    assert.deepEqual(sourceState(dir), s1);
    // 中身の同じ空のコミット・メッセージだけ変えたコミットでは、指紋は変わらない（コミットは変わる）
    git("commit", "-q", "--allow-empty", "-m", "empty");
    const e1 = sourceState(dir);
    assert.notEqual(e1.head, s1.head);
    assert.equal(e1.content, s1.content);
    assert.deepEqual(problems(s1, e1), []);
    git("commit", "-q", "--amend", "--allow-empty", "-m", "amended");
    assert.deepEqual(problems(s1, sourceState(dir)), []);
    // まだコミットしていない変更
    writeFileSync(join(dir, "a.rs"), "fn a() { 1; }\n");
    const s2 = sourceState(dir);
    assert.equal(s2.dirty, true);
    assert.notEqual(s2.content, s1.content);
    assert.equal(problems(s1, s2).length, 1);
    // まだコミットしていない変更が、さらに変わった（あり → あり でも気づく）
    writeFileSync(join(dir, "a.rs"), "fn a() { 2; }\n");
    const s3 = sourceState(dir);
    assert.equal(s3.dirty, true);
    assert.notEqual(s3.content, s2.content);
    // まだ足していないファイル（中身が変わっても気づく）
    writeFileSync(join(dir, "b.rs"), "1");
    const s4 = sourceState(dir);
    writeFileSync(join(dir, "b.rs"), "2");
    const s5 = sourceState(dir);
    assert.notEqual(s4.content, s3.content);
    assert.notEqual(s5.content, s4.content);
    assert.equal(s5.changedFiles, 2);
    // 名前だけ変えても気づく
    renameSync(join(dir, "b.rs"), join(dir, "c.rs"));
    const s5b = sourceState(dir);
    assert.notEqual(s5b.content, s5.content);
    renameSync(join(dir, "c.rs"), join(dir, "b.rs"));
    assert.equal(sourceState(dir).content, s5.content);
    // 作者の流れ：変更ありでビルド → コミット → --skip-build。コミットは変わるが、中身は同じなので通す
    git("add", ".");
    git("commit", "-q", "-m", "2");
    const s6 = sourceState(dir);
    assert.notEqual(s6.head, s5.head);
    assert.equal(s6.dirty, false);
    assert.equal(s6.content, s5.content);
    assert.deepEqual(problems(s5, s6), []);
    // 消したファイル：消しただけ（まだコミットしていない）と、消してコミットしたものは同じ指紋
    rmSync(join(dir, "b.rs"));
    const s7 = sourceState(dir);
    assert.notEqual(s7.content, s6.content);
    git("add", "-A");
    git("commit", "-q", "-m", "3");
    assert.equal(sourceState(dir).content, s7.content);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("インストーラーの中の exe：Tauri が書き込む印のほかは同じなら同じとみなす", () => {
  const a = Buffer.concat([Buffer.alloc(100, 1), Buffer.from("__TAURI_BUNDLE_TYPE_VAR_UNK", "latin1"), Buffer.alloc(100, 2)]);
  const b = Buffer.from(a);
  b.write("NSS", 100 + "__TAURI_BUNDLE_TYPE_VAR_".length, "latin1");
  assert.equal(sameExceptBundleMarker(a, a).same, true);
  assert.equal(sameExceptBundleMarker(a, b).same, true);
  const c = Buffer.from(b);
  c[5] = 9;
  assert.equal(sameExceptBundleMarker(a, c).same, false);
  assert.equal(sameExceptBundleMarker(a, Buffer.concat([a, Buffer.alloc(1)])).same, false);
});

test("リポジトリのファイルに、この PC のフォルダの場所が書かれていない", (t) => {
  const git = spawnSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], { cwd: project, encoding: "utf8" });
  if (git.status !== 0) return t.skip("git が使えない");
  const here = needlesFor(currentRoots(project));
  const files = git.stdout.split("\0").filter((f) => f.length > 0);
  const bad = files.filter((f) => {
    try {
      return scanFile(join(project, f), here).hits.length > 0;
    } catch {
      return false; // 消したばかりで、まだ git に残っているもの
    }
  });
  assert.deepEqual(bad, []);
});

test("Windows PowerShell 5.1 へ渡す環境：PSModulePath を外す（PowerShell 7 から走らせても 5.1 が自分のモジュールを読む）", () => {
  const env = { Path: "C:/x", PSModulePath: "C:/ps7/Modules", PL_A: "1" };
  assert.deepEqual(windowsPowerShellEnv(env), { Path: "C:/x", PL_A: "1" });
  // 名前の大文字小文字は問わない（Windows の環境変数）
  assert.deepEqual(windowsPowerShellEnv({ psmodulepath: "a", PSMODULEPATH: "b", Other: "c" }), { Other: "c" });
  // 足す値は残す。元の環境は変えない
  const out = windowsPowerShellEnv(env, { PL_ZIP: "z" });
  assert.equal(out.PL_ZIP, "z");
  assert.equal(env.PSModulePath, "C:/ps7/Modules");
  // powershell.exe を呼ぶ所は、どれもこの環境を渡している（release.mts とこのテスト）
  for (const f of ["release.mts", "release-scan.test.mts"]) {
    const src = readFileSync(join(project, "scripts", f), "utf8");
    const calls = [...src.matchAll(/(?:run|spawnSync)\("powershell\.exe"[^\n]*(?:\n[^\n]*){0,3}/g)].map((m) => m[0]);
    assert.ok(calls.length > 0, f);
    for (const c of calls) assert.match(c, /env: (?:psEnv|windowsPowerShellEnv\()/, `${f}: ${c.slice(0, 80)}`);
  }
});
