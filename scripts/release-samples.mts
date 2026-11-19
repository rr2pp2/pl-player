// 配布の zip に入れる「見本」フォルダ（v3.9.1）。配布用ビルド scripts/release.mts が使う。
// 中身：
//   - 立ち絵の見本「くじら」の絵2枚（アプリに最初から入っている絵と同じファイル。samples\kujira\。Rust は src-tauri\src\skins\sample.rs が埋め込む）
//   - スキンの見本「くもくじら」の zip（samples\kumokujira\ の skin.json とウィンドウのアイコンに、くじらの絵を character\ として足して組む。
//     絵を2か所に持たないため。並びは Rust のテスト skins::sample::tests::sample_skin_zip と同じで、そちらが今のアプリの取り込みに通す）
//   - 使い方（scripts\release-samples-README.md を型にする）
// 配る絵に、作った PC・ソフト・日時を書ける PNG の塊（tEXt など）があれば止める（書かれていても目では見えないため）
import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { makeZip, type ZipInput } from "./release-zip.mts";

/** zip の中のフォルダの名前（exe と同じ段に置く） */
export const SAMPLE_DIR = "見本";
export const SAMPLE_SKIN_ZIP = "くもくじら.zip";

/** 見本フォルダの中の名前と、リポジトリの中の元のファイル（samples\ から） */
export const SAMPLE_IMAGES: readonly (readonly [string, string])[] = [
  ["くじら_再生中.png", "kujira/playing.png"],
  ["くじら_停止中.png", "kujira/stopped.png"],
];

/** スキンの見本の zip の中の名前と、元のファイル（samples\ から）。いちばん上に skin.json */
export const SAMPLE_SKIN_FILES: readonly (readonly [string, string])[] = [
  ["skin.json", "kumokujira/skin.json"],
  ["app/window.png", "kumokujira/app/window.png"],
  ["character/playing.png", "kujira/playing.png"],
  ["character/stopped.png", "kujira/stopped.png"],
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

/** スキンの見本の zip の中身 */
export function sampleSkinEntries(project: string): ZipInput[] {
  return SAMPLE_SKIN_FILES.map(([name, rel]) => {
    const p = fromSamples(project, rel);
    return { name, data: readFileSync(p), mtime: statSync(p).mtime };
  });
}

/** 使い方を型から作る（改行は LF。{{VERSION}} を埋める。埋めていない {{…}} が残れば投げる） */
export function renderSamplesReadme(template: string, version: string): string {
  const out = template.replace(/\r\n?/g, "\n").replace(/\{\{VERSION\}\}/g, version);
  if (/\{\{[A-Z_]+\}\}/.test(out)) throw new Error("見本の使い方の型に、埋めていない {{…}} が残っています");
  return out;
}

export type SampleFile = { name: string; data: Buffer };

/**
 * 見本フォルダに置くもの（名前は見本フォルダの中の名前）。配る PNG（スキンの zip の中も）に PNG_METADATA_CHUNKS の塊があれば投げる。
 * スキンの zip の中の日時は、元のファイルの日時
 */
export function sampleFiles(project: string, version: string): SampleFile[] {
  const pngs = new Map<string, Buffer>();
  for (const [, rel] of [...SAMPLE_IMAGES, ...SAMPLE_SKIN_FILES]) {
    if (rel.endsWith(".png")) pngs.set(rel, readFileSync(fromSamples(project, rel)));
  }
  for (const [rel, data] of pngs) {
    const found = pngMetadataChunks(data);
    if (found.length > 0) throw new Error(`見本の絵 samples/${rel} に、作った PC やソフトを書ける塊（${found.join("・")}）があります。消してから配ってください`);
  }
  const readme = renderSamplesReadme(readFileSync(join(project, "scripts", "release-samples-README.md"), "utf8"), version);
  return [
    ...SAMPLE_IMAGES.map(([name, rel]) => ({ name, data: pngs.get(rel)! })),
    { name: SAMPLE_SKIN_ZIP, data: makeZip(sampleSkinEntries(project)) },
    { name: "README.md", data: Buffer.from(readme, "utf8") },
  ];
}
