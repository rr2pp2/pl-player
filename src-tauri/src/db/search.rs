//! 曲名・アーティスト・アルバムの検索。
//!
//! 3文字以上の語は FTS5（trigram）で引き、1〜2文字の語は LIKE で引く（trigram が3文字未満を引けないため）。
//! 空白で区切った語は、すべてを満たすもの（AND）を返す。

use rusqlite::types::Value;

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '%' || c == '_' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn fts_phrase(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// 検索語から WHERE の条件を足す。`args` の番号は続きから振る。
pub fn push_search_filter(search: &str, wheres: &mut Vec<String>, args: &mut Vec<Value>) {
    for word in search.split_whitespace() {
        let n = word.chars().count();
        if n == 0 {
            continue;
        }
        if n >= 3 {
            wheres.push(format!(
                "t.id IN (SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?{})",
                args.len() + 1
            ));
            args.push(Value::Text(fts_phrase(word)));
        } else {
            let i = args.len() + 1;
            wheres.push(format!(
                "(t.title LIKE ?{i} ESCAPE '\\' OR t.artist LIKE ?{i} ESCAPE '\\' OR t.album LIKE ?{i} ESCAPE '\\')"
            ));
            args.push(Value::Text(format!("%{}%", escape_like(word))));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tracks::{query, upsert, TrackInfo, TrackQuery};
    use crate::db::Db;

    fn add(db: &Db, path: &str, title: &str, artist: &str, album: &str) {
        upsert(
            &db.writer(),
            &TrackInfo {
                path: path.into(),
                title: title.into(),
                artist: artist.into(),
                album: album.into(),
                ..Default::default()
            },
        )
        .unwrap();
    }

    fn search(db: &Db, s: &str) -> Vec<String> {
        query(
            db,
            &TrackQuery {
                view: "all".into(),
                key: None,
                search: Some(s.into()),
                sort: Some("title".into()),
                desc: None,
            },
        )
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect()
    }

    #[test]
    fn short_and_long_words() {
        let db = Db::open_memory("search_words").unwrap();
        add(&db, "C:/1.flac", "夜明けのスケッチ", "NIGHTOWL", "FIRST NOTE");
        add(&db, "C:/2.flac", "灯台", "NIGHTOWL", "FIRST NOTE");
        add(&db, "C:/3.flac", "Orange Peel", "架空楽団", "PAPER SHEEP");
        // 1文字（LIKE）
        assert_eq!(search(&db, "夜"), vec!["夜明けのスケッチ"]);
        // 2文字（LIKE）
        assert_eq!(search(&db, "灯台"), vec!["灯台"]);
        // 3文字以上（FTS5 trigram）・大文字小文字を問わない
        assert_eq!(search(&db, "nightowl").len(), 2);
        assert_eq!(search(&db, "架空楽"), vec!["Orange Peel"]);
        // アルバムでも引ける
        assert_eq!(search(&db, "SHEEP"), vec!["Orange Peel"]);
        // AND
        assert_eq!(search(&db, "NIGHTOWL 灯台"), vec!["灯台"]);
        // 記号を含む語でも落ちない
        assert!(search(&db, "\"%_").is_empty());
    }

    #[test]
    fn fts_follows_updates() {
        let db = Db::open_memory("search_updates").unwrap();
        add(&db, "C:/1.mp3", "Original Title", "A", "B");
        add(&db, "C:/1.mp3", "Changed Title", "A", "B");
        assert!(search(&db, "Original").is_empty());
        assert_eq!(search(&db, "Changed"), vec!["Changed Title"]);
    }
}
