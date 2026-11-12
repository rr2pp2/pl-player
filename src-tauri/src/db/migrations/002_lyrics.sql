-- 歌詞（v3、仕様 2.2）。既存の表・FTS5・トリガには手を入れない。
-- manual：歌詞の編集画面で保存したもの／external：受け渡しフォルダから取り込んだもの。
-- 曲ファイルの歌詞タグ（embedded_tag）は保存しない（表示のたびに読む）。

CREATE TABLE IF NOT EXISTS lyrics_sources (
    id          INTEGER PRIMARY KEY,
    track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('manual', 'external')),
    raw_text    TEXT NOT NULL,
    format      TEXT NOT NULL CHECK (format IN ('timed', 'plain')),
    updated_at  TEXT NOT NULL,
    UNIQUE (track_id, source_kind)
);

CREATE TABLE IF NOT EXISTS track_lyrics_settings (
    track_id      INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    user_delay_ms INTEGER NOT NULL DEFAULT 0,
    updated_at    TEXT NOT NULL
);
