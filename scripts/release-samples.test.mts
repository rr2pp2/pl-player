// 配布の zip に入れる「見本」フォルダ（scripts/release-samples.mts。v3.9.1）を確かめる。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。ビルドはしない）：
//   node --test scripts/release-samples.test.mts
// スキンの見本が今のアプリの取り込みに通ることは、Rust のテスト skins::sample::tests::sample_skin_imports が確かめる（同じ並び・同じファイル）
import { test } from "node:test";
import assert from "node:assert/strict";
import { crc32 } from "node:zlib";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { readZip } from "./release-zip.mts";
import { PNG_METADATA_CHUNKS, SAMPLE_IMAGES, SAMPLE_SKIN_FILES, SAMPLE_SKIN_ZIP, pngChunks, pngMetadataChunks, renderSamplesReadme, sampleFiles } from "./release-samples.mts";

const project = fileURLToPath(new URL("..", import.meta.url));
const samples = (rel: string) => readFileSync(join(project, "samples", ...rel.split("/")));
const rust = (rel: string) => readFileSync(join(project, "src-tauri", "src", ...rel.split("/")), "utf8");

/** PNG の塊を1つ作る */
function chunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body) >>> 0);
  return Buffer.concat([len, body, crc]);
}

test("見本フォルダの中身：くじらの絵2枚・スキンの見本の zip・使い方", () => {
  const files = sampleFiles(project, "9.9.9");
  assert.deepEqual(files.map((f) => f.name), ["くじら_再生中.png", "くじら_停止中.png", SAMPLE_SKIN_ZIP, "README.md"]);
  // くじらの絵は、アプリに埋め込む絵（samples\kujira\）と同じファイル
  assert.deepEqual(files[0].data, samples("kujira/playing.png"));
  assert.deepEqual(files[1].data, samples("kujira/stopped.png"));
  assert.ok(rust("skins/sample.rs").includes('include_bytes!("../../../samples/kujira/playing.png")'));
  assert.ok(rust("skins/sample.rs").includes('include_bytes!("../../../samples/kujira/stopped.png")'));
  // 使い方は型の {{VERSION}} を埋めたもの・改行は LF
  const readme = files[3].data.toString("utf8");
  assert.ok(readme.startsWith("# 見本（PL Player 9.9.9）\n"));
  assert.ok(!readme.includes("\r") && !readme.includes("{{"));
  for (const [name] of SAMPLE_IMAGES) assert.ok(readme.includes(`\`${name}\``), name);
  assert.ok(readme.includes(`\`${SAMPLE_SKIN_ZIP}\``));
});

test("スキンの見本の zip：いちばん上に skin.json・くじらの絵を character に・中身は元のファイルのまま", () => {
  const zip = sampleFiles(project, "9.9.9").find((f) => f.name === SAMPLE_SKIN_ZIP)!.data;
  const back = readZip(zip);
  assert.deepEqual(back.map((e) => e.name), ["skin.json", "app/window.png", "character/playing.png", "character/stopped.png"]);
  for (const [i, [, rel]] of SAMPLE_SKIN_FILES.entries()) assert.deepEqual(back[i].data, samples(rel), rel);
  // Rust のテスト（取り込みに通す側）と同じ並び・同じファイル
  const t = rust("skins/sample_tests.rs");
  const order = SAMPLE_SKIN_FILES.map(([name, rel]) => t.indexOf(`Z::file("${name}", ${rel.startsWith("kujira/") ? `KUJIRA_${rel.includes("playing") ? "PLAYING" : "STOPPED"}` : `&read("${rel}")`})`));
  assert.ok(order.every((at) => at >= 0), JSON.stringify(order));
  assert.deepEqual([...order].sort((a, b) => a - b), order);
  // skin.json：配布の見本としての名前・id・説明
  const skin = JSON.parse(back[0].data.toString("utf8"));
  assert.equal(skin.id, "kumokujira-sample");
  assert.equal(skin.name, "くもくじら（見本）");
  assert.ok(skin.description.startsWith("スキン作りの見本。"), skin.description);
  assert.deepEqual(skin.parts.character.states, { playing: "character/playing.png", stopped: "character/stopped.png" });
  assert.equal(skin.parts.application.window, "app/window.png");
});

test("配る絵に、作った PC・ソフト・日時を書ける塊が無い", () => {
  for (const [, rel] of [...SAMPLE_IMAGES, ...SAMPLE_SKIN_FILES].filter(([, r]) => r.endsWith(".png"))) {
    const chunks = pngChunks(samples(rel));
    assert.deepEqual(pngMetadataChunks(samples(rel)), [], rel);
    assert.equal(chunks[0], "IHDR", rel);
    assert.equal(chunks[chunks.length - 1], "IEND", rel);
  }
  // 見つける側：tEXt を足した PNG は見つかる・壊れた PNG は投げる
  const png = samples("kumokujira/app/window.png");
  const iend = png.length - 12;
  for (const type of PNG_METADATA_CHUNKS) {
    const tagged = Buffer.concat([png.subarray(0, iend), chunk(type, Buffer.from("Software\0example", "latin1")), png.subarray(iend)]);
    assert.deepEqual(pngMetadataChunks(tagged), [type]);
  }
  assert.throws(() => pngChunks(Buffer.from("not a png")));
  assert.throws(() => pngChunks(png.subarray(0, iend)));
  assert.throws(() => pngChunks(png.subarray(0, png.length - 2)));
});

test("使い方の決まりの値が、アプリ（Rust）の上限と同じ", () => {
  const readme = renderSamplesReadme(readFileSync(join(project, "scripts", "release-samples-README.md"), "utf8"), "9.9.9");
  const num = (src: string, re: RegExp) => {
    const m = re.exec(src);
    assert.ok(m, String(re));
    return Number(m[1].replace(/_/g, ""));
  };
  const images = rust("skins/images.rs");
  const mib = num(images, /pub const MAX_IMAGE_BYTES: u64 = (\d+) \* 1024 \* 1024;/);
  const side = num(images, /pub const MAX_SIDE: u32 = (\d+);/);
  const frames = num(images, /pub const MAX_FRAMES: u32 = (\d+);/);
  const durationMs = num(images, /pub const MAX_DURATION_MS: u64 = ([\d_]+);/);
  const maxAvatars = num(rust("skins/avatars.rs"), /pub const MAX_AVATARS: usize = (\d+);/);
  assert.ok(readme.includes(`1枚 ${mib}MB まで・縦横 ${side}px まで`));
  assert.ok(readme.includes(`${frames} コマ・${durationMs / 1000} 秒まで`));
  assert.ok(readme.includes(`${maxAvatars} 体まで`));
  // 形式は立ち絵を追加するときに選べるもの（Rust のファイルを選ぶ画面の絞り込み）と同じ
  assert.ok(rust("commands/avatars.rs").includes('add_filter("画像（PNG・WebP・GIF）", &["png", "webp", "gif"])'));
  assert.ok(readme.includes("| 形式 | PNG・WebP・GIF。"));
  // 右の欄の幅（画面の character.ts の下限・上限）
  const character = readFileSync(join(project, "src", "skins", "character.ts"), "utf8");
  const railMin = num(character, /export const RAIL_MIN_PX = (\d+);/);
  const railMax = num(character, /export const RAIL_MAX_PX = (\d+);/);
  assert.ok(readme.includes(`${railMin}〜${railMax}px`));
  // くじらの大きさ（PNG の頭の縦横）
  const head = samples("kujira/playing.png");
  assert.ok(readme.includes(`${head.readUInt32BE(16)}×${head.readUInt32BE(20)}px`));
  assert.throws(() => renderSamplesReadme("{{OTHER}}", "1.0.0"));
});
