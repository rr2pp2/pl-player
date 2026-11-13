// zip を作る（Node の標準機能だけ。配布用ビルド scripts/release.mts が使う）。
// 日本語のファイル名が化けないように、名前は UTF-8 で書き、UTF-8 の印（汎用フラグの bit 11）を付ける。
// フォルダの区切りは / （zip の決まり。PowerShell 5.1 の Compress-Archive は \ で書くことがあり、Windows 以外で展開すると崩れる）。
// 4GB を超えるもの（ZIP64）は扱わない。
import { crc32, deflateRawSync, inflateRawSync } from "node:zlib";

export type ZipInput = { name: string; data: Buffer; mtime: Date };

/** 日時を MS-DOS の形（2秒きざみ。1980〜2107年） */
function dosTime(d: Date): { time: number; date: number } {
  const y = Math.min(Math.max(d.getFullYear(), 1980), 2107);
  return {
    time: (d.getHours() << 11) | (d.getMinutes() << 5) | Math.floor(d.getSeconds() / 2),
    date: ((y - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate(),
  };
}

export function makeZip(items: ZipInput[]): Buffer {
  const locals: Buffer[] = [];
  const centrals: Buffer[] = [];
  let offset = 0;
  for (const it of items) {
    if (it.name.includes("\\") || it.name.startsWith("/") || it.name.split("/").includes("..")) throw new Error(`zip の中の名前が正しくありません：${it.name}`);
    const name = Buffer.from(it.name, "utf8");
    const crc = crc32(it.data) >>> 0;
    const packed = deflateRawSync(it.data, { level: 9 });
    const stored = packed.length >= it.data.length; // 縮まないものはそのまま入れる
    const body = stored ? it.data : packed;
    if (body.length >= 0xffffffff || it.data.length >= 0xffffffff || offset >= 0xffffffff) throw new Error("4GB を超えるものは入れられません");
    const { time, date } = dosTime(it.mtime);
    const flags = 0x0800; // bit 11：名前は UTF-8
    const method = stored ? 0 : 8;

    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4); // 展開に要る版 2.0
    local.writeUInt16LE(flags, 6);
    local.writeUInt16LE(method, 8);
    local.writeUInt16LE(time, 10);
    local.writeUInt16LE(date, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(body.length, 18);
    local.writeUInt32LE(it.data.length, 22);
    local.writeUInt16LE(name.length, 26);
    local.writeUInt16LE(0, 28);
    locals.push(local, name, body);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4); // 作った側：MS-DOS（0）・2.0
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(flags, 8);
    central.writeUInt16LE(method, 10);
    central.writeUInt16LE(time, 12);
    central.writeUInt16LE(date, 14);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(body.length, 20);
    central.writeUInt32LE(it.data.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt16LE(0, 30); // 拡張欄
    central.writeUInt16LE(0, 32); // コメント
    central.writeUInt16LE(0, 34); // ディスク番号
    central.writeUInt16LE(0, 36); // 内部属性
    central.writeUInt32LE(0x20, 38); // 外部属性：アーカイブ
    central.writeUInt32LE(offset, 42);
    centrals.push(central, name);

    offset += local.length + name.length + body.length;
  }
  const cdSize = centrals.reduce((n, b) => n + b.length, 0);
  if (items.length > 0xffff) throw new Error("入れるものが多すぎます");
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(0, 4);
  end.writeUInt16LE(0, 6);
  end.writeUInt16LE(items.length, 8);
  end.writeUInt16LE(items.length, 10);
  end.writeUInt32LE(cdSize, 12);
  end.writeUInt32LE(offset, 16);
  end.writeUInt16LE(0, 20);
  return Buffer.concat([...locals, ...centrals, end]);
}

/** 読み直し（テスト用）。中央ディレクトリから名前・UTF-8 の印・中身を取り、CRC を確かめる */
export function readZip(zip: Buffer): { name: string; utf8Flag: boolean; data: Buffer }[] {
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) throw new Error("zip の終わりの印がありません");
  const count = zip.readUInt16LE(eocd + 10);
  let p = zip.readUInt32LE(eocd + 16);
  const out: { name: string; utf8Flag: boolean; data: Buffer }[] = [];
  for (let i = 0; i < count; i++) {
    if (zip.readUInt32LE(p) !== 0x02014b50) throw new Error("中央ディレクトリが壊れています");
    const flags = zip.readUInt16LE(p + 8);
    const method = zip.readUInt16LE(p + 10);
    const crc = zip.readUInt32LE(p + 16);
    const csize = zip.readUInt32LE(p + 20);
    const nlen = zip.readUInt16LE(p + 28);
    const elen = zip.readUInt16LE(p + 30);
    const clen = zip.readUInt16LE(p + 32);
    const lo = zip.readUInt32LE(p + 42);
    const name = zip.toString("utf8", p + 46, p + 46 + nlen);
    const lnlen = zip.readUInt16LE(lo + 26);
    const lelen = zip.readUInt16LE(lo + 28);
    const start = lo + 30 + lnlen + lelen;
    const body = zip.subarray(start, start + csize);
    const data = method === 8 ? inflateRawSync(body) : Buffer.from(body);
    if ((crc32(data) >>> 0) !== crc) throw new Error(`CRC が合いません：${name}`);
    out.push({ name, utf8Flag: (flags & 0x0800) !== 0, data });
    p += 46 + nlen + elen + clen;
  }
  return out;
}
