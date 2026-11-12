-- 歌詞をネットから取る（v3.1.0）：見つからなかった・instrumental だった・大きすぎた曲を覚えておく。
-- 「歌詞の無い曲だけ」で毎回聞き直さないため。「取り直す」と「1曲だけ取る」では聞き直して書き換える。
-- query_sig は送った曲名などの印（lyrics/lrclib.rs の query_sig）。曲名などを直したら印が変わり、聞き直す。
-- 既存の表には手を入れない。曲をライブラリから外すと消える。

CREATE TABLE IF NOT EXISTS lyrics_fetch_misses (
    track_id   INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    result     TEXT NOT NULL CHECK (result IN ('not_found', 'instrumental', 'too_large')),
    query_sig  TEXT NOT NULL,
    checked_at TEXT NOT NULL
);
