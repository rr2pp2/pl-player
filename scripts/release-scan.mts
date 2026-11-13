// 配る物の中に、ビルドした PC のフォルダの場所（ユーザーのホームなど）が残っていないかを探す。
// 配布用ビルド（scripts/release.mts）が使う。単独でも走らせられる：
//   node scripts/release-scan.mts <ファイルかフォルダ> ...   … 見つからなければ 0、見つかったら 1、
//     無いパス・ファイルが1つも無い・指定なしは 2（検査していないので 0 にしない）
// テスト：node --test scripts/release-scan.test.mts
//
// 探すもの（どれも実行のときに環境変数から求める。リポジトリに作者の PC のパスは書かない）
//   - ユーザーのホーム（USERPROFILE）・プロジェクトのフォルダ・cargo のホーム・rustup のホーム
//   - 「Users\<ユーザー名>」「Documents and Settings\<ユーザー名>」「home/<ユーザー名>」（ドライブを問わない）
//   - ユーザー名だけのフォルダ（\<ユーザー名>\）。ユーザー名が3文字以上のときだけ（1〜2文字だと、圧縮されたデータの偶然の並びに当たるため）
// 探し方
//   - 区切りは \ と / のどちらでも、続けて2つ（\\。文字列の中で書いた形）でも当てる。英字は大文字小文字を問わない
//   - 文字の形は ASCII / UTF-8 と UTF-16LE（偶数・奇数のどちらの位置から始まっても）の両方
//   - 名前の続きは当てない（ユーザー名が ab のとき、C:\Users\abc は別のフォルダ）。前の文字は問わない
//   - 実行ファイル（PE）は、リンカーが埋める PDB の場所（デバッグ情報の CodeView）も読み、フォルダが付いていたら失敗にする
//   - 参考（失敗にはしない）：ほかのユーザーのフォルダ（X:\Users\<名前>）と、8.3 形式の短い名前（\ABCDEF~1 の形）に見えるもの
import { existsSync, readFileSync, readdirSync, statSync, realpathSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { homedir, userInfo } from "node:os";

export type Needle = { label: string; source: RegExp; utf16: RegExp };
export type Hit = { file: string; label: string; encoding: "ascii/utf-8" | "utf-16le"; offset: number; context: string };
export type Roots = { home?: string; project?: string; cargoHome?: string; rustupHome?: string; temp?: string[]; user?: string };

const SEP = "[\\\\/]+";
// 名前の続きに見える文字（これが続くなら別のフォルダ）
const NAME_CONT = "(?![A-Za-z0-9_-])";
// 前の文字は問わない。Rust の文字列は区切りなしで並ぶので（「overflowC:\Users\…」の形）、前を見ると取りこぼす

function esc(c: string): string {
  // u フラグの正規表現でも使うので、- は逃がさない（u では \- が文法の誤りになる）
  return /[.*+?^${}()|[\]\\/]/.test(c) ? "\\" + c : c;
}

/** 1文字を、大文字小文字を問わない形にする（UTF-16 の検索用。JS の文字列にそのまま当てる） */
function charClass16(c: string): string {
  const lo = c.toLowerCase();
  const up = c.toUpperCase();
  if (lo !== up && lo.length === 1 && up.length === 1) return `[${esc(lo)}${esc(up)}]`;
  return esc(c);
}

/** 1文字を、UTF-8 のバイトを latin1 で読んだ文字列に当てる形にする。英字だけ大文字小文字を問わない */
function charClass8(c: string): string {
  if (/^[A-Za-z]$/.test(c)) return `[${c.toLowerCase()}${c.toUpperCase()}]`;
  if (c.charCodeAt(0) < 0x80) return esc(c);
  return [...Buffer.from(c, "utf8")].map((b) => "\\x" + b.toString(16).padStart(2, "0")).join("");
}

function partsOf(path: string): string[] {
  return path.replace(/[\\/]+/g, "\\").replace(/\\+$/, "").split("\\").filter((p) => p.length > 0);
}

/** パス（区切りの並び）を、区切りを問わない正規表現の元にする */
function pathSource(parts: string[], cls: (c: string) => string): string {
  return parts.map((p) => [...p].map(cls).join("")).join(SEP);
}

function needle(label: string, build: (cls: (c: string) => string) => string): Needle {
  return {
    label,
    source: new RegExp(build(charClass8), "g"),
    utf16: new RegExp(build(charClass16), "gu"),
  };
}

/** 絶対パスを探す形（C:\Users\<名前> と、その下）。ドライブ文字の大文字小文字も問わない */
export function pathNeedle(label: string, path: string): Needle {
  const parts = partsOf(path);
  return needle(label, (cls) => pathSource(parts, cls) + NAME_CONT);
}

/** ドライブを問わず「Users\<名前>」などを探す形 */
export function userNeedles(user: string): Needle[] {
  const out: Needle[] = [];
  for (const [label, base] of [
    ["Users\\<ユーザー名>", ["Users"]],
    ["Documents and Settings\\<ユーザー名>", ["Documents and Settings"]],
    ["home/<ユーザー名>", ["home"]],
  ] as const) {
    out.push(needle(label, (cls) => pathSource([...base, user], cls) + NAME_CONT));
  }
  if ([...user].length >= 3) {
    out.push(needle("\\<ユーザー名>\\", (cls) => "[\\\\/]" + pathSource([user], cls) + "[\\\\/]"));
  }
  return out;
}

/** 環境から、探すものを組み立てる。roots を渡せばそれを使う（テスト用） */
export function needlesFor(roots: Roots): Needle[] {
  const out: Needle[] = [];
  const add = (label: string, p: string | undefined) => {
    if (!p) return;
    const seen = new Set<string>();
    for (const v of variants(p)) {
      const key = partsOf(v).join("\\").toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(pathNeedle(label, v));
    }
  };
  // 細かいものを先に（同じ場所に当たったら、先に並べたほうの名前で知らせる）
  add("<プロジェクト>", roots.project);
  add("<cargo のホーム>", roots.cargoHome);
  add("<rustup のホーム>", roots.rustupHome);
  for (const t of roots.temp ?? []) add("<一時フォルダ>", t);
  add("<ユーザーのホーム>", roots.home);
  if (roots.user) out.push(...userNeedles(roots.user));
  return out;
}

/** 同じフォルダの、書き方の違い（環境変数のまま・実際の大文字小文字） */
export function variants(p: string): string[] {
  const out = [resolve(p)];
  try {
    out.push(realpathSync.native(p));
  } catch {
    // 無いフォルダは、書いたままの形だけ
  }
  return [...new Set(out)];
}

/** 今の PC の値。USERPROFILE・CARGO_HOME・RUSTUP_HOME が無ければ既定の場所 */
export function currentRoots(project: string, env: NodeJS.ProcessEnv = process.env): Roots {
  const home = env.USERPROFILE || homedir();
  let user = env.USERNAME || "";
  try {
    user = user || userInfo().username;
  } catch {
    // 取れなければ、ホームの最後のフォルダ名を使う
  }
  if (!user) user = partsOf(home).at(-1) ?? "";
  return {
    home,
    project,
    cargoHome: env.CARGO_HOME || join(home, ".cargo"),
    // 一時フォルダは 8.3 形式の短い名前（C:UsersABCDEF~1…）で書かれていることがあるので、そのまま探す
    temp: [...new Set([env.TEMP, env.TMP].filter((t): t is string => !!t))],
    rustupHome: env.RUSTUP_HOME || join(home, ".rustup"),
    user,
  };
}

function printable8(b: number): boolean {
  return b >= 0x20 && b < 0x7f;
}

/** 見つけた場所の前後の、読める文字の並び。探したものは <名前> に伏せる */
function contextOf(text: string, start: number, end: number, ok: (code: number) => boolean, needles: Needle[], utf16: boolean): string {
  let a = start;
  let b = end;
  while (a > 0 && start - a < 60 && ok(text.charCodeAt(a - 1))) a--;
  while (b < text.length && b - end < 60 && ok(text.charCodeAt(b))) b++;
  let s = text.slice(a, b);
  for (const n of needles) {
    // 写しを使う（同じ正規表現を外の exec の繰り返しが使っているので、lastIndex を戻さない）
    const re = utf16 ? n.utf16 : n.source;
    s = s.replace(new RegExp(re.source, re.flags), n.label);
  }
  // 伏せきれなかった、ASCII 以外のバイトは \xNN で見せる
  return s.replace(/[^\x20-\x7e\u3000-\u9fff\uff00-\uffef]/g, (c) => "\\x" + c.charCodeAt(0).toString(16).padStart(2, "0"));
}

/** 文字列（JS の文字列として読めるもの）の中の、探しているものを <名前> に伏せる */
export function maskText(s: string, needles: Needle[]): string {
  for (const n of needles) s = s.replace(new RegExp(n.utf16.source, n.utf16.flags), n.label);
  return s;
}

type Span = { label: string; encoding: Hit["encoding"]; start: number; end: number; context: string };

function findAll(text: string, needles: Needle[], utf16: boolean, encoding: Hit["encoding"], toOffset: (i: number) => number, ok: (c: number) => boolean): Span[] {
  const spans: Span[] = [];
  for (const n of needles) {
    const re = utf16 ? n.utf16 : n.source;
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      const s = toOffset(m.index);
      const e = toOffset(m.index + m[0].length);
      spans.push({ label: n.label, encoding, start: s, end: e, context: contextOf(text, m.index, m.index + m[0].length, ok, needles, utf16) });
      if (m[0].length === 0) re.lastIndex++;
    }
  }
  return spans;
}

/** バッファを探す。同じ場所に重なって当たったものは1つに数える（先に並べた探し物の名前で） */
export function scanBuffer(buf: Buffer, needles: Needle[], file = "<buffer>"): Hit[] {
  const spans: Span[] = [];
  const latin = buf.toString("latin1");
  spans.push(...findAll(latin, needles, false, "ascii/utf-8", (i) => i, printable8));
  for (const align of [0, 1]) {
    const len = (buf.length - align) & ~1;
    if (len <= 0) continue;
    const text = buf.subarray(align, align + len).toString("utf16le");
    spans.push(...findAll(text, needles, true, "utf-16le", (i) => align + i * 2, (c) => c >= 0x20 && c !== 0xfffd && !(c >= 0xd800 && c < 0xe000) && c < 0xfff0));
  }
  // 重なりをまとめる：始まりの順に並べ、前のものと重なれば捨てる
  const order = new Map(needles.map((n, i) => [n.label, i] as const));
  spans.sort((x, y) => x.start - y.start || (order.get(x.label) ?? 0) - (order.get(y.label) ?? 0));
  const hits: Hit[] = [];
  let lastEnd = -1;
  for (const s of spans) {
    if (s.start < lastEnd) continue;
    hits.push({ file, label: s.label, encoding: s.encoding, offset: s.start, context: s.context });
    lastEnd = s.end;
  }
  return hits;
}

/** PE（exe・dll）の CodeView デバッグ情報に書かれた PDB の名前。PE でなければ null */
export function pdbPaths(buf: Buffer): string[] | null {
  if (buf.length < 0x40 || buf.readUInt16LE(0) !== 0x5a4d) return null;
  const pe = buf.readUInt32LE(0x3c);
  if (pe + 24 > buf.length || buf.readUInt32LE(pe) !== 0x00004550) return null;
  const nSections = buf.readUInt16LE(pe + 6);
  const optSize = buf.readUInt16LE(pe + 20);
  const opt = pe + 24;
  const magic = buf.readUInt16LE(opt);
  const dirs = magic === 0x20b ? opt + 112 : magic === 0x10b ? opt + 96 : -1;
  if (dirs < 0) return null;
  const nDirs = buf.readUInt32LE(dirs - 4);
  if (nDirs <= 6) return [];
  const dbgRva = buf.readUInt32LE(dirs + 6 * 8);
  const dbgSize = buf.readUInt32LE(dirs + 6 * 8 + 4);
  if (dbgRva === 0 || dbgSize === 0) return [];
  const secs = opt + optSize;
  const toFile = (rva: number): number => {
    for (let i = 0; i < nSections; i++) {
      const s = secs + i * 40;
      const va = buf.readUInt32LE(s + 12);
      const vsize = Math.max(buf.readUInt32LE(s + 8), buf.readUInt32LE(s + 16));
      const raw = buf.readUInt32LE(s + 20);
      if (rva >= va && rva < va + vsize) return raw + (rva - va);
    }
    return -1;
  };
  const at = toFile(dbgRva);
  if (at < 0) return [];
  const out: string[] = [];
  for (let i = 0; i + 28 <= dbgSize; i += 28) {
    const d = at + i;
    if (d + 28 > buf.length) break;
    const type = buf.readUInt32LE(d + 12);
    const size = buf.readUInt32LE(d + 16);
    const ptr = buf.readUInt32LE(d + 24);
    if (type !== 2 || ptr + 24 > buf.length) continue; // 2 = CODEVIEW
    if (buf.toString("latin1", ptr, ptr + 4) !== "RSDS") continue;
    const end = buf.indexOf(0, ptr + 24);
    const stop = Math.min(end < 0 ? buf.length : end, ptr + size);
    out.push(buf.toString("utf8", ptr + 24, stop));
  }
  return out;
}

export type FileReport = { file: string; bytes: number; hits: Hit[]; pdb: string[] | null; pdbBad: string[]; otherUsers: string[]; shortNames: string[] };

/**
 * 参考：ほかのユーザーのフォルダ（X:\Users\<名前>）に見える名前の一覧。失敗にはしない（依存の中の例の文字などがありうるため）。
 * 今のユーザーなど、探しているものに当たるものは上の検査で数えているので、ここには入れない
 */
export function otherUserFolders(buf: Buffer, needles: Needle[]): string[] {
  const names = new Set<string>();
  const re = /[A-Za-z]:[\\/]+[uU][sS][eE][rR][sS][\\/]+([A-Za-z0-9_. -]{1,40})/g;
  const texts = [buf.toString("latin1")];
  for (const align of [0, 1]) {
    const len = (buf.length - align) & ~1;
    if (len > 0) texts.push(buf.subarray(align, align + len).toString("utf16le"));
  }
  for (const t of texts) {
    for (const m of t.matchAll(re)) {
      if (needles.some((n) => new RegExp(n.utf16.source, "u").test(m[0]))) continue;
      names.add(m[1].trim());
    }
  }
  return [...names].filter((n) => !/^(public|default|default user|all users)$/i.test(n)).sort();
}

/**
 * 参考：8.3 形式の短い名前に見えるもの。失敗にはしない。
 * 形：前が区切り（\ か /）、英数字と _ - が1〜6文字、~、数字。続きは区切り・拡張子の点・引用符・文字列の終わり（圧縮されたデータの偶然の並びを減らすため）。
 * 一時フォルダの環境変数に入っている形は上の検査で探している。ここは、それ以外の短い名前（ほかのフォルダ・ほかのユーザー）に気づくための網。
 * 今のユーザー名から作られる形に見えるもの（英数字だけにして比べ、頭の2文字以上がユーザー名と同じ）は、名前を伏せて <ユーザー名の短い形> と出す
 */
export function shortNameLike(buf: Buffer, user?: string): string[] {
  const re = /[\\/]([A-Za-z0-9_-]{1,6}~\d{1,3})(?=[\\/."'\x00])/g;
  // 比べるときは英数字だけにする（Windows が短い名前を作るときに落とす文字・残す文字の違いを問わない）
  const alnum = (s: string) => s.toUpperCase().replace(/[^A-Z0-9]/g, "");
  const head = alnum(user ?? "");
  const names = new Set<string>();
  const texts = [buf.toString("latin1")];
  for (const align of [0, 1]) {
    const len = (buf.length - align) & ~1;
    if (len > 0) texts.push(buf.subarray(align, align + len).toString("utf16le"));
  }
  for (const t of texts) {
    for (const m of t.matchAll(re)) {
      const n = m[1].toUpperCase();
      const a = alnum(n.slice(0, n.indexOf("~")));
      const k = Math.min(head.length, a.length);
      names.add(k >= 2 && a.slice(0, k) === head.slice(0, k) ? "<ユーザー名の短い形>" : n);
    }
  }
  return [...names].sort();
}

export function scanFile(file: string, needles: Needle[], user?: string): FileReport {
  const buf = readFileSync(file);
  const pdb = pdbPaths(buf);
  const pdbBad = (pdb ?? []).filter((p) => /[\\/:]/.test(p));
  return { file, bytes: buf.length, hits: scanBuffer(buf, needles, file), pdb, pdbBad, otherUsers: otherUserFolders(buf, needles), shortNames: shortNameLike(buf, user) };
}

/** 失敗にするもの：探しているものが見つかった、または PDB の名前にフォルダが付いている */
export function isBad(r: FileReport): boolean {
  return r.hits.length > 0 || r.pdbBad.length > 0;
}

/** フォルダなら中のファイルを全部（下のフォルダも） */
export function listFiles(p: string): string[] {
  const st = statSync(p);
  if (!st.isDirectory()) return [p];
  const out: string[] = [];
  for (const e of readdirSync(p, { withFileTypes: true })) {
    const q = join(p, e.name);
    if (e.isDirectory()) out.push(...listFiles(q));
    else if (e.isFile()) out.push(q);
  }
  return out;
}

/** 報告の文。探したもの自体は書かない（<名前> に伏せてある） */
export function formatReport(reports: FileReport[], shown = (f: string) => f, maxSamples = 20): string {
  const lines: string[] = [];
  for (const r of reports) {
    const bad = isBad(r);
    const pdbNote = r.pdb === null ? "" : ` PDB の名前: ${r.pdb.length === 0 ? "なし" : r.pdbBad.length > 0 ? "フォルダ付き（失敗）" : r.pdb.join(", ")}`;
    lines.push(`${bad ? "NG" : "OK"} ${shown(r.file)}（${r.bytes} バイト）見つかった場所 ${r.hits.length} 件${pdbNote}`);
    if (r.hits.length > 0) {
      const byLabel = new Map<string, number>();
      for (const h of r.hits) {
        const k = `${h.label} / ${h.encoding}`;
        byLabel.set(k, (byLabel.get(k) ?? 0) + 1);
      }
      for (const [k, n] of byLabel) lines.push(`    ${k}: ${n} 件`);
      for (const h of r.hits.slice(0, maxSamples)) lines.push(`    @0x${h.offset.toString(16)} [${h.encoding}] ${h.context}`);
      if (r.hits.length > maxSamples) lines.push(`    …ほか ${r.hits.length - maxSamples} 件`);
    }
    if (r.otherUsers.length > 0) lines.push(`    参考（失敗にはしない）：ほかのユーザーのフォルダに見える名前 ${r.otherUsers.join(", ")}`);
    if (r.shortNames.length > 0) {
      const more = r.shortNames.length > 20 ? ` …ほか ${r.shortNames.length - 20} 個` : "";
      lines.push(`    参考（失敗にはしない）：8.3 形式の短い名前に見えるもの ${r.shortNames.slice(0, 20).join(", ")}${more}`);
    }
  }
  return lines.join("\n");
}

// NSIS の命令のうち、パスをビルドのときにだけ使うもの（インストーラーの中に文字として残らない）
const NSIS_BUILD_TIME = /^\s*(!define|!undef|!include|!addincludedir|!addplugindir|!if|!ifdef|!ifndef|!else|!endif|!insertmacro\s+MUI_PAGE_LICENSE|File|Icon|UninstallIcon|LicenseData|OutFile)\b/i;

/**
 * Tauri が作る NSIS の台本（installer.nsi と .nsh）で、探しているパスが「インストーラーを動かすときの文字」に入らないかを確かめる。
 * インストーラーの台本の部分は圧縮されていて、7-Zip でも取り出せないため、元の台本で確かめる。
 * パスを持つ !define の名前を追い（その名前を使う !define も追う）、ビルドのときだけ使う命令の外で使われていたら、その行を返す
 */
export function nsisRuntimePathLines(script: string, needles: Needle[]): string[] {
  const has = (s: string) => needles.some((n) => new RegExp(n.utf16.source, "u").test(s));
  const defs = new Map<string, string>();
  for (const m of script.matchAll(/^\s*!define\s+(?:\/\w+\s+)*([A-Za-z_][\w.]*)\s*(.*)$/gm)) defs.set(m[1], m[2]);
  const tainted = new Set<string>();
  for (let changed = true; changed; ) {
    changed = false;
    for (const [name, value] of defs) {
      if (tainted.has(name)) continue;
      if (has(value) || [...tainted].some((t) => value.includes("${" + t + "}"))) {
        tainted.add(name);
        changed = true;
      }
    }
  }
  const out: string[] = [];
  script.split(/\r?\n/).forEach((line, i) => {
    const uses = has(line) || [...tainted].some((t) => line.includes("${" + t + "}"));
    if (uses && !NSIS_BUILD_TIME.test(line)) out.push(`${i + 1}: ${maskText(line.trim(), needles).slice(0, 160)}`);
  });
  return out;
}

// 単独で走らせたとき。終了コード：0 = 見つからなかった、1 = 見つかった、2 = 検査できなかった（指定なし・無いパス・ファイルが0個）
if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args.length === 0) {
    console.error("使い方: node scripts/release-scan.mts <ファイルかフォルダ> ...");
    process.exit(2);
  }
  // 無いパスがあっても、あるものは検査して結果を出す。終了コードは、見つかれば 1 を先にする（無いパスがあっても 1）。
  // 見つからず、無いパスがある・ファイルが1つも無いときは 2（全部を検査できていない）
  const missing = args.filter((a) => !existsSync(a));
  const project = fileURLToPath(new URL("..", import.meta.url));
  const roots = currentRoots(project);
  const needles = needlesFor(roots);
  const files = args.filter((a) => existsSync(a)).flatMap((a) => listFiles(a));
  const reports = files.map((f) => scanFile(f, needles, roots.user));
  const bad = reports.filter(isBad).length;
  if (reports.length > 0) {
    // 今いるフォルダからの相対で出す（ホームの場所を画面に出さないため）
    console.log(formatReport(reports, (f) => relative(process.cwd(), f) || f));
    console.log(bad === 0 ? `見つかりませんでした（${reports.length} ファイル）` : `見つかりました（${bad} ファイル）`);
  }
  if (missing.length > 0) console.error(`見つからないパスがあります：${missing.join("、")}（そこは検査していません）`);
  if (files.length === 0 && missing.length < args.length) console.error("検査するファイルが1つもありませんでした（空のフォルダ？）");
  process.exit(bad > 0 ? 1 : missing.length > 0 || files.length === 0 ? 2 : 0);
}
