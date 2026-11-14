-- 再生の記録（v3.6.0）。「年間まとめ」（db::history::yearly_top）が、1年の間に再生した回数を数えるのに使う。
-- 1回ずつ、曲と再生を始めた時刻を貯める。書くのは tracks.play_count を +1 するのと同じ所（db::tracks::mark_played）だけ。
-- played_at は tracks.last_played_at と同じ表し方（1970-01-01 UTC からのミリ秒。db::now_ms）。
-- 3.5.0 までの再生は記録が無い（tracks.play_count から作り出さない。時刻が無いので嘘になる）。
-- 曲をライブラリから外すと、記録も一緒に消える（ON DELETE CASCADE。3.5.0 の版で外しても、表の決まりなので同じ）。
-- 古い記録は消さない（理由は README の「変更点（v3.6.0）」）。
CREATE TABLE IF NOT EXISTS play_history (
    id         INTEGER PRIMARY KEY,
    track_id   INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    played_at  INTEGER NOT NULL
);
-- 期間で絞って曲ごとに数える（WHERE played_at の範囲 → GROUP BY track_id）を、表を読まずに索引だけで済ませる
CREATE INDEX IF NOT EXISTS idx_play_history_played_at ON play_history(played_at, track_id);
-- 曲を外すときの ON DELETE CASCADE が、その曲の記録を表の全部を読まずに見つけるため
CREATE INDEX IF NOT EXISTS idx_play_history_track ON play_history(track_id);

-- 数え始めた時刻（この移行が走った時刻。1行だけ）。画面の「3.6.0 から数え始めた」の知らせに使う
-- （記録のいちばん古い時刻では、その曲を外すと後ろへずれ、「再生していない日」と「数えていない日」を分けられない）
CREATE TABLE IF NOT EXISTS play_history_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    started_at  INTEGER NOT NULL
);
INSERT OR IGNORE INTO play_history_meta (id, started_at)
    VALUES (1, CAST((julianday('now') - 2440587.5) * 86400000.0 AS INTEGER));
