-- PL Player 初期スキーマ（アーキテクチャ案 5章）
-- 音声ファイルはコピーしない。パスだけを持つ。

CREATE TABLE IF NOT EXISTS library_folders (
    id            INTEGER PRIMARY KEY,
    path          TEXT NOT NULL UNIQUE,
    recursive     INTEGER NOT NULL DEFAULT 1,
    added_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tracks (
    id             INTEGER PRIMARY KEY,
    path           TEXT NOT NULL UNIQUE,
    file_size      INTEGER NOT NULL DEFAULT 0,
    file_mtime     INTEGER NOT NULL DEFAULT 0,
    format         TEXT NOT NULL DEFAULT '',
    duration_ms    INTEGER NOT NULL DEFAULT 0,
    title          TEXT NOT NULL DEFAULT '',
    artist         TEXT NOT NULL DEFAULT '',
    album          TEXT NOT NULL DEFAULT '',
    album_artist   TEXT NOT NULL DEFAULT '',
    genre          TEXT NOT NULL DEFAULT '',
    year           INTEGER,
    track_no       INTEGER,
    disc_no        INTEGER,
    composer       TEXT NOT NULL DEFAULT '',
    comment        TEXT NOT NULL DEFAULT '',
    has_art        INTEGER NOT NULL DEFAULT 0,
    art_key        TEXT,
    album_key      TEXT NOT NULL DEFAULT '',
    added_at       INTEGER NOT NULL,
    last_played_at INTEGER,
    play_count     INTEGER NOT NULL DEFAULT 0,
    favorite       INTEGER NOT NULL DEFAULT 0,
    missing        INTEGER NOT NULL DEFAULT 0,
    tag_error      INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_tracks_artist      ON tracks(artist COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_tracks_album       ON tracks(album COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_tracks_album_key   ON tracks(album_key);
CREATE INDEX IF NOT EXISTS idx_tracks_genre       ON tracks(genre COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_tracks_added_at    ON tracks(added_at);
CREATE INDEX IF NOT EXISTS idx_tracks_last_played ON tracks(last_played_at);
CREATE INDEX IF NOT EXISTS idx_tracks_favorite    ON tracks(favorite);

-- 曲名・アーティスト・アルバムの全文検索（trigram）。3文字以上はこちらで引く。
CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
    title, artist, album,
    content='tracks', content_rowid='id',
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, title, artist, album) VALUES (new.id, new.title, new.artist, new.album);
END;
CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album) VALUES ('delete', old.id, old.title, old.artist, old.album);
END;
CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE OF title, artist, album ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album) VALUES ('delete', old.id, old.title, old.artist, old.album);
    INSERT INTO tracks_fts(rowid, title, artist, album) VALUES (new.id, new.title, new.artist, new.album);
END;

CREATE TABLE IF NOT EXISTS playlists (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS playlist_items (
    id           INTEGER PRIMARY KEY,
    playlist_id  INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position     INTEGER NOT NULL,
    track_id     INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_playlist_items ON playlist_items(playlist_id, position);

CREATE TABLE IF NOT EXISTS queue_items (
    id        INTEGER PRIMARY KEY,
    position  INTEGER NOT NULL,
    track_id  INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE
);

-- 1行だけ（id = 1）
CREATE TABLE IF NOT EXISTS playback_state (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    track_id     INTEGER,
    position_ms  INTEGER NOT NULL DEFAULT 0,
    volume       REAL NOT NULL DEFAULT 0.8,
    muted        INTEGER NOT NULL DEFAULT 0,
    shuffle      INTEGER NOT NULL DEFAULT 0,
    repeat_mode  TEXT NOT NULL DEFAULT 'off',
    device_id    TEXT,
    context_json TEXT NOT NULL DEFAULT '[]',
    context_index INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO playback_state (id) VALUES (1);

CREATE TABLE IF NOT EXISTS settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS album_art_overrides (
    album_key   TEXT PRIMARY KEY,
    image_path  TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);
