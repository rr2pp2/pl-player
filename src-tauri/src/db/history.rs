//! 再生の記録（v3.6.0。移行 6 の `play_history`）と、それを数える「年間まとめ」。
//!
//! - 記録を書くのは `tracks::mark_played_at` だけ（再生回数 `tracks.play_count` を +1 するのと同じトランザクション）。
//!   数え方は今の再生回数と同じ「再生を始めたら1回」。数え方を2か所で別々に持たない
//! - 時刻は `tracks.last_played_at` と同じ表し方（1970-01-01 UTC からのミリ秒。`db::now_ms`）
//! - 曲をライブラリから外すと、記録も消える（表の `ON DELETE CASCADE`）
//! - 古い記録は消さない（README の「変更点（v3.6.0）」。数えるときは索引で期間の分だけを読む）

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::tracks::{self, Track};
use super::{search, Db};
use crate::error::AppResult;

/// 「1年」の長さ：365 日（ミリ秒）。
/// 暦の1年（去年の同じ日付の同じ時刻から）にしないのは、
/// - 記録は UTC のミリ秒なので、日付で区切るには時差・夏時間の決まりが要り、2月29日の「去年の同じ日」が決まらない
/// - 365 日なら、どの日に開いても同じ長さの期間で数える（うるう年をまたぐ年だけ、暦の1年より1日短い）
pub const YEAR_MS: i64 = 365 * 24 * 60 * 60 * 1000;

/// 年間まとめに出す曲の数
pub const TOP_N: i64 = 100;

/// 1回の再生を記録する（`tracks::mark_played_at` から、同じトランザクションの中で呼ぶ）
pub(crate) fn record_play(conn: &Connection, id: i64, at: i64) -> AppResult<()> {
    conn.execute("INSERT INTO play_history (track_id, played_at) VALUES (?1, ?2)", params![id, at])?;
    Ok(())
}

/// 数え始めた時刻（移行 6 が走った時刻）。無ければ None（表が無い DB は無いはずだが、念のため）
pub fn started_at(conn: &Connection) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row("SELECT started_at FROM play_history_meta WHERE id = 1", [], |r| r.get(0))
        .optional()?)
}

/// 年間まとめの1曲
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedTrack {
    /// 順位（1 から）。検索で絞っても、絞る前の順位のまま
    pub rank: i64,
    /// 期間の中で再生した回数
    pub plays: i64,
    /// 期間の中で最後に再生した時刻
    pub last_played_at: i64,
    pub track: Track,
}

/// 年間まとめ（画面へ渡す形）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearlyTop {
    /// 数える期間：since 〜 until（ミリ秒。until は画面を開いた時点の今）
    pub since: i64,
    pub until: i64,
    /// 数え始めた時刻（3.6.0 に上げて最初に起動した時刻）。since より後なら、期間の頭の分は記録が無い
    pub started_at: Option<i64>,
    /// 検索で絞る前の曲の数（TOP_N 以下）
    pub ranked: usize,
    /// 順位の順（検索で絞ったときは、当たった曲だけ）
    pub entries: Vec<RankedTrack>,
}

/// `now` から 1 年（`YEAR_MS`）さかのぼった期間に再生した回数の、上位 `TOP_N` 曲。
///
/// 並び：回数の多い順 → 同じ回数なら、期間の中で最後に再生した時刻の新しい順 → それも同じなら曲の id の小さい順。
/// 同じ回数の曲を「最近も聞いている曲」を上にするのは、まとめを開いた人の今の好みに近い順のため。
/// 最後の id は、同じミリ秒に記録が重なっても（ほぼ起きない）、開くたびに並びが変わらないようにするため。
/// 順位は同じ回数でも 1 ずつ進める（上の決まりで並びが1通りに決まるので、上位 100 の境目もあいまいにならない。回数は列に出る）。
///
/// 検索語があれば、上位 `TOP_N` を決めてから、その中で当たる曲だけを返す（順位は絞る前のまま）。
/// 期間の終わりも `now` で切る（PC の時計を戻したときの、先の時刻の記録は入れない）
pub fn yearly_top(db: &Db, now: i64, search_text: Option<&str>) -> AppResult<YearlyTop> {
    let since = now - YEAR_MS;
    let conn = db.reader();
    let started_at = started_at(&conn)?;
    let ranked: Vec<(i64, i64, i64)> = {
        // 索引 idx_play_history_played_at（played_at, track_id）で期間の分だけを読む。
        // 曲の行があるものだけを数える（外した曲の記録は消えているが、外部キーを切った接続で消された場合にも出さない）
        let mut stmt = conn.prepare(
            "SELECT p.track_id, COUNT(*) AS plays, MAX(p.played_at) AS last_at
             FROM play_history p
             WHERE p.played_at >= ?1 AND p.played_at <= ?2
               AND EXISTS (SELECT 1 FROM tracks t WHERE t.id = p.track_id)
             GROUP BY p.track_id
             ORDER BY plays DESC, last_at DESC, p.track_id ASC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![since, now, TOP_N], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut wheres: Vec<String> = vec!["t.id = ?1".into()];
    let mut args: Vec<rusqlite::types::Value> = vec![rusqlite::types::Value::Null];
    if let Some(s) = search_text {
        search::push_search_filter(s, &mut wheres, &mut args);
    }
    let sql = format!("{} WHERE {}", tracks::SELECT_TRACK, wheres.join(" AND "));
    let mut stmt = conn.prepare(&sql)?;
    let mut entries = Vec::with_capacity(ranked.len());
    for (i, (id, plays, last_at)) in ranked.iter().enumerate() {
        args[0] = (*id).into();
        let t = stmt
            .query_row(rusqlite::params_from_iter(args.iter()), |r| tracks::row_to_track(db, r))
            .optional()?;
        if let Some(track) = t {
            entries.push(RankedTrack {
                rank: i as i64 + 1,
                plays: *plays,
                last_played_at: *last_at,
                track,
            });
        }
    }
    Ok(YearlyTop {
        since,
        until: now,
        started_at,
        ranked: ranked.len(),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tracks::{mark_played_at, upsert, TrackInfo};

    const DAY: i64 = 24 * 60 * 60 * 1000;
    /// 試しの「今」：2026-11-14 00:00 UTC（実際の時計は使わない）
    const NOW: i64 = 1_794_614_400_000;

    fn add(db: &Db, path: &str, title: &str) -> i64 {
        upsert(
            &db.writer(),
            &TrackInfo {
                path: path.into(),
                title: title.into(),
                artist: "Test Artist".into(),
                format: "mp3".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .0
    }

    fn plays(db: &Db, id: i64, times: &[i64]) {
        for t in times {
            mark_played_at(db, id, *t).unwrap();
        }
    }

    fn history_rows(db: &Db) -> i64 {
        db.reader().query_row("SELECT COUNT(*) FROM play_history", [], |r| r.get(0)).unwrap()
    }

    /// 再生を始めた1回が、再生回数（play_count・last_played_at）と記録の両方に、同じ時刻で入る。
    /// ライブラリに無い曲は、どちらにも入らない（エラーにもしない）
    #[test]
    fn a_play_counts_once_in_both_places() {
        let db = Db::open_memory("history_mark").unwrap();
        let a = add(&db, "C:/m/a.mp3", "Paper Lantern");
        mark_played_at(&db, a, NOW - 5).unwrap();
        mark_played_at(&db, a, NOW - 3).unwrap();
        let t = tracks::get(&db, a).unwrap().unwrap();
        assert_eq!((t.play_count, t.last_played_at), (2, Some(NOW - 3)));
        let times: Vec<i64> = {
            let c = db.reader();
            let mut s = c.prepare("SELECT played_at FROM play_history WHERE track_id = ?1 ORDER BY played_at").unwrap();
            let v = s.query_map([a], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
            v
        };
        assert_eq!(times, vec![NOW - 5, NOW - 3]);
        // ライブラリに無い id
        mark_played_at(&db, 987_654, NOW).unwrap();
        assert_eq!(history_rows(&db), 2);
        // 今の時計で記録する入口（再生の司令塔が呼ぶもの）も、同じ所を通る
        tracks::mark_played(&db, a).unwrap();
        assert_eq!(tracks::get(&db, a).unwrap().unwrap().play_count, 3);
        assert_eq!(history_rows(&db), 3);
    }

    /// 期間は今から 365 日さかのぼったところから今まで。境目ちょうどは入り、1ミリ秒でも古い・先の時刻の記録は入らない。
    /// 並びは回数の多い順 → 同じ回数なら最後に再生した時刻の新しい順 → 曲の id の順
    #[test]
    fn counts_the_last_365_days_in_a_fixed_order() {
        let db = Db::open_memory("history_year").unwrap();
        let a = add(&db, "C:/m/a.mp3", "Alpha Song");
        let b = add(&db, "C:/m/b.mp3", "Beta Song");
        let c = add(&db, "C:/m/c.mp3", "Gamma Song");
        let d = add(&db, "C:/m/d.mp3", "Delta Song");
        let e = add(&db, "C:/m/e.mp3", "Epsilon Song");
        assert_eq!(YEAR_MS, 365 * DAY);
        // a：期間の中で2回（境目ちょうどを含む）＋期間の前に3回（数えない）
        plays(&db, a, &[NOW - YEAR_MS, NOW - 10 * DAY, NOW - YEAR_MS - 1, NOW - 400 * DAY, NOW - 2 * YEAR_MS]);
        // b：3回
        plays(&db, b, &[NOW - 300 * DAY, NOW - 200 * DAY, NOW - 100 * DAY]);
        // c：2回。最後は a より新しい
        plays(&db, c, &[NOW - 50 * DAY, NOW - DAY]);
        // d：先の時刻の記録だけ（PC の時計を戻した）。数えない
        plays(&db, d, &[NOW + 1, NOW + DAY]);
        // e：期間の前だけ。数えない
        plays(&db, e, &[NOW - YEAR_MS - DAY]);
        let top = yearly_top(&db, NOW, None).unwrap();
        assert_eq!((top.since, top.until), (NOW - YEAR_MS, NOW));
        let got: Vec<(i64, i64, i64, i64)> = top.entries.iter().map(|x| (x.rank, x.track.id, x.plays, x.last_played_at)).collect();
        assert_eq!(got, vec![(1, b, 3, NOW - 100 * DAY), (2, c, 2, NOW - DAY), (3, a, 2, NOW - 10 * DAY)]);
        assert_eq!(top.ranked, 3);
        // 再生回数（play_count）は今までどおり、期間に関係なく全部
        assert_eq!(tracks::get(&db, a).unwrap().unwrap().play_count, 5);
        // 同じ回数・同じ時刻なら、id の小さい順（開くたびに並びが変わらない）
        let f = add(&db, "C:/m/f.mp3", "Zeta Song");
        let g = add(&db, "C:/m/g.mp3", "Eta Song");
        plays(&db, g, &[NOW - 7]);
        plays(&db, f, &[NOW - 7]);
        let top = yearly_top(&db, NOW, None).unwrap();
        let ids: Vec<i64> = top.entries.iter().map(|x| x.track.id).collect();
        assert_eq!(ids, vec![b, c, a, f.min(g), f.max(g)]);
    }

    /// 上位 100 曲まで。101 曲目からは出さない
    #[test]
    fn only_the_top_100() {
        let db = Db::open_memory("history_top100").unwrap();
        let mut ids = vec![];
        for i in 0..105 {
            let id = add(&db, &format!("C:/m/{i:03}.mp3"), &format!("Song {i:03}"));
            // i が大きいほど多く再生した（1〜105 回）
            plays(&db, id, &vec![NOW - DAY; i + 1]);
            ids.push(id);
        }
        let top = yearly_top(&db, NOW, None).unwrap();
        assert_eq!(top.entries.len(), TOP_N as usize);
        assert_eq!(top.ranked, TOP_N as usize);
        assert_eq!(top.entries[0].track.id, ids[104]);
        assert_eq!(top.entries[0].plays, 105);
        assert_eq!(top.entries[99].track.id, ids[5]);
        assert_eq!(top.entries[99].rank, 100);
        assert!(!top.entries.iter().any(|x| ids[..5].contains(&x.track.id)));
    }

    /// 検索で絞っても、順位は絞る前のまま。上位 100 に入らない曲は、検索で当たっても出さない
    #[test]
    fn search_keeps_the_rank() {
        let db = Db::open_memory("history_search").unwrap();
        let a = add(&db, "C:/m/a.mp3", "Morning Lantern");
        let b = add(&db, "C:/m/b.mp3", "Evening Bell");
        let c = add(&db, "C:/m/c.mp3", "Night Lantern");
        plays(&db, a, &[NOW - 3 * DAY, NOW - 2 * DAY, NOW - DAY]);
        plays(&db, b, &[NOW - 2 * DAY, NOW - DAY]);
        plays(&db, c, &[NOW - DAY]);
        let top = yearly_top(&db, NOW, Some("Lantern")).unwrap();
        let got: Vec<(i64, i64)> = top.entries.iter().map(|x| (x.rank, x.track.id)).collect();
        assert_eq!(got, vec![(1, a), (3, c)]);
        assert_eq!(top.ranked, 3);
        // 1〜2文字の語（LIKE で引く）でも同じ
        let top = yearly_top(&db, NOW, Some("Be")).unwrap();
        assert_eq!(top.entries.iter().map(|x| (x.rank, x.track.id)).collect::<Vec<_>>(), vec![(2, b)]);
        // 再生していない曲は、検索で当たっても出さない
        add(&db, "C:/m/d.mp3", "Lantern Unplayed");
        assert_eq!(yearly_top(&db, NOW, Some("Lantern")).unwrap().entries.len(), 2);
    }

    /// 曲をライブラリから外すと、その曲の記録も消える（一覧に出ない。同じファイルを取り込み直しても、前の記録は戻らない）。
    /// ごみ箱へ移すも、ライブラリから外すのは同じ `tracks::remove`
    #[test]
    fn removing_a_track_removes_its_history() {
        let db = Db::open_memory("history_remove").unwrap();
        let a = add(&db, "C:/m/a.mp3", "Paper Lantern");
        let b = add(&db, "C:/m/b.mp3", "Glass Harbor");
        plays(&db, a, &[NOW - 2 * DAY, NOW - DAY]);
        plays(&db, b, &[NOW - DAY]);
        assert_eq!(history_rows(&db), 3);
        tracks::remove(&db, &[a]).unwrap();
        assert_eq!(history_rows(&db), 1);
        let top = yearly_top(&db, NOW, None).unwrap();
        assert_eq!(top.entries.iter().map(|x| (x.rank, x.track.id)).collect::<Vec<_>>(), vec![(1, b)]);
        // 同じファイルを取り込み直す：新しい行で、記録は無い
        let a2 = add(&db, "C:/m/a.mp3", "Paper Lantern");
        assert_eq!(tracks::get(&db, a2).unwrap().unwrap().play_count, 0);
        assert_eq!(yearly_top(&db, NOW, None).unwrap().entries.len(), 1);
    }

    /// 数え始めた時刻は、移行 6 が走った時刻（DB を開いた時刻の近く）。記録が無いときも、まとめは空で返る
    #[test]
    fn started_at_is_set_by_the_migration() {
        let before = crate::db::now_ms();
        let db = Db::open_memory("history_started").unwrap();
        let after = crate::db::now_ms();
        let s = started_at(&db.reader()).unwrap().unwrap();
        // SQLite の時計（julianday）と Rust の時計の丸めの差を 1 秒まで見る
        assert!(s >= before - 1000 && s <= after + 1000, "{before} <= {s} <= {after}");
        let top = yearly_top(&db, after, None).unwrap();
        assert!(top.entries.is_empty());
        assert_eq!(top.started_at, Some(s));
    }
}
