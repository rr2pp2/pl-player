// 配布の zip に入れる「見本」フォルダ（v3.9.1。v3.11.0 で見本スキンを2つにし、アイコンを1つずつ入れる用のフォルダを足した）。
// 配布用ビルド scripts/release.mts が使う。
// 中身：
//   - 立ち絵の絵：くじら2枚（アプリに最初から入っている絵と同じファイル。samples\kujira\。Rust は src-tauri\src\skins\sample.rs が埋め込む）と
//     ペンギン2枚（samples\penguin\character\。アプリには埋め込まない）
//   - スキンの見本の zip 2つ（くもくじら・ペンギン）。samples\<フォルダ>\ の中のファイルを全部入れて組む。くもくじらの立ち絵は
//     samples\kujira\ の絵を character\ として足す（くじらの絵を2か所に持たないため）。並び（skin.json を先頭に、残りは名前の順）は
//     Rust のテスト skins::sample::tests::sample_skin_zip と同じで、そちらが今のアプリの取り込みに通す
//   - アイコンを1つずつ入れる用のフォルダ「アイコン\くじら\」「アイコン\ペンギン\」：スキンの zip と同じ絵を、設定の「アイコン」の欄の
//     枠の日本語の名前（src\skins\iconSlots.ts の ICON_SLOTS）で並べたもの。使う人が、どの枠に落とすか迷わないように
//   - 使い方（scripts\release-samples-README.md を型にする）
// 配る絵に、作った PC・ソフト・日時を書ける PNG の塊（tEXt など）があれば止める（書かれていても目では見えないため）
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { ICON_SLOTS, type IconSlot } from "../src/skins/iconSlots.ts";
import { makeZip, type ZipInput } from "./release-zip.mts";

/** zip の中のフォルダの名前（exe と同じ段に置く） */
export const SAMPLE_DIR = "見本";
/** アイコンを1つずつ入れる用のフォルダ（見本フォルダの中） */
export const ICON_DIR = "アイコン";

export interface SampleSkin {
  /** 見本フォルダの中の zip の名前 */
  zip: string;
  /** samples\ の中のフォルダ */
  dir: string;
  id: string;
  name: string;
  /** アイコンのフォルダの名前（アイコン\<これ>\） */
  label: string;
  /** フォルダの外から足すファイル（zip の中の名前, samples\ からの場所） */
  extra: readonly (readonly [string, string])[];
}

/** スキンの見本（並びは見本フォルダの並び） */
export const SAMPLE_SKINS: readonly SampleSkin[] = [
  {
    zip: "くもくじら.zip",
    dir: "kumokujira",
    id: "kumokujira-sample",
    name: "くもくじら（見本）",
    label: "くじら",
    extra: [
      ["character/playing.png", "kujira/playing.png"],
      ["character/stopped.png", "kujira/stopped.png"],
    ],
  },
  { zip: "ペンギン.zip", dir: "penguin", id: "penguin-sample", name: "ペンギン（見本）", label: "ペンギン", extra: [] },
];

/** 見本フォルダの中の名前と、リポジトリの中の元のファイル（samples\ から） */
export const SAMPLE_IMAGES: readonly (readonly [string, string])[] = [
  ["くじら_再生中.png", "kujira/playing.png"],
  ["くじら_停止中.png", "kujira/stopped.png"],
  ["ペンギン_再生中.png", "penguin/character/playing.png"],
  ["ペンギン_停止中.png", "penguin/character/stopped.png"],
];

/** PNG の塊のうち、文字（作った PC のパス・ソフトの名前など）や日時を書けるもの。配る絵には入れない */
export const PNG_METADATA_CHUNKS = new Set(["tEXt", "zTXt", "iTXt", "eXIf", "tIME", "iCCP"]);

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/** PNG の塊の名前を、並んでいる順に返す。PNG でない・途中で切れていれば投げる */
export function pngChunks(buf: Buffer): string[] {
  if (buf.length < 8 || !buf.subarray(0, 8).equals(PNG_SIGNATURE)) throw new Error("PNG ではありません");
  const out: string[] = [];
  let p = 8;
  while (p < buf.length) {
    if (p + 12 > buf.length) throw new Error("PNG が途中で切れています");
    const len = buf.readUInt32BE(p);
    const type = buf.toString("latin1", p + 4, p + 8);
    if (p + 12 + len > buf.length) throw new Error(`PNG の塊 ${type} が途中で切れています`);
    out.push(type);
    p += 12 + len;
    if (type === "IEND") break;
  }
  if (out[out.length - 1] !== "IEND") throw new Error("PNG の終わり（IEND）がありません");
  return out;
}

/** 配る絵に入れない塊（PNG_METADATA_CHUNKS）の名前の一覧。空ならよい */
export function pngMetadataChunks(buf: Buffer): string[] {
  return [...new Set(pngChunks(buf).filter((t) => PNG_METADATA_CHUNKS.has(t)))];
}

const fromSamples = (project: string, rel: string) => join(project, "samples", ...rel.split("/"));

/** フォルダの中のファイルを全部（`/` で区切った、フォルダからの名前） */
function walk(dir: string, prefix = ""): string[] {
  const out: string[] = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const rel = prefix ? `${prefix}/${e.name}` : e.name;
    if (e.isDirectory()) out.push(...walk(join(dir, e.name), rel));
    else if (e.isFile()) out.push(rel);
  }
  return out;
}

/** 名前の比べ方（どれも英数字と . _ - / だけなので、バイトの順と同じ。Rust の文字列の並べ方とそろう） */
const byName = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** スキンの見本の zip に入れるもの（zip の中の名前, samples\ からの場所）。skin.json を先頭に、残りは zip の中の名前の順 */
export function skinFileList(project: string, skin: SampleSkin): [string, string][] {
  const files: [string, string][] = walk(join(project, "samples", skin.dir)).map((rel) => [rel, `${skin.dir}/${rel}`]);
  for (const [name, rel] of skin.extra) {
    if (files.some(([n]) => n === name)) throw new Error(`見本のスキン ${skin.dir} に ${name} が2つあります（samples\\${skin.dir}\\ と足すファイル）`);
    files.push([name, rel]);
  }
  const head = files.filter(([n]) => n === "skin.json");
  if (head.length !== 1) throw new Error(`見本のスキン ${skin.dir} に skin.json がありません`);
  return [...head, ...files.filter(([n]) => n !== "skin.json").sort((a, b) => byName(a[0], b[0]))];
}

/**
 * スキンの見本の zip の項目の日時（どの項目も同じ固定の値）。元のファイルの更新日時を使うと、どの PC で組んだかで zip のバイト列が変わり、
 * 作った日の時間帯も見えるため。zip の日時（MS-DOS の形）が表せるいちばん古い日時 1980-01-01 00:00:00（PC の時刻で数える）
 */
export const SAMPLE_ZIP_TIME = new Date(1980, 0, 1, 0, 0, 0);

/** スキンの見本の zip の中身（日時は SAMPLE_ZIP_TIME） */
export function sampleSkinEntries(project: string, skin: SampleSkin): ZipInput[] {
  return skinFileList(project, skin).map(([name, rel]) => ({ name, data: readFileSync(fromSamples(project, rel)), mtime: SAMPLE_ZIP_TIME }));
}

/** Windows のファイル名に使えない文字（と制御文字）。アイコンのファイル名ではこれを「-」に置き換える */
const BAD_FILE_CHARS = /[\\/:*?"<>|\u0000-\u001f]/g;

/**
 * アイコンのファイルの名前：枠の日本語の名前（状態のある枠は「名前_状態」）＋ .png。例：「再生.png」「シャッフル_切.png」
 * 名前に Windows のファイル名に使えない文字（\ / : * ? " < > | と制御文字）があれば「-」に置き換え、末尾の空白と「.」は外す
 */
export function iconFileName(slot: IconSlot): string {
  const clean = (s: string) => s.replace(BAD_FILE_CHARS, "-").replace(/[ .]+$/, "");
  return `${clean(slot.label)}${slot.stateLabel ? `_${clean(slot.stateLabel)}` : ""}.png`;
}

/** スキンの見本の skin.json から、枠ごとの絵（アイコンのファイルの名前, samples\ からの場所）を引く。無い枠があれば投げる */
export function iconFiles(project: string, skin: SampleSkin): [string, string][] {
  const manifest = JSON.parse(readFileSync(fromSamples(project, `${skin.dir}/skin.json`), "utf8"));
  const icons = manifest.parts?.icons ?? {};
  const app = manifest.parts?.application ?? {};
  const where = new Map(skinFileList(project, skin));
  return ICON_SLOTS.map((slot) => {
    let inZip: unknown;
    if (slot.name === "AppWindow") inZip = app.window;
    else if (slot.name === "AppTray") inZip = typeof app.tray === "string" ? app.tray : app.window;
    else {
      const v = icons[slot.name];
      inZip = typeof v === "string" ? v : v?.[slot.state] ?? v?.default;
    }
    if (typeof inZip !== "string" || !inZip.endsWith(".png")) throw new Error(`見本のスキン ${skin.dir} に、アイコンの枠 ${slot.name}/${slot.state} の PNG がありません`);
    const rel = where.get(inZip);
    if (!rel) throw new Error(`見本のスキン ${skin.dir} の skin.json が指す ${inZip} がありません`);
    return [iconFileName(slot), rel];
  });
}

/** 使い方を型から作る（改行は LF。{{VERSION}} を埋める。埋めていない {{…}} が残れば投げる） */
export function renderSamplesReadme(template: string, version: string): string {
  const out = template.replace(/\r\n?/g, "\n").replace(/\{\{VERSION\}\}/g, version);
  if (/\{\{[A-Z_]+\}\}/.test(out)) throw new Error("見本の使い方の型に、埋めていない {{…}} が残っています");
  return out;
}

/** 見本フォルダに置くもの。名前は見本フォルダの中の名前で、アイコンは `アイコン/<くじら|ペンギン>/<枠の名前>.png`（区切りは /） */
export type SampleFile = { name: string; data: Buffer };

/**
 * 見本フォルダに置くもの。配る PNG（スキンの zip の中・アイコンのフォルダも）に PNG_METADATA_CHUNKS の塊があれば投げる。
 * スキンの zip の中の日時は、どの項目も SAMPLE_ZIP_TIME（固定）。並び：立ち絵の絵 → スキンの zip → アイコンのフォルダ → 使い方
 */
export function sampleFiles(project: string, version: string): SampleFile[] {
  const icons = SAMPLE_SKINS.map((skin) => ({ skin, files: iconFiles(project, skin) }));
  const rels = new Set<string>([
    ...SAMPLE_IMAGES.map(([, rel]) => rel),
    ...SAMPLE_SKINS.flatMap((skin) => skinFileList(project, skin).map(([, rel]) => rel)),
    ...icons.flatMap((x) => x.files.map(([, rel]) => rel)),
  ]);
  const pngs = new Map<string, Buffer>();
  for (const rel of rels) if (rel.endsWith(".png")) pngs.set(rel, readFileSync(fromSamples(project, rel)));
  for (const [rel, data] of pngs) {
    const found = pngMetadataChunks(data);
    if (found.length > 0) throw new Error(`見本の絵 samples/${rel} に、作った PC やソフトを書ける塊（${found.join("・")}）があります。消してから配ってください`);
  }
  const readme = renderSamplesReadme(readFileSync(join(project, "scripts", "release-samples-README.md"), "utf8"), version);
  const out: SampleFile[] = [
    ...SAMPLE_IMAGES.map(([name, rel]) => ({ name, data: pngs.get(rel)! })),
    ...SAMPLE_SKINS.map((skin) => ({ name: skin.zip, data: makeZip(sampleSkinEntries(project, skin)) })),
    ...icons.flatMap(({ skin, files }) => files.map(([name, rel]) => ({ name: `${ICON_DIR}/${skin.label}/${name}`, data: pngs.get(rel)! }))),
    { name: "README.md", data: Buffer.from(readme, "utf8") },
  ];
  const names = out.map((f) => f.name.toLowerCase());
  if (new Set(names).size !== names.length) throw new Error("見本フォルダの中に、同じ名前のファイルがあります");
  return out;
}
