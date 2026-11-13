-- 編集画面で手で変えたタグの項目の印（v3.5.0）。
-- 設定の「古い文字コードのタグを読み直す…」（library::legacy_reread）は、印の付いた項目を書き換えない。
-- fields はビット：曲名 1・アーティスト 2・アルバム 4・アルバムアーティスト 8・ジャンル 16・作曲者 32・コメント 64
-- （db::tracks::LegacyFields の並び）。3.4.0 までに編集画面で保存した曲には印が無い（README の既知の制限）。
-- 曲をライブラリから外すと、印も一緒に消える。
CREATE TABLE IF NOT EXISTS tag_manual_edits (
    track_id    INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    fields      INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL
);
