//! フォルダの走査。拡張子で音声ファイルの候補を絞る（中身の確認は import で Symphonia が行う）。

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// 原文の7形式の拡張子
pub const AUDIO_EXTS: &[&str] = &["mp3", "wav", "flac", "m4a", "aac", "ogg", "opus"];

pub fn is_audio_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub files: Vec<PathBuf>,
    /// 音声の拡張子ではなかったファイル（ドロップ・選択で直接渡されたもの）
    pub not_audio: Vec<PathBuf>,
    /// 開けなかった場所（アクセス拒否など）
    pub errors: Vec<(PathBuf, std::io::ErrorKind)>,
    pub folders: Vec<PathBuf>,
}

pub fn scan(paths: &[PathBuf], recursive: bool) -> ScanResult {
    let mut r = ScanResult::default();
    for p in paths {
        if p.is_dir() {
            r.folders.push(p.clone());
            let walker = WalkDir::new(p)
                .follow_links(false)
                .max_depth(if recursive { usize::MAX } else { 1 });
            for entry in walker {
                match entry {
                    Ok(e) => {
                        if e.file_type().is_file() && is_audio_path(e.path()) {
                            r.files.push(e.into_path());
                        }
                    }
                    Err(err) => {
                        let path = err.path().map(|p| p.to_path_buf()).unwrap_or_else(|| p.clone());
                        let kind = err
                            .io_error()
                            .map(|e| e.kind())
                            .unwrap_or(std::io::ErrorKind::Other);
                        r.errors.push((path, kind));
                    }
                }
            }
        } else if p.is_file() {
            if is_audio_path(p) {
                r.files.push(p.clone());
            } else {
                r.not_audio.push(p.clone());
            }
        } else {
            r.errors.push((p.clone(), std::io::ErrorKind::NotFound));
        }
    }
    r.files.sort();
    r.files.dedup();
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_fixtures_and_rejects_non_audio() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        let r = scan(&[dir.clone(), dir.join("..").join("..").join("Cargo.toml")], true);
        assert!(r.files.len() >= 8, "{:?}", r.files);
        assert_eq!(r.not_audio.len(), 1);
        let r2 = scan(&[dir.join("nope")], true);
        assert_eq!(r2.errors.len(), 1);
    }
}
