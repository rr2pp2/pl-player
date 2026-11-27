// 配布の zip に入れる「見本」フォルダ（scripts/release-samples.mts。v3.9.1。v3.11.0 で見本スキン2つとアイコンのフォルダ）を確かめる。
// 走らせ方（プロジェクトのフォルダで。依存は要らない。ビルドはしない）：
//   node --test scripts/release-samples.test.mts
// スキンの見本が今のアプリの取り込みに通り、アイコンが全部使われることは、Rust のテスト skins::sample::tests::sample_skin_imports が確かめる（同じ並び・同じファイル）
import { test } from "node:test";
import assert from "node:assert/strict";
import { crc32 } from "node:zlib";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { readZip } from "./release-zip.mts";
import { ICON_DIR, PNG_METADATA_CHUNKS, SAMPLE_IMAGES, SAMPLE_SKINS, iconFileName, iconFiles, pngChunks, pngMetadataChunks, renderSamplesReadme, sampleFiles, skinFileList } from "./release-samples.mts";
import { ICON_SLOTS } from "../src/skins/iconSlots.ts";
import { contrastWarnings } from "../src/skins/contrast.ts";
import { readTheme } from "./theme-contrast.mts";

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

test("見本フォルダの中身：立ち絵の絵4枚・スキンの見本の zip 2つ・アイコンのフォルダ2つ（44 枚ずつ）・使い方", () => {
  const files = sampleFiles(project, "9.9.9");
  const names = files.map((f) => f.name);
  assert.equal(files.length, 4 + 2 + 44 * 2 + 1);
  assert.deepEqual(names.slice(0, 6), ["くじら_再生中.png", "くじら_停止中.png", "ペンギン_再生中.png", "ペンギン_停止中.png", "くもくじら.zip", "ペンギン.zip"]);
  assert.equal(names[names.length - 1], "README.md");
  // くじらの絵は、アプリに埋め込む絵（samples\kujira\）と同じファイル。ペンギンは埋め込まない
  assert.deepEqual(files[0].data, samples("kujira/playing.png"));
  assert.deepEqual(files[1].data, samples("kujira/stopped.png"));
  assert.deepEqual(files[2].data, samples("penguin/character/playing.png"));
  assert.deepEqual(files[3].data, samples("penguin/character/stopped.png"));
  const sampleRs = rust("skins/sample.rs");
  assert.ok(sampleRs.includes('include_bytes!("../../../samples/kujira/playing.png")'));
  assert.ok(sampleRs.includes('include_bytes!("../../../samples/kujira/stopped.png")'));
  assert.ok(!sampleRs.includes("penguin"));
  // くじらの立ち絵は samples\kujira\ の1か所だけ（くもくじらのフォルダに写しを置かない）
  assert.ok(!existsSync(join(project, "samples", "kumokujira", "character")));
  // アイコンのフォルダ：くじら・ペンギンそれぞれに、設定の「アイコン」の欄の枠と同じ 44 枚（v3.12.0 で「シャッフル再生.png」を足した）
  for (const skin of SAMPLE_SKINS) {
    const inDir = names.filter((n) => n.startsWith(`${ICON_DIR}/${skin.label}/`)).map((n) => n.split("/")[2]);
    assert.deepEqual(inDir, ICON_SLOTS.map(iconFileName), skin.label);
  }
  // 使い方は型の {{VERSION}} を埋めたもの・改行は LF。ファイルとフォルダの名前を書いてある
  const readme = files[files.length - 1].data.toString("utf8");
  assert.ok(readme.startsWith("# 見本（PL Player 9.9.9）\n"));
  assert.ok(!readme.includes("\r") && !readme.includes("{{"));
  for (const [name] of SAMPLE_IMAGES) assert.ok(readme.includes(`\`${name}\``), name);
  for (const skin of SAMPLE_SKINS) {
    assert.ok(readme.includes(`\`${skin.zip}\``), skin.zip);
    assert.ok(readme.includes(`\`${ICON_DIR}\\${skin.label}\``), skin.label);
  }
  assert.ok(readme.includes("「アイコン」の欄") && readme.includes("ドラッグ＆ドロップ"));
  assert.ok(readme.includes("スキンを替えても"));
});

test("アイコンのファイルの名前：枠の日本語の名前（状態のある枠は「名前_状態」）。使えない文字は「-」に置き換え、名前は重ならない", () => {
  const names = ICON_SLOTS.map(iconFileName);
  assert.equal(new Set(names).size, names.length);
  for (const n of ["再生.png", "シャッフル_切.png", "シャッフル_入.png", "シャッフル再生.png", "リピート_1曲.png", "音量_0.png", "お気に入り_オン.png", "ウィンドウ・タスクバー.png", "タスクトレイ.png"]) assert.ok(names.includes(n), n);
  for (const n of names) assert.ok(!/[\\/:*?"<>|\u0000-\u001f]/.test(n) && !/[ .]\.png$/.test(n), n);
  assert.equal(iconFileName({ name: "X", state: "a", group: "other", label: 'あ/い:う*え?"お<か>き|く', stateLabel: "け\\こ.", px: 16 }), "あ-い-う-え--お-か-き-く_け-こ.png");
  // 枠ごとの絵は、スキンの zip の中の同じ絵（skin.json の icons・application が指すもの）
  for (const skin of SAMPLE_SKINS) {
    const manifest = JSON.parse(samples(`${skin.dir}/skin.json`).toString("utf8"));
    const files = new Map(iconFiles(project, skin));
    for (const slot of ICON_SLOTS) {
      const rel = files.get(iconFileName(slot))!;
      let want: string;
      if (slot.name === "AppWindow") want = manifest.parts.application.window;
      else if (slot.name === "AppTray") want = manifest.parts.application.tray;
      else {
        const v = manifest.parts.icons[slot.name];
        want = typeof v === "string" ? v : v[slot.state];
      }
      assert.equal(rel, `${skin.dir}/${want}`, `${skin.dir} ${slot.name}/${slot.state}`);
    }
  }
});

test("スキンの見本の zip：いちばん上に skin.json・残りは名前の順・skin.json が使うファイルだけ・中身は元のファイルのまま", () => {
  const files = sampleFiles(project, "9.9.9");
  const t = rust("skins/sample_tests.rs");
  for (const skin of SAMPLE_SKINS) {
    const back = readZip(files.find((f) => f.name === skin.zip)!.data);
    const list = skinFileList(project, skin);
    assert.deepEqual(back.map((e) => e.name), list.map(([n]) => n), skin.zip);
    assert.equal(back[0].name, "skin.json");
    const rest = back.slice(1).map((e) => e.name);
    assert.deepEqual([...rest].sort(), rest);
    for (const [i, [, rel]] of list.entries()) assert.deepEqual(back[i].data, samples(rel), rel);
    // 全部入り：skin.json・アイコン 42 枚（35 個の状態ごと。v3.12.0 で icons/IconShufflePlay.png を足した）・ウィンドウとトレイ・立ち絵2枚
    assert.equal(back.length, 47, skin.zip);
    assert.equal(rest.filter((n) => n.startsWith("icons/")).length, 42);
    assert.ok(rest.includes("icons/IconShufflePlay.png"), skin.zip);
    assert.deepEqual(rest.filter((n) => !n.startsWith("icons/")), ["app/tray.png", "app/window.png", "character/playing.png", "character/stopped.png"]);
    // skin.json が使うファイルと、zip の中のファイルが同じ（使っていない画像は取り込みで断られるため）
    const skinJson = JSON.parse(back[0].data.toString("utf8"));
    const used = new Set<string>();
    const collect = (v: unknown) => {
      if (typeof v === "string" && v.endsWith(".png")) used.add(v);
      else if (v && typeof v === "object") for (const x of Object.values(v)) collect(x);
    };
    collect(skinJson.parts);
    assert.deepEqual([...used].sort(), [...rest].sort(), skin.zip);
    // 配布の見本としての名前・id・説明。35 個のアイコン（IconShufflePlay を書くので minAppVersion は 3.12.0）
    assert.equal(skinJson.id, skin.id);
    assert.equal(skinJson.name, skin.name);
    assert.ok(skinJson.description.startsWith("スキン作りの見本。"), skinJson.description);
    assert.equal(Object.keys(skinJson.parts.icons).length, 35);
    assert.equal(skinJson.parts.icons.IconShufflePlay, "icons/IconShufflePlay.png");
    assert.equal(skinJson.minAppVersion, "3.12.0");
    assert.ok(skinJson.description.includes("ボタンのアイコン全部（35 種類）"), skinJson.description);
    assert.deepEqual(skinJson.parts.application, { window: "app/window.png", tray: "app/tray.png" });
    assert.deepEqual(skinJson.parts.character.states, { playing: "character/playing.png", stopped: "character/stopped.png" });
    // Rust のテスト（取り込みに通す側）が同じ決まり（フォルダ・id・名前・足すファイル）で組む
    assert.ok(t.includes(`("${skin.dir}", "${skin.id}", "${skin.name}", &[`), skin.dir);
    for (const [n, r] of skin.extra) assert.ok(t.includes(`("${n}", "${r}")`), `${n} ${r}`);
  }
  assert.ok(t.includes('files.sort_by(|a, b| (a.0 != "skin.json", &a.0).cmp(&(b.0 != "skin.json", &b.0)));'));
});

test("見本スキンの色は、両方のテーマで読みやすさの警告が出ない（取り込みの確認画面と同じ計算）", () => {
  for (const skin of SAMPLE_SKINS) {
    const skinJson = JSON.parse(samples(`${skin.dir}/skin.json`).toString("utf8"));
    for (const theme of ["dark", "light"] as const) {
      const tokens = skinJson.parts.appearance.themes[theme]?.tokens;
      assert.ok(tokens && Object.keys(tokens).length > 0, `${skin.dir} ${theme}`);
      const base = readTheme(join(project, "src", "theme", `${theme}.css`));
      assert.deepEqual(contrastWarnings(base, tokens).map((r) => `${r.scene} ${r.name} ${r.ratio.toFixed(2)}`), [], `${skin.dir} ${theme}`);
    }
  }
});

test("配る絵に、作った PC・ソフト・日時を書ける塊が無い（立ち絵・スキンの zip の中・アイコンのフォルダの全部）", () => {
  const rels = new Set<string>([
    ...SAMPLE_IMAGES.map(([, r]) => r),
    ...SAMPLE_SKINS.flatMap((skin) => skinFileList(project, skin).map(([, r]) => r)),
    ...SAMPLE_SKINS.flatMap((skin) => iconFiles(project, skin).map(([, r]) => r)),
  ]);
  const pngs = [...rels].filter((r) => r.endsWith(".png"));
  // くじら2枚・くもくじらのフォルダ 44 枚（アイコン 42・アプリ 2。立ち絵はくじらの2枚）・ペンギンのフォルダ 46 枚
  assert.equal(pngs.length, 2 + 44 + 46);
  for (const rel of pngs) {
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

/** zip の中央ディレクトリの項目ごとの日時（MS-DOS の形の時刻と日付）を読む */
function zipTimes(zip: Buffer): { time: number; date: number }[] {
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  const count = zip.readUInt16LE(eocd + 10);
  let p = zip.readUInt32LE(eocd + 16);
  const out: { time: number; date: number }[] = [];
  for (let i = 0; i < count; i++) {
    assert.equal(zip.readUInt32LE(p), 0x02014b50);
    out.push({ time: zip.readUInt16LE(p + 12), date: zip.readUInt16LE(p + 14) });
    const local = zip.readUInt32LE(p + 42);
    // ローカルヘッダの日時も同じ
    assert.deepEqual({ time: zip.readUInt16LE(local + 10), date: zip.readUInt16LE(local + 12) }, out[out.length - 1]);
    p += 46 + zip.readUInt16LE(p + 28) + zip.readUInt16LE(p + 30) + zip.readUInt16LE(p + 32);
  }
  return out;
}

test("スキンの見本の zip の項目の日時は、どれも固定の 1980-01-01 00:00（元のファイルの更新日時を出さない。どの PC で組んでも同じバイト列）", () => {
  const a = sampleFiles(project, "9.9.9");
  const b = sampleFiles(project, "9.9.9");
  for (const skin of SAMPLE_SKINS) {
    const zip = a.find((f) => f.name === skin.zip)!.data;
    const times = zipTimes(zip);
    assert.equal(times.length, 47); // skin.json・アイコン 42 枚・ウィンドウとトレイ・立ち絵2枚
    // 時刻 0（00:00:00）・日付 0x21（1980 年 1 月 1 日。MS-DOS の形が表せるいちばん古い日）。Rust のテストの zip（testkit）も同じ値
    assert.deepEqual([...new Set(times.map((t) => `${t.date}/${t.time}`))], ["33/0"], skin.zip);
    assert.deepEqual(b.find((f) => f.name === skin.zip)!.data, zip, `${skin.zip} is reproducible`);
  }
  assert.ok(rust("skins/testkit.rs").includes("out.extend_from_slice(&0u16.to_le_bytes());\n        out.extend_from_slice(&0x21u16.to_le_bytes());"));
});
