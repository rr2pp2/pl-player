// 配布用ビルド：GitHub Releases に置く物を作る（作者が配るときだけ使う。改造して自分で使うだけなら npx tauri build でよい）。
// 走らせ方（プロジェクトのフォルダで）：
//   npm run release                 … ビルド → 検査 → release\ に Releases に置く4つ（zip・インストーラー・インストールせずに使う exe・SHA256 の一覧）を作る
//   npm run release -- --skip-build … ビルドせず、前に作った物（src-tauri\target\dist）を検査して組み立てる。
//                                     ビルドしたときの記録（build-info.json）と今のソースの中身・版・exe を比べ、違えば止まる（コミットしただけで中身が同じなら通す）
//   npm run release -- --no-7zip    … 7-Zip が無い PC で、インストーラーの中身を取り出す検査を飛ばす（README の「8. 作者が配るときの手順」の限界）
//   npm run release -- --out=<フォルダ> … release\ の代わりにそこへ組み立てる（組み立ての試し用。src-tauri\target\ の下だけ。dist とその中は断る）
//   npm run release -- --no-clean   … 前のビルドの残り（src-tauri\target\dist）を使って速く組む（検査の試し用。配る物は既定の、消してから組むほうで作る）
//
// やること
//   1. 版の番号が package.json・tauri.conf.json・Cargo.toml で同じか、リポジトリのファイルに作者の PC のパスが無いかを確かめる。
//      ビルドに要るもの（NSIS・Tauri の CLI・7-Zip）と --out の場所も、前の物を消す前にここで全部確かめる
//   2. フォルダの場所を中身の分からない名前に置き換えてビルドする（rustc の --remap-path-prefix。パスはこの PC の環境変数から求める）
//      置き換え先：ユーザーのホーム → /build/home、一時フォルダ → /build/tmp、cargo のホーム → /build/cargo、rustup のホーム → /build/rustup、プロジェクト → /build/src
//      C のコード（libopus・SQLite）の __FILE__ は、cl.exe の /d1trimfile で同じフォルダの頭を落とす（CFLAGS・CXXFLAGS に足す）
//      通信しない（CARGO_NET_OFFLINE=true。NSIS は手元にあるものだけを使い、無ければ止まる）
//      置き場所は src-tauri\target\dist（ふだんの開発のビルドと分ける。混ざると置き換えていない物が入りうるため）
//      終わったら src-tauri\target\dist\build-info.json に、版・git のコミット・ソースの中身の指紋（git が見ているファイルの名前と中身）・exe とインストーラーの SHA256 を書く
//   3. 作った物の中を検査し、ユーザーのホーム・ユーザー名を含むパスが1つでもあれば止まる（scripts/release-scan.mts）
//      インストールせずに使う exe・インストーラー・インストーラーから 7-Zip で取り出した中身・画面のビルド結果（dist）・NSIS の台本
//   4. release\ に組み立てる。前の物があれば release\控え\<日時>\ へ動かす（消さない）
//      - Releases に置く物：PL-Player-<版>.zip・PL-Player-<版>-x64-setup.exe・PL-Player-<版>-x64-portable.exe・PL-Player-<版>.sha256.txt
//        GitHub は空白や英数字以外を含む資産の名前を変えるので、英数字と . _ - だけの名前にする。SHA256 の一覧もこの名前（フォルダなし）で書く
//      - release\PL-Player-<版>\：zip の中身と同じもの（日本語の名前の exe 2つと、使う人向けの README（scripts/release-README.md から）と、
//        v3.9.1 から「見本」フォルダ（立ち絵とスキンの見本と使い方。scripts/release-samples.mts））
//   5. zip を Windows の Expand-Archive で展開し、中身が置いた物と同じかを SHA256 で確かめる。SHA256 の一覧も読み直して照らす
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { closeSync, copyFileSync, cpSync, existsSync, mkdirSync, openSync, readdirSync, readFileSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve, win32 } from "node:path";
import { fileURLToPath } from "node:url";
import { currentRoots, formatReport, isBad, listFiles, maskText, needlesFor, nsisRuntimePathLines, scanFile, variants, type FileReport, type Roots } from "./release-scan.mts";
import { makeZip } from "./release-zip.mts";
import { SAMPLE_DIR, sampleFiles, type SampleFile } from "./release-samples.mts";

const PORTABLE_NAME = "PL Player 3（インストールせずに使う）.exe";

/** GitHub Releases に置く物の名前。GitHub が名前を変えない文字（英数字と . _ -）だけにする */
export function assetNames(version: string) {
  const base = `PL-Player-${version}`;
  return { base, zip: `${base}.zip`, setup: `${base}-x64-setup.exe`, portable: `${base}-x64-portable.exe`, sums: `${base}.sha256.txt` };
}

/** GitHub が名前を変えない形か（英数字で始まり英数字で終わり、間は英数字と . _ -。GitHub は空白・英数字以外・頭と終わりの点を含む名前を変える） */
export function isSafeAssetName(name: string): boolean {
  return /^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$/.test(name);
}

/** SHA256 の一覧（sha256sum の形。「<64桁> *<名前>」、改行は LF）。名前はフォルダを付けない資産の名前だけ */
export function sumsText(items: { name: string; hash: string }[]): string {
  for (const it of items) {
    if (!isSafeAssetName(it.name)) throw new Error(`SHA256 の一覧に書けない名前です（フォルダ付き・GitHub が変える文字）：${it.name}`);
    if (!/^[0-9a-f]{64}$/.test(it.hash)) throw new Error(`SHA256 の形ではありません：${it.hash}`);
  }
  return items.map((it) => `${it.hash} *${it.name}\n`).join("");
}

/** SHA256 の一覧を、dir に置いたファイルと照らす（sha256sum -c と同じ見方）。無いファイルは missing */
export function checkSums(text: string, dir: string): { name: string; result: "ok" | "ng" | "missing" | "bad-line" }[] {
  const out: { name: string; result: "ok" | "ng" | "missing" | "bad-line" }[] = [];
  for (const line of text.split("\n")) {
    if (line.length === 0) continue;
    const m = /^([0-9a-fA-F]{64}) [ *](.+)$/.exec(line);
    if (!m) {
      out.push({ name: line, result: "bad-line" });
      continue;
    }
    const f = join(dir, m[2]);
    if (!existsSync(f)) out.push({ name: m[2], result: "missing" });
    else out.push({ name: m[2], result: sha256(f) === m[1].toLowerCase() ? "ok" : "ng" });
  }
  return out;
}

/**
 * フォルダの場所を、実際の長い名前に直す（8.3 形式の短い名前・ジャンクションを解く）。
 * まだ無いフォルダは、あるところまで上がって解き、無い部分をそのまま後ろにつなぐ
 */
export function longPath(p: string): string {
  const rest: string[] = [];
  let cur = win32.resolve(p);
  for (;;) {
    try {
      const real = realpathSync.native(cur);
      return rest.length > 0 ? win32.join(real, ...rest.reverse()) : real;
    } catch {
      const parent = win32.dirname(cur);
      if (parent === cur) return win32.join(cur, ...rest.reverse());
      rest.push(win32.basename(cur));
      cur = parent;
    }
  }
}

/** Windows の予約された名前（拡張子が付いていても、その名前の装置を指す） */
const RESERVED_NAME = /^(con|prn|aux|nul|com[0-9¹²³]|lpt[0-9¹²³]|conin\$|conout\$)$/i;

const OUT_ALLOWED = "--out は src-tauri\\target\\ の下のフォルダにしてください（組み立ての試し用。.git・src などを書き換えないよう、ほかの場所は断る）";

/**
 * --out の場所を確かめ、組み立てる先（長い名前に直したもの）を返す。よくなければ理由を返す。
 * 許すのは src-tauri\target\ の下だけ（target そのもの・ビルドのたびに消える src-tauri\target\dist とその中は断る）。
 * 比べる前に、それぞれの名前の末尾の . と空白を落とし（Windows は落とした名前として扱う）、8.3 形式の短い名前は長い名前に直す。
 * 使えない文字（< > : " | ? * と制御文字）・予約された名前（CON・NUL・COM1 など）・末尾が . か空白の名前は断る
 */
export function resolveOutDir(project: string, out: string, toLong: (p: string) => string = longPath): { dir: string } | { problem: string } {
  if (out.length === 0) return { problem: "--out= の後ろが空です" };
  const full = win32.resolve(project, out);
  const root = win32.parse(full).root;
  const names = full.slice(root.length).split("\\").filter((s) => s.length > 0);
  const trimmed: string[] = [];
  let trailing: string | null = null;
  for (const name of names) {
    if (/[<>:"|?*\x00-\x1f]/.test(name)) return { problem: `--out に Windows で使えない文字があります（${JSON.stringify(name)}）` };
    const t = name.replace(/[. ]+$/, "");
    if (t.length === 0) return { problem: `--out に使えない名前があります（${JSON.stringify(name)}）` };
    if (RESERVED_NAME.test(t.split(".")[0].replace(/ +$/, ""))) return { problem: `--out に Windows の予約された名前があります（${name}。CON・NUL・COM1 などは装置を指す）` };
    if (t !== name && trailing === null) trailing = name;
    trimmed.push(t);
  }
  const dir = toLong(root + trimmed.join("\\"));
  const target = toLong(win32.join(project, "src-tauri", "target"));
  const dist = toLong(win32.join(project, "src-tauri", "target", "dist"));
  const inside = (child: string, parent: string) => {
    const rel = win32.relative(parent, child);
    return rel !== "" && !win32.isAbsolute(rel) && rel.split("\\")[0] !== "..";
  };
  if (!inside(dir, target)) return { problem: OUT_ALLOWED };
  if (win32.relative(dist, dir) === "" || inside(dir, dist)) return { problem: "--out に src-tauri\\target\\dist とその中は使えません（ビルドのたびに消える場所）" };
  if (trailing !== null) return { problem: `--out の名前の末尾に . か空白があります（${JSON.stringify(trailing)}。Windows では名前が変わるので使わない）` };
  return { dir };
}

/** resolveOutDir の理由だけ（よければ null） */
export function outDirProblem(project: string, out: string, toLong?: (p: string) => string): string | null {
  const r = resolveOutDir(project, out, toLong);
  return "problem" in r ? r.problem : null;
}

/**
 * 今のソースの状態。content は、git が見ているファイル（追跡しているもの・まだ足していないもの。.gitignore で外したものは除く）の
 * 今の中身の指紋（名前と中身から作る SHA256）。コミットしても中身が同じなら変わらない。head・dirty・changedFiles は知らせるためだけに使う
 */
export type SourceState = { git: boolean; head: string | null; dirty: boolean | null; changedFiles: number; content: string | null };

export function sourceState(project: string): SourceState {
  const none: SourceState = { git: false, head: null, dirty: null, changedFiles: 0, content: null };
  const g = (args: string[]) => spawnSync("git", args, { cwd: project, windowsHide: true, maxBuffer: 1 << 30 });
  const list = (r: ReturnType<typeof g>) => r.stdout.toString("utf8").split("\0").filter((f) => f.length > 0);
  const head = g(["rev-parse", "--verify", "HEAD"]);
  if (head.error || head.status !== 0) return none;
  const names = g(["diff", "HEAD", "--name-only", "-z"]);
  const untracked = g(["ls-files", "-z", "--others", "--exclude-standard"]);
  const seen = g(["ls-files", "-z", "--cached", "--others", "--exclude-standard"]);
  if (names.status !== 0 || untracked.status !== 0 || seen.status !== 0) return none;
  const h = createHash("sha256");
  for (const f of [...new Set(list(seen))].sort()) {
    const p = join(project, f);
    let st;
    try {
      st = statSync(p);
    } catch {
      continue; // 消したファイル（コミットして消しても同じ指紋になる）
    }
    if (!st.isFile()) {
      h.update(`\0${f}\0<フォルダ>\0`);
      continue;
    }
    let data: Buffer;
    try {
      data = readFileSync(p);
    } catch {
      data = Buffer.from("<読めない>");
    }
    h.update(`\0${f}\0${data.length}\0`);
    h.update(data);
  }
  const changed = list(names).length + list(untracked).length;
  return { git: true, head: head.stdout.toString("utf8").trim(), dirty: changed > 0, changedFiles: changed, content: h.digest("hex") };
}

/** 知らせるための形：「コミット <先頭10桁>・まだコミットしていない変更 <数> ファイル」 */
export function describeSource(s: SourceState): string {
  if (!s.git) return "git が使えない";
  return `コミット ${s.head?.slice(0, 10)}・まだコミットしていない変更 ${s.dirty ? `${s.changedFiles} ファイル` : "なし"}`;
}

/** ビルドしたときの記録（src-tauri\target\dist\build-info.json） */
export type BuildInfo = { version: string; builtAt: string; clean: boolean; source: SourceState; exeSha256: string; setupSha256: string };

/**
 * --skip-build のとき、記録と今を比べる。違いがあれば、その理由の一覧（空なら同じ）。
 * forTrial は --out 付き（組み立ての試し）。--no-clean で作った物は、試しのときだけ通す
 */
export function buildInfoProblems(rec: BuildInfo, now: { version: string; source: SourceState; exeSha256: string; setupSha256: string }, forTrial: boolean): string[] {
  const p: string[] = [];
  if (rec.version !== now.version) p.push(`版：ビルドしたときと違います（ビルドしたとき ${rec.version}／今 ${now.version}）`);
  if (!rec.source.git || !now.source.git) {
    p.push("ソースの中身：git が使えない（ビルドしたときか今）ので、変わっていないかを確かめられません");
  } else if (typeof rec.source.content !== "string") {
    p.push("ソースの中身：ビルドしたときの記録が古い形で、中身の指紋がありません");
  } else if (rec.source.content !== now.source.content) {
    p.push(`ソースの中身が、ビルドしたときと違います（ビルドしたとき：${describeSource(rec.source)}／今：${describeSource(now.source)}）。コミットしただけで中身が同じなら止まりません`);
  }
  if (rec.exeSha256 !== now.exeSha256) p.push("exe：記録したあとに差し替えられています（src-tauri\\target\\dist\\release\\pl-player.exe）");
  if (rec.setupSha256 !== now.setupSha256) p.push("インストーラー：記録したあとに差し替えられています（src-tauri\\target\\dist\\release\\bundle\\nsis）");
  if (!rec.clean && !forTrial) p.push("作り方：--no-clean で作った物です（配る物には使わない。--out を付けた試しのときだけ通す）");
  return p;
}
const MARKER_UNK = "__TAURI_BUNDLE_TYPE_VAR_UNK";
const MARKER_NSIS = "__TAURI_BUNDLE_TYPE_VAR_NSS";

/** 置き換えの組（FROM, TO）。rustc は「最後に当たったもの」を使うので、短い（上の）フォルダから順に並べる */
export function remapPairs(roots: Roots): [string, string][] {
  const groups: [string | undefined, string][] = [
    [roots.home, "/build/home"],
    ...(roots.temp ?? []).map((t) => [t, "/build/tmp"] as [string, string]),
    [roots.cargoHome, "/build/cargo"],
    [roots.rustupHome, "/build/rustup"],
    [roots.project, "/build/src"],
  ];
  const pairs: [string, string][] = [];
  for (const [p, to] of groups) {
    if (!p) continue;
    for (const v of variants(p)) {
      const base = v.replace(/[\\/]+$/, "");
      // 書き方の違い：区切り（\ と /）、ドライブ文字の大文字小文字
      for (const s of [base, base.replace(/\\/g, "/")]) {
        pairs.push([s, to]);
        if (/^[A-Za-z]:/.test(s)) {
          pairs.push([s[0].toLowerCase() + s.slice(1), to]);
          pairs.push([s[0].toUpperCase() + s.slice(1), to]);
        }
      }
    }
  }
  const seen = new Set<string>();
  const uniq = pairs.filter(([f]) => (seen.has(f) ? false : (seen.add(f), true)));
  // 長さの順（安定ソート）。上のフォルダが先、下のフォルダ（プロジェクトなど）が後になり、後のものが勝つ
  return uniq.sort((a, b) => a[0].length - b[0].length);
}

/**
 * C のコード（libopus・SQLite。cc と cmake のクレートが MSVC の cl.exe で組む）の __FILE__ から、フォルダの場所を落とす指定。
 * cl.exe の /d1trimfile:<前> は、__FILE__ の頭の <前> を落とす（大文字小文字・\ と / を問わない。複数あれば先に書いたものが勝つので、長い順に並べる）。
 * Microsoft の文書には無い指定なので、効いたかは検査で確かめる。
 * cc は CFLAGS を空白で切り、cmake は CMAKE_C_FLAGS に空白でつなぐので、空白を含むフォルダは渡せない（skipped に返す。残れば検査で止まる）
 */
export function cTrimFlags(roots: Roots): { flags: string[]; skipped: string[] } {
  const prefixes = new Map<string, string>();
  for (const p of [roots.home, ...(roots.temp ?? []), roots.cargoHome, roots.rustupHome, roots.project]) {
    if (!p) continue;
    for (const v of variants(p)) {
      const fwd = v.replace(/[\\/]+/g, "/").replace(/\/+$/, "") + "/";
      prefixes.set(fwd.toLowerCase(), fwd);
    }
  }
  const all = [...prefixes.values()].sort((a, b) => b.length - a.length);
  return {
    flags: all.filter((p) => !/\s/.test(p)).map((p) => `/d1trimfile:${p}`),
    skipped: all.filter((p) => /\s/.test(p)),
  };
}

/** CARGO_ENCODED_RUSTFLAGS の値（区切りは 0x1f）。前からある指定は残し、うしろに足す */
export function encodedRustflags(env: NodeJS.ProcessEnv, pairs: [string, string][]): string {
  const before = env.CARGO_ENCODED_RUSTFLAGS
    ? env.CARGO_ENCODED_RUSTFLAGS.split("\x1f").filter((s) => s.length > 0)
    : (env.RUSTFLAGS ?? "").split(/\s+/).filter((s) => s.length > 0);
  return [...before, ...pairs.map(([f, t]) => `--remap-path-prefix=${f}=${t}`)].join("\x1f");
}

function sha256(file: string): string {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

function fail(msg: string): never {
  console.error(`\n止めました：${msg}`);
  process.exit(1);
}

function step(msg: string) {
  console.log(`\n== ${msg}`);
}

function run(cmd: string, args: string[], opts: { cwd: string; env?: NodeJS.ProcessEnv; quiet?: boolean }): { status: number | null; stdout: string } {
  const r = spawnSync(cmd, args, { cwd: opts.cwd, env: opts.env ?? process.env, stdio: opts.quiet ? ["ignore", "pipe", "pipe"] : "inherit", encoding: "utf8", windowsHide: true });
  if (r.error) return { status: -1, stdout: String(r.error) };
  return { status: r.status, stdout: (r.stdout ?? "") + (r.stderr ?? "") };
}

function find7zip(env: NodeJS.ProcessEnv): string | null {
  for (const base of [env.ProgramFiles, env["ProgramFiles(x86)"], env.ProgramW6432]) {
    if (!base) continue;
    const p = join(base, "7-Zip", "7z.exe");
    if (existsSync(p)) return p;
  }
  const w = spawnSync("where", ["7z"], { encoding: "utf8", windowsHide: true });
  const first = (w.stdout ?? "").split(/\r?\n/)[0]?.trim();
  return w.status === 0 && first ? first : null;
}

/**
 * Windows PowerShell 5.1（powershell.exe）を呼ぶときに渡す環境：PSModulePath を外す（名前の大文字小文字は問わない）。
 * PowerShell 7 の中から npm run release を走らせると、7 は PSModulePath の頭に 7 用のモジュールの場所を足している。node をはさむと
 * それがそのまま 5.1 に渡り、5.1 が 7 用の Microsoft.PowerShell.Archive を読んで、Expand-Archive が
 * 「"ExtractToDirectory" のオーバーロードで、引数の数が "3" であるものが見つかりません」で止まった（v3.3.0 で作者が踏んだ）。
 * PSModulePath が無ければ、5.1 は起動のときに自分の既定（ユーザーと全ユーザーの WindowsPowerShell\Modules と $PSHOME\Modules）を組み立てる。
 * Microsoft の about_PSModulePath（Starting Windows PowerShell from PowerShell 7）に、間に別のプロセスをはさむときの直し方として、この形が書いてある
 */
export function windowsPowerShellEnv(env: NodeJS.ProcessEnv, extra: Record<string, string> = {}): NodeJS.ProcessEnv {
  const out: NodeJS.ProcessEnv = {};
  for (const [k, v] of Object.entries(env)) if (k.toLowerCase() !== "psmodulepath") out[k] = v;
  return { ...out, ...extra };
}

/** cmake が PATH に無ければ、Visual Studio（Build Tools）に入っているものを足す（libopus を組むのに要る） */
function withCmake(env: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  if (spawnSync("where", ["cmake"], { windowsHide: true }).status === 0) return env;
  const vswhere = join(env["ProgramFiles(x86)"] ?? "C:\\Program Files (x86)", "Microsoft Visual Studio", "Installer", "vswhere.exe");
  if (!existsSync(vswhere)) return env;
  const r = spawnSync(vswhere, ["-latest", "-products", "*", "-property", "installationPath"], { encoding: "utf8", windowsHide: true });
  const inst = (r.stdout ?? "").split(/\r?\n/)[0]?.trim();
  if (!inst) return env;
  const bin = join(inst, "Common7", "IDE", "CommonExtensions", "Microsoft", "CMake", "CMake", "bin");
  if (!existsSync(join(bin, "cmake.exe"))) return env;
  console.log("cmake が PATH に無いので、Visual Studio に入っているものを使います");
  const key = Object.keys(env).find((k) => k.toLowerCase() === "path") ?? "Path";
  return { ...env, [key]: `${env[key] ?? ""};${bin}` };
}

/** 2つの exe が、Tauri がインストーラー用に書き換える印（__TAURI_BUNDLE_TYPE_VAR_…）のほかは同じか */
export function sameExceptBundleMarker(a: Buffer, b: Buffer): { same: boolean; diffBytes: number; markerAt: number } {
  if (a.length !== b.length) return { same: false, diffBytes: -1, markerAt: -1 };
  const at = a.indexOf(MARKER_UNK, 0, "latin1");
  let diff = 0;
  let outside = 0;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      diff++;
      if (at < 0 || i < at || i >= at + MARKER_UNK.length) outside++;
    }
  }
  const markerOk = at >= 0 && b.toString("latin1", at, at + MARKER_NSIS.length) === MARKER_NSIS;
  return { same: outside === 0 && (diff === 0 || markerOk), diffBytes: diff, markerAt: at };
}

/**
 * 前の物を控えへ動かす。フォルダをエクスプローラーで開いているなどで動かせないときは、
 * 写してから（中身が同じかを確かめて）元の中身だけを消す（フォルダは空にして残し、そのまま使う）
 */
function keepAside(src: string, dest: string) {
  try {
    renameSync(src, dest);
    return;
  } catch (e) {
    const code = (e as NodeJS.ErrnoException).code;
    if (code !== "EBUSY" && code !== "EPERM" && code !== "EACCES") throw e;
    if (!statSync(src).isDirectory()) throw e;
  }
  cpSync(src, dest, { recursive: true, errorOnExist: true, force: false });
  for (const f of listFiles(src)) {
    const copy = join(dest, relative(src, f));
    if (sha256(copy) !== sha256(f)) fail(`控えへの写しが元と違います：${relative(src, f)}`);
  }
  for (const e of readdirSync(src)) rmSync(join(src, e), { recursive: true, force: true });
  console.log(`  ${basename(src)} は動かせなかったので（開いているアプリがある？）、控えへ写してから中身を空にしました`);
}

function timestamp(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

/** 使う人向けの README を型から作る。「<版> で変えたこと」の見出しが無ければ止める（前の版の中身のまま配らないため） */
export function renderReadme(template: string, version: string): string {
  // 改行は LF にそろえる（作業フォルダの型が CRLF で書き出されていても、配る README は同じ形にする）
  const out = template.replace(/\r\n?/g, "\n").replace(/\{\{VERSION\}\}/g, version);
  const heading = new RegExp(`^## ${version.replace(/\./g, "\\.")} で変えたこと\\s*$`, "m");
  if (!heading.test(out)) throw new Error(`README の型に「## ${version} で変えたこと」の見出しがありません。scripts/release-README.md の変えたことを、今の版に書き換えてください`);
  if (/\{\{[A-Z_]+\}\}/.test(out)) throw new Error("README の型に、埋めていない {{…}} が残っています");
  return out;
}

/**
 * 型を埋めた README の中の、今の版と違う版番号（x.y.z）を、行の番号つきで返す（知らせるだけ。止めない）。
 * 「3.0.0 以降と同じアプリ」のような正しいものも出るので、作者が目で確かめる。見出しだけ書き換えて本文に前の版が残る形に気づくため
 */
export function otherVersions(text: string, version: string): { line: number; version: string; text: string }[] {
  const out: { line: number; version: string; text: string }[] = [];
  text.split("\n").forEach((l, i) => {
    for (const m of l.matchAll(/(?<![\d.])(\d+\.\d+\.\d+)(?![\d.]*\d)/g)) {
      if (m[1] !== version) out.push({ line: i + 1, version: m[1], text: l.trim().slice(0, 80) });
    }
  });
  return out;
}

function main() {
  const args = new Set(process.argv.slice(2));
  const skipBuild = args.has("--skip-build");
  const no7zip = args.has("--no-7zip");
  const noClean = args.has("--no-clean");
  const known = new Set(["--skip-build", "--no-7zip", "--no-clean"]);
  const outArg = [...args].find((a) => a.startsWith("--out="));
  for (const a of args) if (!known.has(a) && a !== outArg) fail(`知らない指定です：${a}（使えるのは ${[...known].join("・")}・--out=<フォルダ>）`);
  if ([...args].filter((a) => a.startsWith("--out=")).length > 1) fail("--out は1つだけにしてください");
  if (process.platform !== "win32") fail("Windows で走らせてください");

  const project = resolve(fileURLToPath(new URL("..", import.meta.url)));
  const tauriDir = join(project, "src-tauri");
  const targetDir = join(tauriDir, "target", "dist");
  const checkDir = join(targetDir, "release-check");
  const buildInfoPath = join(targetDir, "build-info.json");
  const shown = (p: string) => relative(project, p) || ".";

  const roots = currentRoots(project);
  const needles = needlesFor(roots);

  // ---- 1. 前の確かめ（ここで止まるなら、何も消さず何も動かさない）
  step("1/5 前の確かめ");
  // 置き場所：既定は release\。--out=<フォルダ> は組み立ての試し用（src-tauri\target\ の下だけ）。ビルドの前に確かめる
  let releaseDir = join(project, "release");
  if (outArg) {
    const r = resolveOutDir(project, outArg.slice("--out=".length));
    if ("problem" in r) fail(r.problem);
    releaseDir = r.dir;
  }

  const version = JSON.parse(readFileSync(join(project, "package.json"), "utf8")).version as string;
  const confVersion = JSON.parse(readFileSync(join(tauriDir, "tauri.conf.json"), "utf8")).version as string;
  const cargoVersion = /^\s*version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(tauriDir, "Cargo.toml"), "utf8"))?.[1];
  if (!/^\d+\.\d+\.\d+$/.test(version)) fail(`package.json の版が読めません（${version}）`);
  if (version !== confVersion || version !== cargoVersion) fail(`版が揃っていません（package.json ${version}／tauri.conf.json ${confVersion}／Cargo.toml ${cargoVersion}）`);
  console.log(`版：${version}`);
  const productName = JSON.parse(readFileSync(join(tauriDir, "tauri.conf.json"), "utf8")).productName as string;
  const setupName = `${productName}_${version}_x64-setup.exe`;
  const assets = assetNames(version);
  for (const n of [assets.zip, assets.setup, assets.portable, assets.sums]) if (!isSafeAssetName(n)) fail(`Releases に置く名前に、GitHub が変える文字があります：${n}`);

  let readme: string;
  try {
    readme = renderReadme(readFileSync(join(project, "scripts", "release-README.md"), "utf8"), version);
  } catch (e) {
    fail((e as Error).message);
  }
  // v3.9.1：見本フォルダの中身（絵に作った PC・ソフトを書ける塊があれば、ここで止まる）
  let samples: SampleFile[];
  try {
    samples = sampleFiles(project, version);
  } catch (e) {
    fail((e as Error).message);
  }
  // v3.11.0：アイコンのフォルダはファイルが多いので、フォルダごとの数だけを出す
  const sampleTop = samples.filter((f) => !f.name.includes("/")).map((f) => f.name);
  const sampleDirs = [...new Set(samples.filter((f) => f.name.includes("/")).map((f) => f.name.slice(0, f.name.lastIndexOf("/"))))];
  console.log(`見本：${[...sampleTop, ...sampleDirs.map((d) => `${d}/（${samples.filter((f) => f.name.startsWith(`${d}/`)).length} 枚）`)].join("・")}`);
  const others = otherVersions(readme, version);
  if (others.length > 0) {
    console.log(`使う人向けの README に、今の版（${version}）と違う版番号があります。前の版のまま残っていないかを目で確かめてください（止めません）：`);
    for (const o of others) console.log(`  ${o.line} 行目 ${o.version}：${o.text}`);
  }

  // リポジトリのファイルに、この PC のパスが入っていないか（公開リポジトリなので、それ自体が漏れになる）
  const git = spawnSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], { cwd: project, encoding: "utf8", windowsHide: true });
  if (git.status === 0) {
    const files = git.stdout.split("\0").filter((f) => f.length > 0).map((f) => join(project, f)).filter((f) => existsSync(f));
    const bad = files.map((f) => scanFile(f, needles, roots.user)).filter((r) => r.hits.length > 0);
    if (bad.length > 0) {
      console.log(formatReport(bad, shown));
      fail("リポジトリのファイルに、この PC のフォルダの場所が書かれています（上の一覧）");
    }
    console.log(`リポジトリのファイル ${files.length} 個：この PC のパスは見つかりませんでした`);
  } else {
    console.log("git が使えないので、リポジトリのファイルの検査を飛ばします");
  }

  const sevenZip = no7zip ? null : find7zip(process.env);
  if (!no7zip && !sevenZip) fail("7-Zip（7z.exe）が見つかりません。インストーラーの中身を検査できません。入れるか、--no-7zip を付けて限界を承知で進めてください");

  const exe = join(targetDir, "release", "pl-player.exe");
  const setup = join(targetDir, "release", "bundle", "nsis", setupName);
  const nsisDir = join(targetDir, "release", "nsis", "x64");
  const pairs = remapPairs(roots);
  const tauriJs = join(project, "node_modules", "@tauri-apps", "cli", "tauri.js");
  if (!skipBuild) {
    // ビルドに要るものは、前の出力を消す前に全部確かめる（ここで止まれば、前に作った物は残る）
    const nsisHome = join(process.env.LOCALAPPDATA ?? "", "tauri", "NSIS");
    if (!existsSync(join(nsisHome, "makensis.exe")) || !existsSync(join(nsisHome, "Plugins", "x86-unicode", "additional", "nsis_tauri_utils.dll"))) {
      fail("手元に NSIS がありません（%LOCALAPPDATA%\\tauri\\NSIS）。配布用ビルドは取りに行きません。一度ふつうの npx tauri build を走らせて揃えてから、もう一度");
    }
    if (!existsSync(tauriJs)) fail("node_modules に Tauri の CLI がありません。先に npm ci を");
  }

  // ---- 2. ビルド
  if (!skipBuild) {
    step("2/5 ビルド（フォルダの場所を置き換える）");
    const shownPairs = new Set(pairs.map(([f, t]) => `  置き換え（Rust）：${maskText(f, needles)} → ${t}`));
    for (const l of shownPairs) console.log(l);
    const trim = cTrimFlags(roots);
    console.log(`  置き換え（C）：/d1trimfile を ${trim.flags.length} 個（${[...new Set(trim.flags.map((f) => maskText(f.slice("/d1trimfile:".length), needles)))].join("・")} の頭を落とす）`);
    if (trim.skipped.length > 0) console.log(`  空白を含むので C には渡せないフォルダ：${trim.skipped.length} 個（残れば検査で止まる）`);
    const before = sourceState(project);
    // 前の出力を消す（どれもビルドで作り直せる）。既定は置き場所ごと消して、全部を組み直す。
    // ビルドスクリプトの中には、CFLAGS などが変わっても組み直さないもの（opusic-sys は ANDROID_NDK_HOME・OPUS_LIB_* だけを見る）があり、
    // 残りを使うと、置き換えていない C のコードが入ったままになるため
    if (noClean) {
      console.log("  --no-clean：前のビルドの残りを使います（配る物には使わない）");
      for (const d of [join(targetDir, "release", "bundle"), nsisDir, checkDir, buildInfoPath]) rmSync(d, { recursive: true, force: true });
    } else {
      rmSync(targetDir, { recursive: true, force: true });
    }
    const env: NodeJS.ProcessEnv = withCmake({ ...process.env });
    delete env.RUSTFLAGS;
    env.CARGO_ENCODED_RUSTFLAGS = encodedRustflags(process.env, pairs);
    // C と C++：前からある指定は残し、うしろに足す（cc と cmake のクレートが読む）
    for (const k of ["CFLAGS", "CXXFLAGS"]) env[k] = [process.env[k] ?? "", ...trim.flags].filter((s) => s.length > 0).join(" ");
    env.CARGO_NET_OFFLINE = "true";
    env.CARGO_TARGET_DIR = targetDir;
    const started = Date.now();
    const r = run(process.execPath, [tauriJs, "build"], { cwd: project, env });
    if (r.status !== 0) fail(`ビルドが失敗しました（終了コード ${r.status}）`);
    for (const f of [exe, setup]) {
      if (!existsSync(f)) fail(`できるはずの物がありません：${shown(f)}`);
      if (statSync(f).mtimeMs < started - 2000) fail(`古い物が残っています（今回のビルドで作られていない）：${shown(f)}`);
    }
    // ビルドの間にソースが変わっていたら、どちらの中身で組まれたか分からない
    const after = sourceState(project);
    if (before.git !== after.git || before.content !== after.content) {
      fail("ビルドの間に、リポジトリのファイルの中身が変わりました。どちらの中身で組まれたか分からないので、もう一度走らせてください");
    }
    const info: BuildInfo = { version, builtAt: new Date().toISOString(), clean: !noClean, source: after, exeSha256: sha256(exe), setupSha256: sha256(setup) };
    writeFileSync(buildInfoPath, JSON.stringify(info, null, 2) + "\n", "utf8");
    console.log(`  ビルドの記録：${shown(buildInfoPath)}（${describeSource(after)}）`);
  } else {
    step("2/5 ビルドは飛ばします（--skip-build）");
    for (const f of [exe, setup]) if (!existsSync(f)) fail(`前に作った物がありません：${shown(f)}。--skip-build を外して走らせてください`);
    // 前のビルドが今のソースから作られたものかを、ビルドしたときの記録と比べる（ソースを直したあとに、直す前の exe を配らないため）
    if (!existsSync(buildInfoPath)) fail(`ビルドしたときの記録（${shown(buildInfoPath)}）がありません。今のソースから作られたかを確かめられないので、--skip-build を外して走らせてください`);
    let rec: BuildInfo;
    try {
      rec = JSON.parse(readFileSync(buildInfoPath, "utf8")) as BuildInfo;
      if (typeof rec.version !== "string" || typeof rec.exeSha256 !== "string" || !rec.source) throw new Error("形が違う");
    } catch (e) {
      fail(`ビルドしたときの記録が読めません（${(e as Error).message}）。--skip-build を外して走らせてください`);
    }
    const now = { version, source: sourceState(project), exeSha256: sha256(exe), setupSha256: sha256(setup) };
    const problems = buildInfoProblems(rec, now, !!outArg);
    if (problems.length > 0) fail(`前に作った物は、そのままでは組み立てに使えません（違うところ）：\n  - ${problems.join("\n  - ")}\n--skip-build を外して、ビルドからやり直してください`);
    console.log(`前のビルド（${rec.builtAt}、${describeSource(rec.source)}）は、今のソースと中身が同じです`);
    if (rec.source.head !== now.source.head) console.log(`  ビルドしたあとにコミットしています（今：${describeSource(now.source)}）。中身は同じなので通します`);
    if (!rec.clean) console.log("  --no-clean で作った物です（--out を付けた試しなので通します）");
    rmSync(checkDir, { recursive: true, force: true });
  }

  // ---- 3. 検査
  step("3/5 検査（ユーザーのホーム・ユーザー名を含むパス）");
  const scan = (f: string) => scanFile(f, needles, roots.user);
  const reports: FileReport[] = [];
  reports.push(scan(exe));
  reports.push(scan(setup));
  const notes: string[] = [];
  let installerChecked = "取り出していない（--no-7zip）";
  if (sevenZip) {
    const out = join(checkDir, "setup");
    mkdirSync(out, { recursive: true });
    const r = run(sevenZip, ["x", "-y", `-o${out}`, setup], { cwd: project, quiet: true });
    if (r.status !== 0) fail(`7-Zip でインストーラーを取り出せませんでした：\n${r.stdout}`);
    const inner = listFiles(out);
    if (inner.length === 0) fail("インストーラーから何も取り出せませんでした");
    for (const f of inner) reports.push(scan(f));
    const innerExe = inner.find((f) => basename(f).toLowerCase() === "pl-player.exe");
    if (!innerExe) fail("インストーラーの中に pl-player.exe が見つかりません");
    const cmp = sameExceptBundleMarker(readFileSync(exe), readFileSync(innerExe));
    if (!cmp.same) fail(`インストーラーの中の exe が、検査した exe と違います（違うバイト ${cmp.diffBytes}）`);
    installerChecked = `7-Zip で ${inner.length} 個を取り出して検査。中の exe は、インストールせずに使う exe と ${cmp.diffBytes === 0 ? "同じ" : `Tauri が書き込む印（${MARKER_UNK} → ${MARKER_NSIS}、${cmp.diffBytes} バイト）のほかは同じ`}`;
  }
  notes.push(`インストーラーの中身：${installerChecked}`);
  // 画面のビルド結果（exe に埋め込まれる元）
  const distDir = join(project, "dist");
  if (existsSync(distDir)) for (const f of listFiles(distDir)) reports.push(scan(f));
  // NSIS の台本：インストーラーを動かすときの文字にパスが入らないか
  const nsisFiles = existsSync(nsisDir) ? listFiles(nsisDir).filter((f) => /\.(nsi|nsh)$/i.test(f)) : [];
  const nsisBad: string[] = [];
  for (const f of nsisFiles) for (const l of nsisRuntimePathLines(readFileSync(f, "utf8"), needles)) nsisBad.push(`${shown(f)}:${l}`);
  if (nsisFiles.length === 0) notes.push("NSIS の台本が見つからないので、台本の検査は飛ばしました（--skip-build で前の台本が消えているときなど）");
  else notes.push(`NSIS の台本 ${nsisFiles.length} 個：インストーラーを動かすときの文字にパスを使う行 ${nsisBad.length} 行`);

  console.log(formatReport(reports, shown));
  for (const n of notes) console.log(n);
  const badReports = reports.filter(isBad);
  if (nsisBad.length > 0) console.log(nsisBad.map((l) => `  NG ${l}`).join("\n"));
  if (badReports.length > 0 || nsisBad.length > 0) {
    fail(`配る物に、この PC のフォルダの場所が残っています（${badReports.length} ファイル、台本 ${nsisBad.length} 行）。上の一覧を見て塞いでください。${shown(releaseDir)}\\ には何も置いていません`);
  }
  console.log(`検査を通りました（${reports.length} ファイル）`);

  // ---- 4. 組み立て
  step("4/5 配布物の組み立て");
  const outDir = join(releaseDir, assets.base);
  const zipPath = join(releaseDir, assets.zip);
  const sumPath = join(releaseDir, assets.sums);
  const setupAsset = join(releaseDir, assets.setup);
  const portableAsset = join(releaseDir, assets.portable);
  const olds = [outDir, zipPath, sumPath, setupAsset, portableAsset].filter((p) => existsSync(p));
  if (olds.length > 0) {
    // 動かす前に、どれも使われていないかを確かめる（前の版の exe を起動しているなど）。1つでも使われていたら、何も動かさずに止める
    const busy: string[] = [];
    for (const p of olds) {
      for (const f of listFiles(p)) {
        try {
          closeSync(openSync(f, "r+"));
        } catch {
          busy.push(shown(f));
        }
      }
    }
    if (busy.length > 0) {
      fail(`前の配布物が使われています（起動している、開いているなど）：${busy.join("、")}。閉じてから npm run release -- --skip-build で組み立て直せます（ビルドと検査の済んだ物は src-tauri\\target\\dist にあります）。${shown(releaseDir)}\\ は何も変えていません`);
    }
    const keep = join(releaseDir, "控え", timestamp());
    mkdirSync(keep, { recursive: true });
    for (const p of olds) keepAside(p, join(keep, basename(p)));
    console.log(`前の物を ${shown(keep)} へ動かしました（${olds.map((p) => basename(p)).join("、")}）`);
  }
  // zip の中身（日本語の名前のまま。展開した人が見る名前）
  mkdirSync(outDir, { recursive: true });
  for (const [src, n] of [[setup, setupName], [exe, PORTABLE_NAME]] as const) copyFileSync(src, join(outDir, n));
  writeFileSync(join(outDir, "README.md"), readme, "utf8");
  // v3.9.1：見本フォルダ（立ち絵とスキンの見本と使い方）。v3.11.0 からアイコンのフォルダ（アイコン/<くじら|ペンギン>/…。名前の区切りは /）も
  const samplePath = (name: string) => join(outDir, SAMPLE_DIR, ...name.split("/"));
  mkdirSync(join(outDir, SAMPLE_DIR), { recursive: true });
  for (const f of samples) {
    mkdirSync(dirname(samplePath(f.name)), { recursive: true });
    writeFileSync(samplePath(f.name), f.data);
  }
  const entries = [
    ...[setupName, PORTABLE_NAME, "README.md"].map((n) => ({ name: `${assets.base}/${n}`, path: join(outDir, n) })),
    ...samples.map((f) => ({ name: `${assets.base}/${SAMPLE_DIR}/${f.name}`, path: samplePath(f.name) })),
  ];
  writeFileSync(zipPath, makeZip(entries.map((e) => ({ name: e.name, data: readFileSync(e.path), mtime: statSync(e.path).mtime }))));
  // Releases に1つずつ置く物（GitHub が名前を変えない名前で）
  copyFileSync(setup, setupAsset);
  copyFileSync(exe, portableAsset);
  const scannedSetup = sha256(setup);
  const scannedExe = sha256(exe);
  const sums = [
    { name: assets.zip, hash: sha256(zipPath) },
    { name: assets.setup, hash: sha256(setupAsset) },
    { name: assets.portable, hash: sha256(portableAsset) },
  ];
  if (sums[1].hash !== scannedSetup || sums[2].hash !== scannedExe) fail("Releases に置く exe の写しが、検査した物と違います");
  writeFileSync(sumPath, sumsText(sums), "utf8");
  // 一覧を読み直し、同じフォルダの物と照らす（落とした人が sha256sum -c でするのと同じ）
  const checked = checkSums(readFileSync(sumPath, "utf8"), releaseDir);
  if (checked.length !== sums.length || checked.some((c) => c.result !== "ok")) fail(`SHA256 の一覧が、置いた物と合いません：${checked.map((c) => `${c.name} ${c.result}`).join("、")}`);

  // ---- 5. zip の確かめ（別の実装で展開して、中身が同じか）
  step("5/5 zip を Expand-Archive で展開して確かめる");
  const unz = join(checkDir, "zip");
  rmSync(unz, { recursive: true, force: true });
  mkdirSync(unz, { recursive: true });
  const ps = run("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath $env:PL_ZIP -DestinationPath $env:PL_UNZ"], {
    cwd: project,
    // PSModulePath を外して渡す（PowerShell 7 から走らせても 5.1 が自分のモジュールを読むように。windowsPowerShellEnv）
    env: windowsPowerShellEnv(process.env, { PL_ZIP: zipPath, PL_UNZ: unz }),
    quiet: true,
  });
  if (ps.status !== 0) fail(`zip を展開できませんでした：\n${ps.stdout}`);
  for (const e of entries) {
    const got = join(unz, ...e.name.split("/"));
    if (!existsSync(got)) fail(`zip を展開したら ${e.name} がありません（名前が化けている？）`);
    if (sha256(got) !== sha256(e.path)) fail(`zip の中の ${e.name} が、置いた物と違います`);
  }
  const extra = listFiles(unz).length - entries.length;
  if (extra !== 0) fail(`zip の中のファイルの数が違います（${extra > 0 ? "多い" : "少ない"}）`);
  console.log("zip の中身は、置いた物と同じでした（名前・SHA256）。zip の中の exe 2つは、Releases に1つずつ置く exe と同じ中身です");

  console.log(`\nできました（${shown(releaseDir)}\\）`);
  console.log(`GitHub Releases に置く物（名前を変えずに置く）：`);
  for (const s of sums) console.log(`  ${s.hash}  ${s.name}`);
  console.log(`  ${assets.sums}（上の3つの SHA256 の一覧）`);
  console.log(`zip の中身と同じもの：${shown(outDir)}\\`);
}

if (import.meta.main) main();
