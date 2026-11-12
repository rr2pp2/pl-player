//! 歌詞の組み合わせ（commands から呼ぶ）。判断はここと lrc・resolver・inbox が持つ。

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::inbox::{self, InboxReport};
use super::lrc::{self, LyricsFormat, LyricsLine};
use super::resolver::{self, SourceKind};
use crate::db::lyrics::{self as store, StoredKind};
use crate::db::{tracks, Db};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::library::tags_read;

/// 曲ごとの時刻補正の上限（±10分）
pub const MAX_DELAY_MS: i64 = 600_000;

/// 歌詞が大きすぎて読み解かなかった（1MiB 超。主に曲ファイルのタグ）。歌詞なしとして扱い、警告にとどめる
pub const WARN_TOO_LARGE: &str = "lyrics_too_large";

/// 解析に渡してよい大きさか（1回目の検査の指摘：解析の前に上限を当てる）
fn within_limit(text: &str) -> bool {
    text.len() <= lrc::MAX_TEXT_BYTES
}

/// 画面へ返す歌詞（src/ipc/types.ts の LyricsPayload と同じ形）
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsPayload {
    pub track_id: i64,
    /// None は歌詞なし
    pub source: Option<SourceKind>,
    pub format: Option<LyricsFormat>,
    pub raw_text: String,
    /// time_ms は「元の時刻 − offset」（0 未満でも切らない）。画面で max(0, time_ms + 曲ごとの補正) にする
    pub lines: Vec<LyricsLine>,
    pub plain_text: String,
    pub user_delay_ms: i64,
    /// 警告のコード。文は src/messages/ja.ts
    pub warnings: Vec<String>,
}

/// 編集画面の「保存前の見本」
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsPreview {
    pub format: LyricsFormat,
    /// 表示される行の数（「消す」合図は数えない）
    pub line_count: usize,
    pub has_content: bool,
    pub too_large: bool,
    pub warnings: Vec<String>,
}

/// 受け渡しフォルダの親（`<データフォルダ>\lyrics`）
pub fn lyrics_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("lyrics")
}

fn track_path(db: &Db, track_id: i64) -> AppResult<String> {
    tracks::path_of(db, track_id)?.ok_or_else(|| AppError::new(ErrorKind::FileNotFound, format!("track {track_id}")))
}

/// 優先順で選んだ歌詞を読み解いて返す
pub fn get_lyrics(db: &Db, track_id: i64) -> AppResult<LyricsPayload> {
    let path = track_path(db, track_id)?;
    let resolved = resolver::resolve(db, track_id, || tags_read::read_lyrics(Path::new(&path)))?;
    let user_delay_ms = store::delay(db, track_id)?;
    let mut warnings = resolved.warnings;
    let raw_text = lrc::normalize(&resolved.raw_text);
    // 曲のタグの歌詞には大きさの上限が無いので、解析する前に当てる（手動・外部は保存のときに当てている）
    let source = if resolved.source.is_some() && !within_limit(&raw_text) {
        tracing::info!("lyrics too large (track {track_id}): {} bytes", raw_text.len());
        warnings.push(WARN_TOO_LARGE.to_string());
        None
    } else {
        resolved.source
    };
    let Some(source) = source else {
        return Ok(LyricsPayload {
            track_id,
            source: None,
            format: None,
            raw_text: String::new(),
            lines: vec![],
            plain_text: String::new(),
            user_delay_ms,
            warnings,
        });
    };
    let parsed = lrc::parse(&raw_text);
    warnings.extend(parsed.warnings);
    Ok(LyricsPayload {
        track_id,
        source: Some(source),
        format: Some(parsed.format),
        raw_text,
        lines: parsed.lines,
        plain_text: parsed.plain_text,
        user_delay_ms,
        warnings,
    })
}

/// 編集画面の見本（ファイルも DB も触らない）
pub fn preview(raw_text: &str) -> LyricsPreview {
    let text = lrc::normalize(raw_text);
    // 大きすぎるものは読み解かない（200ms ごとに呼ばれるので、上限の前に解析しない）
    if !within_limit(&text) {
        return LyricsPreview {
            format: lrc::LyricsFormat::Plain,
            line_count: 0,
            has_content: false,
            too_large: true,
            warnings: vec![],
        };
    }
    let p = lrc::parse(&text);
    LyricsPreview {
        format: p.format,
        line_count: p.lines.iter().filter(|l| l.kind != lrc::LineKind::Clear).count(),
        has_content: p.has_content(),
        too_large: false,
        warnings: p.warnings,
    }
}

/// 読み解けるか確かめてから manual に保存する
pub fn save_manual(db: &Db, track_id: i64, raw_text: &str) -> AppResult<LyricsPayload> {
    if !store::track_exists(db, track_id)? {
        return Err(AppError::new(ErrorKind::FileNotFound, format!("track {track_id}")));
    }
    let text = lrc::normalize(raw_text);
    if !within_limit(&text) {
        return Err(AppError::new(ErrorKind::LyricsFormatInvalid, format!("too large ({} bytes)", text.len())));
    }
    let parsed = lrc::parse(&text);
    if !parsed.has_content() {
        return Err(AppError::new(ErrorKind::LyricsFormatInvalid, "empty"));
    }
    store::upsert(&db.writer(), track_id, StoredKind::Manual, &text, parsed.format.as_str())?;
    get_lyrics(db, track_id)
}

/// manual だけを消す（external・曲のタグに戻る）
pub fn delete_manual(db: &Db, track_id: i64) -> AppResult<LyricsPayload> {
    store::delete(db, track_id, StoredKind::Manual)?;
    get_lyrics(db, track_id)
}

/// 曲ごとの補正を保存する（±600000ms まで）
pub fn set_delay(db: &Db, track_id: i64, delay_ms: i64) -> AppResult<i64> {
    if delay_ms.abs() > MAX_DELAY_MS {
        return Err(AppError::other(format!("delay out of range: {delay_ms}")));
    }
    if !store::track_exists(db, track_id)? {
        return Err(AppError::new(ErrorKind::FileNotFound, format!("track {track_id}")));
    }
    store::set_delay(db, track_id, delay_ms)?;
    Ok(delay_ms)
}

/// 受け渡しフォルダを取り込む
pub fn import_inbox(db: &Db, data_dir: &Path) -> AppResult<InboxReport> {
    inbox::import(db, &lyrics_dir(data_dir))
}

/// 受け渡しフォルダ（inbox）のパス。無ければ3つとも作る
pub fn inbox_dir(data_dir: &Path) -> AppResult<PathBuf> {
    Ok(inbox::ensure_dirs(&lyrics_dir(data_dir))?.inbox)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::TrackInfo;

    fn add(db: &Db, path: &str) -> i64 {
        tracks::upsert(&db.writer(), &TrackInfo { path: path.into(), ..Default::default() })
            .unwrap()
            .0
    }

    /// 優先順：manual ＞ external ＞ embedded_tag。取り込みで manual は変わらない。manual を消すと戻る
    #[test]
    fn priority_through_inbox_and_manual() {
        let db = Db::open_memory("lyrics_service_prio").unwrap();
        let song = tags_read::test_copy_with_lyrics("sine.mp3", "svc", "[00:01.00]タグの歌詞");
        let id = add(&db, &song.to_string_lossy());
        let data = crate::test_tmp().join(format!("plp-svc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let inbox = inbox_dir(&data).unwrap();

        let p = get_lyrics(&db, id).unwrap();
        assert_eq!(p.source, Some(SourceKind::EmbeddedTag));
        assert_eq!(p.format, Some(LyricsFormat::Timed));
        assert_eq!(p.lines[0].text, "タグの歌詞");

        std::fs::write(inbox.join(format!("{id}.txt")), "外部の歌詞・一回目".as_bytes()).unwrap();
        let r = import_inbox(&db, &data).unwrap();
        assert_eq!(r.imported, 1);
        let p = get_lyrics(&db, id).unwrap();
        assert_eq!(p.source, Some(SourceKind::External));
        assert_eq!(p.format, Some(LyricsFormat::Plain));
        assert_eq!(p.plain_text, "外部の歌詞・一回目");

        let p = save_manual(&db, id, "[00:05.00]手動の歌詞\r\n").unwrap();
        assert_eq!(p.source, Some(SourceKind::Manual));
        assert_eq!(p.raw_text, "[00:05.00]手動の歌詞\n");

        // 取り込み直しても manual のまま（external だけ更新）
        std::fs::write(inbox.join(format!("{id}.lrc")), "[00:02.00]外部の歌詞・二回目".as_bytes()).unwrap();
        let r = import_inbox(&db, &data).unwrap();
        assert_eq!(r.manual_kept, 1);
        assert_eq!(r.imported, 0);
        let p = get_lyrics(&db, id).unwrap();
        assert_eq!(p.source, Some(SourceKind::Manual));
        assert_eq!(p.lines[0].text, "手動の歌詞");

        // manual を消すと external（二回目）へ、external も無ければ曲のタグへ
        let p = delete_manual(&db, id).unwrap();
        assert_eq!(p.source, Some(SourceKind::External));
        assert_eq!(p.lines[0].text, "外部の歌詞・二回目");
        assert_eq!(p.lines[0].time_ms, Some(2_000));
        store::delete(&db, id, StoredKind::External).unwrap();
        assert_eq!(get_lyrics(&db, id).unwrap().source, Some(SourceKind::EmbeddedTag));
    }

    /// 1回目の検査 1a：曲のタグの歌詞が 1MiB を超えたら、解析せず「歌詞なし＋警告」
    #[test]
    fn oversized_tag_lyrics_are_not_parsed() {
        let db = Db::open_memory("lyrics_service_bigtag").unwrap();
        let big = format!("[00:01.00]{}", "<".repeat(lrc::MAX_TEXT_BYTES));
        let song = tags_read::test_copy_with_lyrics("sine.mp3", "bigtag", &big);
        let id = add(&db, &song.to_string_lossy());
        let t = std::time::Instant::now();
        let p = get_lyrics(&db, id).unwrap();
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(p.source, None);
        assert!(p.lines.is_empty());
        assert_eq!(p.warnings, vec![WARN_TOO_LARGE.to_string()]);
    }

    #[test]
    fn manual_validation_delay_and_missing_tag() {
        let db = Db::open_memory("lyrics_service_misc").unwrap();
        let id = add(&db, "C:/no/such/file.mp3");
        // 曲ファイルが読めない：歌詞なし＋警告（エラーにしない）
        let p = get_lyrics(&db, id).unwrap();
        assert_eq!(p.source, None);
        assert_eq!(p.warnings, vec![resolver::WARN_EMBEDDED_UNREADABLE.to_string()]);
        // 空・空白だけ・情報行だけは保存しない
        for bad in ["", "  \n ", "[ti:題]"] {
            let e = save_manual(&db, id, bad).unwrap_err();
            assert_eq!(e.kind, ErrorKind::LyricsFormatInvalid, "{bad:?}");
        }
        assert_eq!(save_manual(&db, 9999, "a").unwrap_err().kind, ErrorKind::FileNotFound);
        assert_eq!(get_lyrics(&db, 9999).unwrap_err().kind, ErrorKind::FileNotFound);
        // 補正
        assert_eq!(set_delay(&db, id, 1500).unwrap(), 1500);
        assert_eq!(get_lyrics(&db, id).unwrap().user_delay_ms, 1500);
        assert!(set_delay(&db, id, 600_001).is_err());
        assert!(set_delay(&db, id, -600_001).is_err());
        assert_eq!(set_delay(&db, id, -600_000).unwrap(), -600_000);
        // 1回目の検査 1a：見本・手動保存は 1MiB を超えたら解析しない（閉じない `<` の山でも、すぐ返る）
        let big = "<".repeat(lrc::MAX_TEXT_BYTES + 1);
        let t = std::time::Instant::now();
        let pv = preview(&big);
        assert!(pv.too_large && !pv.has_content && pv.line_count == 0);
        assert_eq!(save_manual(&db, id, &big).unwrap_err().kind, ErrorKind::LyricsFormatInvalid);
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
        // ちょうど 1MiB は読み解く
        assert!(!preview(&"a".repeat(lrc::MAX_TEXT_BYTES)).too_large);
        // 見本
        let pv = preview("[00:01.00]一\n[00:02.00]\n[00:03.00]三");
        assert_eq!(pv.format, LyricsFormat::Timed);
        assert_eq!(pv.line_count, 2);
        assert!(pv.has_content);
        assert!(!preview(" ").has_content);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の10）：行の上限は、手動保存・編集の見本・曲のタグ・外部の歌詞のどれにも当たる
    #[test]
    fn line_cap_applies_to_every_source() {
        let db = Db::open_memory("lyrics_service_cap").unwrap();
        let many: String = (0..6000).map(|i| format!("[{:02}:{:02}.00]行{i}\n", i / 60, i % 60)).collect();
        let cut = |p: &LyricsPayload| p.lines.len() == lrc::MAX_LINES && p.warnings.iter().any(|w| w == lrc::warn::LINES_TRUNCATED);
        // 曲のタグ
        let song = tags_read::test_copy_with_lyrics("sine.mp3", "cap", &many);
        let id = add(&db, &song.to_string_lossy());
        let p = get_lyrics(&db, id).unwrap();
        assert_eq!(p.source, Some(SourceKind::EmbeddedTag));
        assert!(cut(&p));
        // 外部（受け渡しフォルダ・ネットから取ったものは external として保存される）
        store::upsert(&db.writer(), id, StoredKind::External, &many, "timed").unwrap();
        let p = get_lyrics(&db, id).unwrap();
        assert_eq!(p.source, Some(SourceKind::External));
        assert!(cut(&p));
        // 編集の見本：警告が出る
        assert!(preview(&many).warnings.iter().any(|w| w == lrc::warn::LINES_TRUNCATED));
        // 手動保存：保存はでき、表示は 5,000 行と警告
        let p = save_manual(&db, id, &many).unwrap();
        assert_eq!(p.source, Some(SourceKind::Manual));
        assert!(cut(&p));
        let _ = std::fs::remove_dir_all(song.parent().unwrap());
    }
}
