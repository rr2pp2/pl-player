//! どの歌詞を出すか（仕様 2.1）：manual ＞ external ＞ embedded_tag。
//!
//! 曲ファイルのタグは、manual も external も無いときだけ読む（保存しない。表示のたびに読む）。

use serde::Serialize;

use crate::db::lyrics::{self as store, StoredKind};
use crate::db::Db;
use crate::error::{AppError, AppResult};

/// 曲ファイルのタグを読めなかった（歌詞なしとして扱い、警告にとどめる）
pub const WARN_EMBEDDED_UNREADABLE: &str = "embedded_tag_unreadable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Manual,
    External,
    EmbeddedTag,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// None は歌詞なし
    pub source: Option<SourceKind>,
    pub raw_text: String,
    pub warnings: Vec<String>,
}

/// 優先順で1つ選ぶ。`read_embedded` は manual・external が無いときだけ呼ぶ
pub fn resolve(
    db: &Db,
    track_id: i64,
    read_embedded: impl FnOnce() -> Result<Option<String>, AppError>,
) -> AppResult<Resolved> {
    for (kind, source) in [
        (StoredKind::Manual, SourceKind::Manual),
        (StoredKind::External, SourceKind::External),
    ] {
        if let Some(s) = store::get(db, track_id, kind)? {
            return Ok(Resolved {
                source: Some(source),
                raw_text: s.raw_text,
                warnings: vec![],
            });
        }
    }
    match read_embedded() {
        Ok(Some(text)) => Ok(Resolved {
            source: Some(SourceKind::EmbeddedTag),
            raw_text: text,
            warnings: vec![],
        }),
        Ok(None) => Ok(Resolved {
            source: None,
            raw_text: String::new(),
            warnings: vec![],
        }),
        Err(e) => {
            tracing::info!("lyrics tag read failed (track {track_id}): {}", e.detail);
            Ok(Resolved {
                source: None,
                raw_text: String::new(),
                warnings: vec![WARN_EMBEDDED_UNREADABLE.to_string()],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::{self, TrackInfo};
    use crate::error::ErrorKind;

    #[test]
    fn priority_manual_external_embedded() {
        let db = Db::open_memory("lyrics_resolver").unwrap();
        let id = tracks::upsert(&db.writer(), &TrackInfo { path: "C:/r/a.mp3".into(), ..Default::default() })
            .unwrap()
            .0;
        let tag = || Ok(Some("タグの歌詞".to_string()));
        assert_eq!(resolve(&db, id, tag).unwrap().source, Some(SourceKind::EmbeddedTag));
        store::upsert(&db.writer(), id, StoredKind::External, "外部の歌詞", "plain").unwrap();
        let r = resolve(&db, id, || panic!("manual・external があればタグは読まない")).unwrap();
        assert_eq!(r.source, Some(SourceKind::External));
        assert_eq!(r.raw_text, "外部の歌詞");
        store::upsert(&db.writer(), id, StoredKind::Manual, "手動の歌詞", "plain").unwrap();
        assert_eq!(resolve(&db, id, tag).unwrap().source, Some(SourceKind::Manual));
        store::delete(&db, id, StoredKind::Manual).unwrap();
        assert_eq!(resolve(&db, id, tag).unwrap().source, Some(SourceKind::External));
        store::delete(&db, id, StoredKind::External).unwrap();
        assert_eq!(resolve(&db, id, tag).unwrap().source, Some(SourceKind::EmbeddedTag));
        // タグが無い・読めない
        assert_eq!(resolve(&db, id, || Ok(None)).unwrap().source, None);
        let bad = resolve(&db, id, || Err(AppError::new(ErrorKind::TagReadFailed, "x"))).unwrap();
        assert_eq!(bad.source, None);
        assert_eq!(bad.warnings, vec![WARN_EMBEDDED_UNREADABLE.to_string()]);
    }
}
