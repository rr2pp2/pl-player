//! テスト用の zip と画像（v3.2.0）。画像は単純な図形だけをその場で作る（実在の絵は使わない）。
//!
//! - zip：無圧縮・deflate・暗号化の印・Unix のリンク・名前の食い違い・CRC 違いなど、壊れた zip も作れる
//! - 動く画像：GIF（自前の LZW）・APNG（png の塊を組む）・動く WebP（無損失の1枚を ANMF に包む）

use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder, Rgba, RgbaImage};

// ---------------- zip ----------------

#[derive(Clone)]
pub struct Z {
    pub name: Vec<u8>,
    pub data: Vec<u8>,
    pub deflate: bool,
    pub made_by: u16,
    pub external_attr: u32,
    pub flags: u16,
    /// 中央ディレクトリに書く CRC を変える（CRC 違い）
    pub crc_override: Option<u32>,
    /// 中央ディレクトリに書く「戻した大きさ」を変える
    pub size_override: Option<u32>,
    /// ローカルヘッダの名前を変える
    pub local_name: Option<Vec<u8>>,
    /// 圧縮したデータの後ろに足すバイト（圧縮後の大きさの申告にも含める）
    pub trailing: Vec<u8>,
}

impl Z {
    pub fn file(name: &str, data: &[u8]) -> Self {
        Self::raw(name.as_bytes(), data)
    }
    pub fn raw(name: &[u8], data: &[u8]) -> Self {
        Z {
            name: name.to_vec(),
            data: data.to_vec(),
            deflate: false,
            made_by: 20,
            external_attr: 0,
            flags: 0x0800,
            crc_override: None,
            size_override: None,
            local_name: None,
            trailing: Vec::new(),
        }
    }
    pub fn dir(name: &str) -> Self {
        let mut z = Self::file(name, &[]);
        z.external_attr = 0x10;
        z
    }
    pub fn deflated(mut self) -> Self {
        self.deflate = true;
        self
    }
}

pub fn zip(entries: &[Z]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let crc = e.crc_override.unwrap_or_else(|| crate::settings::test_crc32(&e.data));
        let mut body = if e.deflate { miniz_oxide::deflate::compress_to_vec(&e.data, 6) } else { e.data.clone() };
        body.extend_from_slice(&e.trailing);
        let method: u16 = if e.deflate { 8 } else { 0 };
        let usize_ = e.size_override.unwrap_or(e.data.len() as u32);
        let offset = out.len() as u32;
        let lname = e.local_name.clone().unwrap_or_else(|| e.name.clone());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&e.flags.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x21u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&usize_.to_le_bytes());
        out.extend_from_slice(&(lname.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&lname);
        out.extend_from_slice(&body);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&e.made_by.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&e.flags.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0x21u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(body.len() as u32).to_le_bytes());
        central.extend_from_slice(&usize_.to_le_bytes());
        central.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&e.external_attr.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(&e.name);
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

// ---------------- 図形 ----------------

/// 透明の上に、丸（`round`）か四角を1つ描く
pub fn shape(w: u32, h: u32, color: [u8; 3], round: bool) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    let (cx, cy, r) = (w as f32 / 2.0, h as f32 / 2.0, w.min(h) as f32 * 0.4);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
        let inside = if round { dx * dx + dy * dy <= r * r } else { dx.abs() <= r && dy.abs() <= r };
        if inside {
            *p = Rgba([color[0], color[1], color[2], 255]);
        }
    }
    img
}

/// 立ち絵の代わりの図形（丸い頭と四角い体。`phase` で頭の位置が少し動く）
pub fn figure(w: u32, h: u32, color: [u8; 3], phase: u32) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    let head_r = w as f32 * 0.18;
    let (hx, hy) = (w as f32 / 2.0 + (phase % 8) as f32 * (w as f32 / 80.0), h as f32 * 0.18);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let head = (fx - hx).powi(2) + (fy - hy).powi(2) <= head_r * head_r;
        let body = fy >= h as f32 * 0.32 && (fx - w as f32 / 2.0).abs() <= w as f32 * 0.3;
        if head || body {
            *p = Rgba([color[0], color[1], color[2], 255]);
        }
    }
    img
}

// ---------------- 静止画 ----------------

pub fn png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out).write_image(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgba8).unwrap();
    out
}

pub fn webp(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out)
        .encode(img.as_raw(), img.width(), img.height(), ExtendedColorType::Rgba8)
        .unwrap();
    out
}

// ---------------- PNG の塊 ----------------

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crate::settings::test_crc32(&body).to_be_bytes());
}

/// PNG の塊を並べる（種類, 中身）
fn png_chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut v = vec![];
    let mut p = 8;
    while p + 8 <= png.len() {
        let len = u32::from_be_bytes(png[p..p + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = png[p + 4..p + 8].try_into().unwrap();
        v.push((kind, png[p + 8..p + 8 + len].to_vec()));
        p += 12 + len;
    }
    v
}

/// APNG（全部のコマが同じ大きさ。最初のコマは既定の画像も兼ねる）
pub fn apng(frames: &[RgbaImage], delay_ms: u16) -> Vec<u8> {
    let (w, h) = frames[0].dimensions();
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let first = png_chunks(&png(&frames[0]));
    let ihdr = first.iter().find(|c| &c.0 == b"IHDR").unwrap().1.clone();
    chunk(&mut out, b"IHDR", &ihdr);
    let mut actl = (frames.len() as u32).to_be_bytes().to_vec();
    actl.extend_from_slice(&0u32.to_be_bytes());
    chunk(&mut out, b"acTL", &actl);
    let mut seq = 0u32;
    let mut idat: Vec<Vec<u8>> = vec![];
    for (i, f) in frames.iter().enumerate() {
        let mut fctl = seq.to_be_bytes().to_vec();
        seq += 1;
        for v in [w, h, 0, 0] {
            fctl.extend_from_slice(&v.to_be_bytes());
        }
        fctl.extend_from_slice(&delay_ms.to_be_bytes());
        fctl.extend_from_slice(&1000u16.to_be_bytes());
        fctl.extend_from_slice(&[0, 0]);
        chunk(&mut out, b"fcTL", &fctl);
        let data = match frames[..i].iter().position(|g| g == f) {
            Some(j) => idat[j].clone(),
            None => png_chunks(&png(f)).into_iter().filter(|c| &c.0 == b"IDAT").flat_map(|c| c.1).collect(),
        };
        idat.push(data.clone());
        if i == 0 {
            chunk(&mut out, b"IDAT", &data);
        } else {
            let mut fdat = seq.to_be_bytes().to_vec();
            seq += 1;
            fdat.extend_from_slice(&data);
            chunk(&mut out, b"fdAT", &fdat);
        }
    }
    chunk(&mut out, b"IEND", &[]);
    out
}

// ---------------- 動く WebP ----------------

fn riff_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(kind);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

fn u24(v: u32) -> [u8; 3] {
    let b = v.to_le_bytes();
    [b[0], b[1], b[2]]
}

/// 動く WebP（無損失の1枚ずつを ANMF に包む）
pub fn webp_anim(frames: &[RgbaImage], delay_ms: u32) -> Vec<u8> {
    let (w, h) = frames[0].dimensions();
    let mut body = b"WEBP".to_vec();
    let mut vp8x = vec![0x10 | 0x02, 0, 0, 0];
    vp8x.extend_from_slice(&u24(w - 1));
    vp8x.extend_from_slice(&u24(h - 1));
    riff_chunk(&mut body, b"VP8X", &vp8x);
    riff_chunk(&mut body, b"ANIM", &[0, 0, 0, 0, 0, 0]);
    let mut cache: Vec<Vec<u8>> = vec![];
    for (i, f) in frames.iter().enumerate() {
        let one = match frames[..i].iter().position(|g| g == f) {
            Some(j) => cache[j].clone(),
            None => webp(f),
        };
        cache.push(one.clone());
        // RIFF(12) のあとの塊から VP8L を探す
        let mut p = 12;
        let mut vp8l = None;
        while p + 8 <= one.len() {
            let kind: [u8; 4] = one[p..p + 4].try_into().unwrap();
            let len = u32::from_le_bytes(one[p + 4..p + 8].try_into().unwrap()) as usize;
            if &kind == b"VP8L" {
                vp8l = Some(one[p + 8..p + 8 + len].to_vec());
            }
            p += 8 + len + (len % 2);
        }
        let mut anmf = Vec::new();
        anmf.extend_from_slice(&u24(0));
        anmf.extend_from_slice(&u24(0));
        anmf.extend_from_slice(&u24(w - 1));
        anmf.extend_from_slice(&u24(h - 1));
        anmf.extend_from_slice(&u24(delay_ms));
        anmf.push(0b10); // 混ぜない・捨てない
        riff_chunk(&mut anmf, b"VP8L", &vp8l.expect("VP8L"));
        riff_chunk(&mut body, b"ANMF", &anmf);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

// ---------------- 動く GIF（自前の LZW） ----------------

struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, code: u16, size: u32) {
        self.acc |= (code as u32) << self.n;
        self.n += size;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

fn lzw(indices: &[u8], min: u32) -> Vec<u8> {
    let clear: u16 = 1 << min;
    let eoi = clear + 1;
    let mut dict = vec![0u16; 4096 * 256];
    let mut next = eoi + 1;
    let mut size = min + 1;
    let mut bits = Bits { out: vec![], acc: 0, n: 0 };
    bits.put(clear, size);
    let mut prefix: Option<u16> = None;
    for &k in indices {
        let Some(p) = prefix else {
            prefix = Some(k as u16);
            continue;
        };
        let slot = (p as usize) << 8 | k as usize;
        if dict[slot] != 0 {
            prefix = Some(dict[slot]);
            continue;
        }
        bits.put(p, size);
        if next >= (1 << size) && size < 12 {
            size += 1;
        }
        if next < 4096 {
            dict[slot] = next;
            next += 1;
        } else {
            bits.put(clear, size);
            dict.iter_mut().for_each(|d| *d = 0);
            next = eoi + 1;
            size = min + 1;
        }
        prefix = Some(k as u16);
    }
    if let Some(p) = prefix {
        bits.put(p, size);
        if next >= (1 << size) && size < 12 {
            size += 1;
        }
    }
    bits.put(eoi, size);
    bits.finish()
}

/// 動く GIF。色は全部のコマを合わせて 255 色まで（0 番は透明）
pub fn gif_anim(frames: &[RgbaImage], delay_cs: u16) -> Vec<u8> {
    let (w, h) = frames[0].dimensions();
    let mut palette: Vec<[u8; 3]> = vec![[0, 0, 0]];
    // 同じ中身のコマは1回だけ符号化する（テストを速くするため）
    let mut done: Vec<(usize, Vec<u8>)> = vec![];
    let mut idx_frames: Vec<Vec<u8>> = vec![];
    for (fi, f) in frames.iter().enumerate() {
        if let Some((_, enc)) = done.iter().find(|(j, _)| frames[*j] == *f) {
            idx_frames.push(enc.clone());
            continue;
        }
        let mut idx = Vec::with_capacity((w * h) as usize);
        let mut last: Option<([u8; 3], u8)> = None;
        for p in f.pixels() {
            if p[3] < 128 {
                idx.push(0u8);
                continue;
            }
            let c = [p[0], p[1], p[2]];
            if let Some((lc, li)) = last {
                if lc == c {
                    idx.push(li);
                    continue;
                }
            }
            let i = match palette.iter().position(|x| *x == c) {
                Some(i) => i,
                None => {
                    assert!(palette.len() < 256, "too many colors");
                    palette.push(c);
                    palette.len() - 1
                }
            };
            last = Some((c, i as u8));
            idx.push(i as u8);
        }
        let enc = lzw(&idx, 8);
        done.push((fi, enc.clone()));
        idx_frames.push(enc);
    }
    palette.resize(256, [0, 0, 0]);
    let mut out = b"GIF89a".to_vec();
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    out.extend_from_slice(&[0xF7, 0, 0]);
    for c in &palette {
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
    out.extend_from_slice(b"NETSCAPE2.0");
    out.extend_from_slice(&[0x03, 0x01, 0, 0, 0]);
    for idx in &idx_frames {
        out.extend_from_slice(&[0x21, 0xF9, 0x04, 0b0000_1001]);
        out.extend_from_slice(&delay_cs.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.push(0x2C);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(w as u16).to_le_bytes());
        out.extend_from_slice(&(h as u16).to_le_bytes());
        out.push(0);
        out.push(8);
        for block in idx.chunks(255) {
            out.push(block.len() as u8);
            out.extend_from_slice(block);
        }
        out.push(0);
    }
    out.push(0x3B);
    out
}

/// コマの並び（図形が少しずつ動く。8 通りを繰り返す）
pub fn moving(w: u32, h: u32, n: u32) -> Vec<RgbaImage> {
    let base: Vec<RgbaImage> = (0..8.min(n)).map(|i| figure(w, h, [120, 110, 200], i)).collect();
    (0..n as usize).map(|i| base[i % base.len()].clone()).collect()
}
