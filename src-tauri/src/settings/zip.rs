//! 設定の書き出し用の、最小限の ZIP（無圧縮・stored のみ）。
//!
//! 承認済みの依存に ZIP の部品が無いため、書き出し・読み込みに要る範囲だけを自前で持つ。
//! 読めるのは、このアプリが書いた形（無圧縮）だけ。それ以外は「非対応形式」として断る。

use std::io::{Read, Write};

fn crc_table() -> &'static [u32; 256] {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, slot) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *slot = c;
        }
        t
    })
}

pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c = t[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn write<W: Write>(out: &mut W, entries: &[Entry]) -> std::io::Result<()> {
    let mut offset: u32 = 0;
    let mut central = Vec::new();
    for e in entries {
        let name = e.name.as_bytes();
        let crc = crc32(&e.data);
        let size = e.data.len() as u32;
        let mut local = Vec::new();
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        local.extend_from_slice(&20u16.to_le_bytes()); // version needed
        local.extend_from_slice(&0x0800u16.to_le_bytes()); // flags: UTF-8 名
        local.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        local.extend_from_slice(&0u16.to_le_bytes()); // time
        local.extend_from_slice(&0x21u16.to_le_bytes()); // date (1980-01-01)
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name);
        out.write_all(&local)?;
        out.write_all(&e.data)?;

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // made by
        central.extend_from_slice(&20u16.to_le_bytes()); // needed
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0x21u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attr
        central.extend_from_slice(&0u32.to_le_bytes()); // external attr
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
        offset += local.len() as u32 + size;
    }
    out.write_all(&central)?;
    let mut eocd = Vec::new();
    eocd.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    eocd.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    eocd.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    eocd.extend_from_slice(&(central.len() as u32).to_le_bytes());
    eocd.extend_from_slice(&offset.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes());
    out.write_all(&eocd)
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// 設定 zip の大きさの上限（v3.1.1。ペネトレーションテスト v3.0.2 の1）。
/// 中身は JSON と背景画像1枚（50MB まで）なので、これで足りる。展開後の合計もこれを超えない
pub const MAX_ZIP_BYTES: u64 = 96 * 1024 * 1024;

/// 中央ディレクトリに並べてよい項目の数（このアプリが書くのは6つ）
const MAX_ENTRIES: usize = 4096;

/// 読み込み。`want` が true を返す名前だけを取り出す（ほかは中身を写さない）。
///
/// v3.1.1 の直し（ペネトレーションテスト v3.0.2 の1「重なりエントリ」）：
/// - ファイルは上限＋1 バイトまでしか読まない
/// - 各項目のデータの範囲（ローカルヘッダの頭からデータの終わりまで）が、ほかの項目や中央ディレクトリと重なれば断る
///   （同じデータを何千回も指す zip で、展開のメモリが何十倍にもなっていた）
/// - 取り出した中身の合計も上限を超えない。同じ名前が2つあれば断る
pub fn read<R: Read>(input: &mut R, want: &dyn Fn(&str) -> bool) -> Result<Vec<Entry>, String> {
    read_limited(input, want, MAX_ZIP_BYTES)
}

fn read_limited<R: Read>(input: &mut R, want: &dyn Fn(&str) -> bool, max: u64) -> Result<Vec<Entry>, String> {
    let mut b = Vec::new();
    input.take(max + 1).read_to_end(&mut b).map_err(|e| e.to_string())?;
    if b.len() as u64 > max {
        return Err(format!("zip: too large (over {max} bytes)"));
    }
    if b.len() < 22 {
        return Err("not a zip".into());
    }
    // 末尾から EOCD を探す
    let mut eocd = None;
    let start = b.len().saturating_sub(22 + 65_535);
    for i in (start..=b.len() - 22).rev() {
        if u32_at(&b, i) == Some(0x0605_4b50) {
            eocd = Some(i);
            break;
        }
    }
    let eocd = eocd.ok_or("zip: end record not found")?;
    let count = u16_at(&b, eocd + 10).ok_or("zip: bad eocd")? as usize;
    let cd_size = u32_at(&b, eocd + 12).ok_or("zip: bad eocd")? as usize;
    let cd_off = u32_at(&b, eocd + 16).ok_or("zip: bad eocd")? as usize;
    if count > MAX_ENTRIES {
        return Err(format!("zip: too many entries ({count})"));
    }
    let cd_end = cd_off.checked_add(cd_size).ok_or("zip: bad central directory")?;
    if cd_end > eocd {
        return Err("zip: central directory overlaps the end record".into());
    }
    // 使った範囲 [始め, 終わり)。中央ディレクトリと終わりの記録も入れておく
    let mut ranges: Vec<(usize, usize, String)> = vec![(cd_off, b.len(), "central directory".into())];
    let mut p = cd_off;
    let mut picked: Vec<(String, u32, usize, usize)> = Vec::new();
    let mut total: u64 = 0;
    for _ in 0..count {
        if p + 46 > cd_end || u32_at(&b, p) != Some(0x0201_4b50) {
            return Err("zip: bad central directory".into());
        }
        let method = u16_at(&b, p + 10).ok_or("zip: short")?;
        let crc = u32_at(&b, p + 16).ok_or("zip: short")?;
        let csize = u32_at(&b, p + 20).ok_or("zip: short")? as usize;
        let usize_ = u32_at(&b, p + 24).ok_or("zip: short")? as usize;
        let name_len = u16_at(&b, p + 28).ok_or("zip: short")? as usize;
        let extra_len = u16_at(&b, p + 30).ok_or("zip: short")? as usize;
        let comment_len = u16_at(&b, p + 32).ok_or("zip: short")? as usize;
        let local = u32_at(&b, p + 42).ok_or("zip: short")? as usize;
        if p + 46 + name_len > cd_end {
            return Err("zip: bad central directory".into());
        }
        let name = String::from_utf8_lossy(b.get(p + 46..p + 46 + name_len).ok_or("zip: short")?).into_owned();
        p += 46 + name_len + extra_len + comment_len;
        if p > cd_end {
            return Err("zip: bad central directory".into());
        }
        if method != 0 {
            return Err(format!("zip: compressed entry not supported: {name}"));
        }
        if csize != usize_ {
            return Err(format!("zip: size mismatch: {name}"));
        }
        if u32_at(&b, local) != Some(0x0403_4b50) {
            return Err("zip: bad local header".into());
        }
        // ローカルヘッダの大きさも中央ディレクトリと同じでなければ断る（このアプリは両方に書く）
        let local_size = u32_at(&b, local + 18).ok_or("zip: short")? as usize;
        if local_size != csize || u16_at(&b, local + 8) != Some(0) {
            return Err(format!("zip: local header mismatch: {name}"));
        }
        let ln = u16_at(&b, local + 26).ok_or("zip: short")? as usize;
        let le = u16_at(&b, local + 28).ok_or("zip: short")? as usize;
        let data_start = local + 30 + ln + le;
        let data_end = data_start.checked_add(csize).ok_or("zip: truncated")?;
        if data_end > cd_off {
            return Err(format!("zip: entry overlaps the central directory: {name}"));
        }
        ranges.push((local, data_end, name.clone()));
        total += csize as u64;
        if total > max {
            return Err("zip: entries too large in total".into());
        }
        if b.get(data_start..data_end).is_none() {
            return Err(format!("zip: truncated: {name}"));
        }
        // 危ない名前（親フォルダへ出る・絶対パス）と、要らない名前は中身を写さない
        if name.contains("..") || name.starts_with('/') || name.contains(':') || !want(&name) {
            continue;
        }
        if picked.iter().any(|(n, ..)| *n == name) {
            return Err(format!("zip: duplicate entry: {name}"));
        }
        picked.push((name, crc, data_start, data_end));
    }
    // 重なりを確かめてから写す（重なる zip では1バイトも写さない）
    ranges.sort_by_key(|r| (r.0, r.1));
    for w in ranges.windows(2) {
        if w[0].1 > w[1].0 {
            return Err(format!("zip: overlapping entries: {} / {}", w[0].2, w[1].2));
        }
    }
    let mut out = Vec::with_capacity(picked.len());
    for (name, crc, start, end) in picked {
        let data = &b[start..end];
        if crc32(data) != crc {
            return Err(format!("zip: crc mismatch: {name}"));
        }
        out.push(Entry { name, data: data.to_vec() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn roundtrip() {
        let entries = vec![
            Entry { name: "settings.json".into(), data: b"{\"a\":1}".to_vec() },
            Entry { name: "背景/bg.png".into(), data: vec![1, 2, 3, 4, 5] },
        ];
        let mut buf = Vec::new();
        write(&mut buf, &entries).unwrap();
        let back = read(&mut std::io::Cursor::new(buf), &|_| true).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].name, "背景/bg.png");
        assert_eq!(back[1].data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read(&mut std::io::Cursor::new(b"hello world, this is not a zip file".to_vec()), &|_| true).is_err());
    }

    /// テスト用：同じ大きさの項目を `copies` 個持つ中央ディレクトリが、全部1つのデータを指す zip（重なりエントリ）
    fn overlapping_zip(copies: usize, size: usize) -> Vec<u8> {
        let mut one = Vec::new();
        write(&mut one, &[Entry { name: "x000000000000".into(), data: vec![b'a'; size] }]).unwrap();
        let local_len = 30 + 13 + size;
        let central = one[local_len..one.len() - 22].to_vec();
        let mut out = one[..local_len].to_vec();
        for i in 0..copies {
            let mut c = central.clone();
            c[46..46 + 13].copy_from_slice(format!("x{i:012}").as_bytes());
            out.extend_from_slice(&c);
        }
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(copies as u16).to_le_bytes());
        out.extend_from_slice(&(copies as u16).to_le_bytes());
        out.extend_from_slice(&((central.len() * copies) as u32).to_le_bytes());
        out.extend_from_slice(&(local_len as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の1）：中央ディレクトリの項目が同じデータを指す zip は断る。
    /// 1MB のデータを 2000 回指す zip（3.1.0 までなら展開で 2GB を要求した）を、写す前に断る
    #[test]
    fn overlapping_entries_are_rejected() {
        let bomb = overlapping_zip(2000, 1024 * 1024);
        assert!(bomb.len() < 2 * 1024 * 1024);
        // 合計が上限（96MB）を超えた時点で、写す前に止まる
        let e = read(&mut std::io::Cursor::new(&bomb), &|_| true).err().unwrap();
        assert!(e.contains("too large in total"), "{e}");
        // 合計が上限より小さくても、重なりで断る（1MB を 50 回）
        let small = overlapping_zip(50, 1024 * 1024);
        let e = read(&mut std::io::Cursor::new(&small), &|_| true).err().unwrap();
        assert!(e.contains("overlapping"), "{e}");
        // 1つだけなら読める（作り方が正しいことの確かめ）
        let ok = overlapping_zip(1, 1024);
        assert_eq!(read(&mut std::io::Cursor::new(&ok), &|_| true).unwrap().len(), 1);
    }

    /// 大きすぎる zip は上限＋1 バイトまでしか読まずに断る。要らない名前は中身を写さない。同じ名前が2つなら断る
    #[test]
    fn size_cap_wanted_names_and_duplicates() {
        let mut buf = Vec::new();
        write(
            &mut buf,
            &[
                Entry { name: "settings.json".into(), data: b"{}".to_vec() },
                Entry { name: "junk.bin".into(), data: vec![7; 4096] },
            ],
        )
        .unwrap();
        let e = read_limited(&mut std::io::Cursor::new(&buf), &|_| true, 1000).err().unwrap();
        assert!(e.contains("too large"), "{e}");
        // 終わりの無い入力でも、上限＋1 バイトで止まる
        let e = read_limited(&mut std::io::repeat(0), &|_| true, 4096).err().unwrap();
        assert!(e.contains("too large"), "{e}");
        let got = read(&mut std::io::Cursor::new(&buf), &|n| n == "settings.json").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "settings.json");
        let mut dup = Vec::new();
        write(
            &mut dup,
            &[
                Entry { name: "settings.json".into(), data: b"{}".to_vec() },
                Entry { name: "settings.json".into(), data: b"{\"theme\":\"light\"}".to_vec() },
            ],
        )
        .unwrap();
        assert!(read(&mut std::io::Cursor::new(&dup), &|_| true).err().unwrap().contains("duplicate"));
    }
}
