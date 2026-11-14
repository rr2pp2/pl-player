//! 設定の形と既定値。DB の settings 表に、キー `app` で JSON として置く。
//! 画面（src/stores/settings.ts）と同じ形。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::db::{self, Db};
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Background {
    /// アプリのデータフォルダへ複製した背景画像のパス
    pub path: Option<String>,
    /// 背景の暗さ（覆いの濃さ）0〜0.95
    pub dim: f32,
    /// ぼかし（px）0〜40
    pub blur: f32,
    /// パネル（UI）の不透明度 0.3〜1
    pub ui_opacity: f32,
    /// cover / contain
    pub fit: String,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            path: None,
            dim: 0.45,
            blur: 6.0,
            ui_opacity: 0.70,
            fit: "cover".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub sidebar_collapsed: bool,
    /// 曲一覧の列（表示するか）。既定のキーは `TRACK_COLUMNS`。無いキーは出す（`sanitize` が true で埋める）
    pub columns: BTreeMap<String, bool>,
    /// v3.0.2：再生バーの下の「流れる歌詞の列」を出すか。既定はオン（古い設定・v2 の書き出しに無ければオン）
    pub lyrics_row: bool,
    /// v3.3.0：再生バーのボタンを出すか（キーは `PLAYER_BUTTONS_HIDEABLE` だけ。無いキーは出す）。
    /// 再生・一時停止・前・次・シークバーは隠せない（キーを持たない。来ても `sanitize` が捨てる）
    pub player_buttons: BTreeMap<String, bool>,
    /// v3.3.0：サイドバーの項目を出すか（キーは `SIDEBAR_ITEMS_HIDEABLE` だけ。無いキーは出す）。設定は隠せない
    pub sidebar_items: BTreeMap<String, bool>,
    /// v3.3.0：曲一覧の行の右端に「ごみ箱へ移す」ボタンを出す。既定はオフ。設定の zip を読み込んだときは、値にかかわらずオフ。
    /// 決めるのは行のボタンだけ（右クリックのメニューには、設定にかかわらず出す。ごみ箱へ移す窓口 commands/library.rs もこの値を見ない）
    pub trash_button: bool,
    /// v3.3.0：設定画面のタブの並び（タブの名前）。空なら既定の順。タブの一覧は画面（src\layout\visibility.ts）が持ち、
    /// 知らない名前を捨て・抜けたタブを足して合わせる。ここでは形だけ整える（名前の形・重なり・数）
    pub settings_tabs: Vec<String>,
    /// v3.4.0：曲一覧のコンパクト表示（`COMPACT_LIST_MODES` のどれか）。既定は "off"（今までの見た目）。
    /// 知らない値・文字でない値は "off" にする（この値だけを既定に戻し、設定全体は既定に戻さない）
    #[serde(deserialize_with = "de_compact_list")]
    pub compact_list: String,
    /// v3.4.0：サイドバーの項目のアイコンの大きさ（px）。`SIDEBAR_ICON_PX_MIN`〜`MAX` の整数。既定は 3.3.0 までの 20。
    /// 範囲の外は端へ丸め、小数は四捨五入する。数でない値は既定（この値だけ）
    #[serde(deserialize_with = "de_sidebar_icon_px")]
    pub sidebar_icon_px: u32,
}

/// v3.4.0：曲一覧のコンパクト表示の値（画面の src/layout/density.ts と同じ並び）。
/// off：使わない（既定）　always：いつも　narrow：ウィンドウが狭いときだけ（境目は画面が持つ。ジャケットが縮む 720px 未満）
pub const COMPACT_LIST_MODES: &[&str] = &["off", "always", "narrow"];

/// v3.4.0：サイドバーのアイコンの大きさ（px）。既定は 3.3.0 までの大きさ（base.css の `.nav-item .icon` の 20px）。
/// 下は、ボタンの中の小さいアイコン（`.icon-btn.small .icon`）の 16px より少し小さい 14px まで（それより小さいと形が読めない）。
/// 上は、たたんだサイドバーの中の幅（64 − 枠 2 − 余白 16 − スクロールバー 10 = 36px）に余白を残して収まる 32px まで
pub const SIDEBAR_ICON_PX_MIN: u32 = 14;
pub const SIDEBAR_ICON_PX_MAX: u32 = 32;
pub const SIDEBAR_ICON_PX_DEFAULT: u32 = 20;

fn compact_list_ok(s: &str) -> String {
    if COMPACT_LIST_MODES.contains(&s) { s.to_string() } else { "off".to_string() }
}

fn clamp_icon_px(v: f64) -> u32 {
    if v.is_finite() {
        v.round().clamp(SIDEBAR_ICON_PX_MIN as f64, SIDEBAR_ICON_PX_MAX as f64) as u32
    } else {
        SIDEBAR_ICON_PX_DEFAULT
    }
}

/// どんな JSON の値でも受け、知らない値は "off" にする（形の違う1つの値で、設定全体を既定に戻さないため）
fn de_compact_list<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(compact_list_ok(v.as_str().unwrap_or("")))
}

/// どんな JSON の値でも受け、数なら丸め、数でなければ既定にする
fn de_sidebar_icon_px<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(v.as_f64().map(clamp_icon_px).unwrap_or(SIDEBAR_ICON_PX_DEFAULT))
}

/// v3.3.0：設定のタブの並びに置ける数と、名前の長さ
pub const SETTINGS_TABS_MAX: usize = 32;
const TAB_NAME_MAX: usize = 32;

fn tab_name_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= TAB_NAME_MAX && s.starts_with(|c: char| c.is_ascii_lowercase()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// v3.3.0：再生バーで隠せるボタン（画面の src/layout/visibility.ts と同じ並び）。
/// 再生・一時停止・前・次・シークバーは、隠すと操作できなくなるので入れない
pub const PLAYER_BUTTONS_HIDEABLE: &[&str] = &["shuffle", "repeat", "volume", "lyricsRow", "lyrics"];

/// v3.3.0：サイドバーで隠せる項目（画面の src/layout/visibility.ts と同じ並び）。設定は戻れなくなるので入れない。
/// v3.6.0：`yearly_top`（年間まとめ）を「最近再生」の次に足した。3.5.0 までの設定・zip には無いので出す（無いキーは出す、の決まりのまま）
pub const SIDEBAR_ITEMS_HIDEABLE: &[&str] =
    &["all", "artist", "album", "genre", "recent_added", "recent_played", "yearly_top", "favorites", "queue", "playlists"];

/// 曲一覧の列のオン・オフ（画面の src\views\SettingsView.tsx の COLS と同じ並び）。
/// v3.5.0：`artist` を足した（作者の要望）。3.4.0 までの設定・zip には無いので、読むときにオンで埋める
pub const TRACK_COLUMNS: &[&str] = &["artist", "album", "genre", "duration", "favorite"];

fn all_shown(keys: &[&str]) -> BTreeMap<String, bool> {
    keys.iter().map(|k| (k.to_string(), true)).collect()
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            sidebar_collapsed: false,
            columns: all_shown(TRACK_COLUMNS),
            lyrics_row: true,
            player_buttons: all_shown(PLAYER_BUTTONS_HIDEABLE),
            sidebar_items: all_shown(SIDEBAR_ITEMS_HIDEABLE),
            trash_button: false,
            settings_tabs: Vec::new(),
            compact_list: "off".into(),
            sidebar_icon_px: SIDEBAR_ICON_PX_DEFAULT,
        }
    }
}

/// 着せ替え（v3.2.0）。古い設定に無ければ既定のスキン・立ち絵は出す・大きさ 100%
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SkinSettings {
    /// 使っているスキン（`builtin:default` か、skins\ の下のフォルダの名前）。変えるのは skins::runtime の適用だけ
    pub active_skin_id: String,
    /// 立ち絵を出すか
    pub character_visible: bool,
    /// 立ち絵の大きさ 0.75〜1.25
    pub character_scale: f32,
    /// 次の起動だけ既定の見た目にする（起動したら消える）
    pub safe_mode_next_start: bool,
}

impl Default for SkinSettings {
    fn default() -> Self {
        Self {
            active_skin_id: crate::skins::names::BUILTIN_ID.into(),
            character_visible: true,
            character_scale: 1.0,
            safe_mode_next_start: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// dark / light（テーマを足すときは src/theme/ にファイルを1つ足し、ここに名前を入れる）
    pub theme: String,
    pub background: Background,
    pub ui: UiSettings,
    /// アプリ内ショートカット（動作 → キー）。原文の10個と、v3 の「歌詞を開く／閉じる」が既定
    pub shortcuts: BTreeMap<String, String>,
    /// グローバルホットキー（動作 → キー）。既定では1つも登録しない
    pub hotkeys: BTreeMap<String, String>,
    /// ×ボタンでタスクトレイへ（false なら完全終了）
    pub close_to_tray: bool,
    /// 起動時にライブラリのフォルダを再走査する
    pub rescan_on_start: bool,
    /// v3.1.0：歌詞をネットから取る（LRCLIB）。既定はオフ。古い設定・v2 の書き出しに無ければオフ。
    /// 設定の zip を読み込んだときは、値にかかわらずオフ（settings::backup::import）。
    /// オフのとき、歌詞を取る窓口（commands/lyrics.rs）は入口で断り、通信の処理にたどり着かない
    pub lyrics_online: bool,
    /// v3.2.0：着せ替え。設定の zip の書き出しには入るが、読み込んでも今の値のまま（スキンは zip に入らないため）
    pub skin: SkinSettings,
}

pub fn default_shortcuts() -> BTreeMap<String, String> {
    [
        ("playPause", "Space"),
        ("next", "Ctrl+ArrowRight"),
        ("previous", "Ctrl+ArrowLeft"),
        ("seekForward", "ArrowRight"),
        ("seekBackward", "ArrowLeft"),
        ("volumeUp", "Ctrl+ArrowUp"),
        ("volumeDown", "Ctrl+ArrowDown"),
        ("mute", "M"),
        ("shuffle", "S"),
        ("repeat", "R"),
        // v3：歌詞画面を開く／閉じる（アプリ内だけ。グローバルホットキーには出さない）
        ("toggleLyrics", "Ctrl+L"),
        // v3.0.2：流れる歌詞の列を出す／隠す（アプリ内だけ）
        ("toggleLyricsRow", "Ctrl+Shift+L"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            background: Background::default(),
            ui: UiSettings::default(),
            shortcuts: default_shortcuts(),
            hotkeys: BTreeMap::new(),
            close_to_tray: true,
            rescan_on_start: false,
            lyrics_online: false,
            skin: SkinSettings::default(),
        }
    }
}

impl AppSettings {
    /// 値の範囲を整える（読み込み・取り込みのとき）
    pub fn sanitize(mut self) -> Self {
        let b = &mut self.background;
        b.dim = b.dim.clamp(0.0, 0.95);
        b.blur = b.blur.clamp(0.0, 40.0);
        b.ui_opacity = b.ui_opacity.clamp(0.3, 1.0);
        if b.fit != "contain" {
            b.fit = "cover".into();
        }
        if self.theme.trim().is_empty() {
            self.theme = "dark".into();
        }
        // 足りないショートカットは既定値で埋める
        for (k, v) in default_shortcuts() {
            self.shortcuts.entry(k).or_insert(v);
        }
        // v3.3.0：隠せるものだけを残す（再生・一時停止・前・次・シークバー・設定などのキーが来ても捨てる）。
        // 無いキーは「出す」として扱う（古い設定・v2 の書き出し）
        self.ui.player_buttons.retain(|k, _| PLAYER_BUTTONS_HIDEABLE.contains(&k.as_str()));
        self.ui.sidebar_items.retain(|k, _| SIDEBAR_ITEMS_HIDEABLE.contains(&k.as_str()));
        // v3.5.0：曲一覧の列。無いキー（3.4.0 までの設定・zip の artist など）はオン。知らないキーは今までどおり残す
        for c in TRACK_COLUMNS {
            self.ui.columns.entry(c.to_string()).or_insert(true);
        }
        // v3.3.0：設定のタブの並び。形の合わない名前・重なりを捨て、数を抑える（どのタブを出すかは画面が決める）
        let mut seen = std::collections::HashSet::new();
        self.ui.settings_tabs.retain(|t| tab_name_ok(t) && seen.insert(t.clone()));
        self.ui.settings_tabs.truncate(SETTINGS_TABS_MAX);
        // v3.4.0：コンパクト表示とサイドバーのアイコンの大きさ（画面から来た値・zip の値も、読むときと同じに整える）
        self.ui.compact_list = compact_list_ok(&self.ui.compact_list);
        self.ui.sidebar_icon_px = clamp_icon_px(self.ui.sidebar_icon_px as f64);
        // v3.2.0：着せ替え
        let sk = &mut self.skin;
        sk.character_scale = if sk.character_scale.is_finite() { sk.character_scale.clamp(0.75, 1.25) } else { 1.0 };
        if sk.active_skin_id != crate::skins::names::BUILTIN_ID && !crate::skins::names::valid_id(&sk.active_skin_id) {
            sk.active_skin_id = crate::skins::names::BUILTIN_ID.into();
        }
        self
    }
}

/// 背景のパスが、データフォルダの背景の置き場（`bg_dir`）の直下を指しているか（v3.1.1。ペネトレーションテスト v3.0.2 の4）。
///
/// 画面から来た設定・書き出しのときに当てる。ファイルには触らず、文字だけで決める
/// （`..`・相対パス・ほかのフォルダ・ネットワーク上の場所は false）。大文字小文字と区切り（`/` と `\`）の違いは同じとみなす。
/// v3.1.1 直し1回目：Windows の予約デバイス名（`CON`・`NUL`・`COM1` など。拡張子つきも）と、末尾が `.` か空白の名前も false
/// （`backgrounds\CON` を書き出しで読むと、ファイルではなく端末を開いて止まりうる）
pub fn background_in_dir(path: &str, bg_dir: &std::path::Path) -> bool {
    use std::path::{Component, Path};
    let p = Path::new(path);
    if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return false;
    }
    let (Some(parent), Some(name)) = (p.parent(), p.file_name()) else {
        return false;
    };
    // v3.1.1 直し2回目（検査2回目の指摘B）：`:` を含む名前は断る（`NUL:` のようなデバイス名と、代替データストリーム `x.png:ads`）
    let name = name.to_string_lossy();
    if name.contains(':') || is_reserved_device_name(&name) {
        return false;
    }
    let norm = |q: &Path| q.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    norm(parent) == norm(bg_dir)
}

/// Windows の予約デバイス名か（CON・PRN・AUX・NUL・CONIN$・CONOUT$・COM0〜9・LPT0〜9。COM¹ などの上付き数字も）。
/// 拡張子がついていても（`NUL.png`）、点の前に空白があっても（`CON .png`）同じ。末尾が `.` か空白の名前も、Windows が
/// 名前を変えて扱うので断る
pub(crate) fn is_reserved_device_name(name: &str) -> bool {
    if name.is_empty() || name.ends_with('.') || name.ends_with(' ') {
        return true;
    }
    let base = name.split(['.', ':']).next().unwrap_or("").trim_end_matches(' ').to_ascii_uppercase();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    match (base.get(..3), base.get(3..)) {
        (Some("COM" | "LPT"), Some(n)) => {
            matches!(n, "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{b9}" | "\u{b2}" | "\u{b3}")
        }
        _ => false,
    }
}

const KEY: &str = "app";

pub fn load(db: &Db) -> AppSettings {
    db::settings::get(db, KEY)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<AppSettings>(v).ok())
        .unwrap_or_default()
        .sanitize()
}

pub fn save(db: &Db, s: &AppSettings) -> AppResult<()> {
    db::settings::set(db, KEY, &serde_json::to_value(s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_partial_json() {
        let s = AppSettings::default();
        assert_eq!(s.shortcuts.len(), 12);
        assert_eq!(s.shortcuts["toggleLyrics"], "Ctrl+L");
        assert_eq!(s.shortcuts["toggleLyricsRow"], "Ctrl+Shift+L");
        assert!(s.ui.lyrics_row);
        assert!(s.hotkeys.is_empty());
        let partial: AppSettings = serde_json::from_str(r#"{"theme":"light","background":{"dim":5}}"#).unwrap();
        let p = partial.sanitize();
        assert_eq!(p.theme, "light");
        assert_eq!(p.background.dim, 0.95);
        assert_eq!(p.shortcuts.len(), 12);
        // 歌詞の列の設定が無い（3.0.1 までの設定・v2 の書き出し）ならオン
        assert!(p.ui.lyrics_row);
        let old_ui: AppSettings = serde_json::from_str(r#"{"ui":{"sidebarCollapsed":true}}"#).unwrap();
        assert!(old_ui.ui.sidebar_collapsed);
        assert!(old_ui.ui.lyrics_row);
        let off: AppSettings = serde_json::from_str(r#"{"ui":{"lyricsRow":false}}"#).unwrap();
        assert!(!off.ui.lyrics_row);
        assert!(serde_json::to_string(&AppSettings::default()).unwrap().contains("\"lyricsRow\":true"));
        // v3.1.0：歌詞をネットから取る。既定はオフ。値の無い古い設定もオフ
        assert!(!s.lyrics_online);
        assert!(!p.lyrics_online);
        assert!(!old_ui.lyrics_online);
        let on: AppSettings = serde_json::from_str(r#"{"lyricsOnline":true}"#).unwrap();
        assert!(on.sanitize().lyrics_online);
        assert!(serde_json::to_string(&AppSettings::default()).unwrap().contains("\"lyricsOnline\":false"));
        // 形が違う値は、設定全体を既定に戻す（今までの読み方と同じ）→ オフ
        assert!(serde_json::from_str::<AppSettings>(r#"{"lyricsOnline":"yes"}"#).is_err());
        // v3.2.0：着せ替え。古い設定には無い → 既定のスキン・立ち絵を出す・100%
        assert_eq!(p.skin, SkinSettings::default());
        assert_eq!(p.skin.active_skin_id, "builtin:default");
        let sk: AppSettings = serde_json::from_str(r#"{"skin":{"activeSkinId":"../x","characterScale":9}}"#).unwrap();
        let sk = sk.sanitize();
        assert_eq!(sk.skin.active_skin_id, "builtin:default");
        assert_eq!(sk.skin.character_scale, 1.25);
        assert!(sk.skin.character_visible);
        let ok: AppSettings = serde_json::from_str(r#"{"skin":{"activeSkinId":"shape-test","characterScale":0.5}}"#).unwrap();
        let ok = ok.sanitize();
        assert_eq!(ok.skin.active_skin_id, "shape-test");
        assert_eq!(ok.skin.character_scale, 0.75);
    }

    /// v3.3.0：再生バーのボタンとサイドバーの項目を隠す設定。古い設定（値が無い）では全部出す。
    /// 隠せないもの（再生・一時停止・前・次・シークバー・設定）は、値が来ても捨てる
    #[test]
    fn hidden_items_default_and_unhideable() {
        let d = AppSettings::default();
        assert_eq!(d.ui.player_buttons.len(), PLAYER_BUTTONS_HIDEABLE.len());
        assert!(d.ui.player_buttons.values().all(|v| *v));
        assert_eq!(d.ui.sidebar_items.len(), SIDEBAR_ITEMS_HIDEABLE.len());
        assert!(d.ui.sidebar_items.values().all(|v| *v));
        assert!(!d.ui.trash_button);
        // 3.2.0 までの設定（ui に新しい値が無い）：全部出す・ごみ箱のボタンは出さない
        let old: AppSettings = serde_json::from_str(r#"{"ui":{"sidebarCollapsed":false,"columns":{"album":true},"lyricsRow":true}}"#).unwrap();
        let old = old.sanitize();
        assert!(old.ui.player_buttons.values().all(|v| *v));
        assert!(old.ui.sidebar_items.values().all(|v| *v));
        assert!(!old.ui.trash_button);
        // 一部だけ書いた設定：書いたものだけが効き、無いキーは捨てられない（出す扱い）
        let some: AppSettings = serde_json::from_str(r#"{"ui":{"playerButtons":{"shuffle":false},"sidebarItems":{"genre":false}}}"#).unwrap();
        let some = some.sanitize();
        assert_eq!(some.ui.player_buttons.get("shuffle"), Some(&false));
        assert_eq!(some.ui.player_buttons.get("repeat"), None);
        assert_eq!(some.ui.sidebar_items.get("genre"), Some(&false));
        // 隠せないもの・知らないキーは捨てる
        let evil: AppSettings = serde_json::from_str(
            r#"{"ui":{"playerButtons":{"playPause":false,"previous":false,"next":false,"seek":false,"seekBar":false,"volume":false},
                "sidebarItems":{"settings":false,"Settings":false,"queue":false,"x":false}}}"#,
        )
        .unwrap();
        let evil = evil.sanitize();
        assert_eq!(evil.ui.player_buttons.keys().collect::<Vec<_>>(), vec!["volume"]);
        assert_eq!(evil.ui.sidebar_items.keys().collect::<Vec<_>>(), vec!["queue"]);
        for k in evil.ui.player_buttons.keys() {
            assert!(PLAYER_BUTTONS_HIDEABLE.contains(&k.as_str()));
        }
        // 画面に送る名前（camelCase）
        let j = serde_json::to_string(&AppSettings::default()).unwrap();
        assert!(j.contains("\"playerButtons\":{") && j.contains("\"sidebarItems\":{") && j.contains("\"trashButton\":false"), "{j}");
    }

    /// v3.6.0：サイドバーの「年間まとめ」（yearly_top）。既定は出す。3.5.0 までの設定（ほかの項目を書いてあっても）には無いので、
    /// 読んでもキーは無いまま（画面は「無いキーは出す」で読む）。隠した値は残る。並びは「最近再生」の次
    #[test]
    fn yearly_top_sidebar_item() {
        assert_eq!(AppSettings::default().ui.sidebar_items.get("yearly_top"), Some(&true));
        let pos = |k: &str| SIDEBAR_ITEMS_HIDEABLE.iter().position(|x| *x == k).unwrap();
        assert_eq!(pos("yearly_top"), pos("recent_played") + 1);
        let old: AppSettings = serde_json::from_str(
            r#"{"ui":{"sidebarItems":{"all":true,"artist":true,"album":true,"genre":false,"recent_added":true,"recent_played":true,"favorites":true,"queue":true,"playlists":true}}}"#,
        )
        .unwrap();
        let old = old.sanitize();
        assert_eq!(old.ui.sidebar_items.get("yearly_top"), None);
        assert_eq!(old.ui.sidebar_items.get("genre"), Some(&false));
        let hidden: AppSettings = serde_json::from_str(r#"{"ui":{"sidebarItems":{"yearly_top":false}}}"#).unwrap();
        assert_eq!(hidden.sanitize().ui.sidebar_items.get("yearly_top"), Some(&false));
    }

    /// v3.3.0：設定のタブの並び。古い設定には無い（空＝既定の順）。形の合わない名前・重なりは捨てる
    #[test]
    fn settings_tabs_shape() {
        assert!(AppSettings::default().ui.settings_tabs.is_empty());
        let old: AppSettings = serde_json::from_str(r#"{"ui":{"sidebarCollapsed":true}}"#).unwrap();
        assert!(old.sanitize().ui.settings_tabs.is_empty());
        let s: AppSettings =
            serde_json::from_str(r#"{"ui":{"settingsTabs":["keys","display","keys","<img>","","Display","future_tab","lyrics"]}}"#).unwrap();
        assert_eq!(s.sanitize().ui.settings_tabs, vec!["keys", "display", "future_tab", "lyrics"]);
        let long = format!(r#"{{"ui":{{"settingsTabs":["{}"]}}}}"#, "a".repeat(33));
        assert!(serde_json::from_str::<AppSettings>(&long).unwrap().sanitize().ui.settings_tabs.is_empty());
        let many: Vec<String> = (0..40).map(|i| format!("t{i}")).collect();
        let mut m = AppSettings::default();
        m.ui.settings_tabs = many;
        assert_eq!(m.sanitize().ui.settings_tabs.len(), SETTINGS_TABS_MAX);
        assert!(serde_json::to_string(&AppSettings::default()).unwrap().contains("\"settingsTabs\":[]"));
    }

    /// v3.4.0：コンパクト表示とサイドバーのアイコンの大きさ。古い設定（値が無い）は既定（使わない・20px）。
    /// 知らない値は "off"、範囲の外は端へ丸める。形の違う値が来ても、その値だけを既定にし、ほかの設定は残す
    #[test]
    fn compact_list_and_sidebar_icon() {
        let d = AppSettings::default();
        assert_eq!(d.ui.compact_list, "off");
        assert_eq!(d.ui.sidebar_icon_px, 20);
        assert_eq!(d.ui.sidebar_icon_px, SIDEBAR_ICON_PX_DEFAULT);
        assert!(SIDEBAR_ICON_PX_MIN < SIDEBAR_ICON_PX_DEFAULT && SIDEBAR_ICON_PX_DEFAULT < SIDEBAR_ICON_PX_MAX);
        // 3.3.0 までの設定（値が無い）
        let old: AppSettings = serde_json::from_str(r#"{"theme":"light","ui":{"sidebarCollapsed":true,"trashButton":true}}"#).unwrap();
        let old = old.sanitize();
        assert_eq!(old.ui.compact_list, "off");
        assert_eq!(old.ui.sidebar_icon_px, 20);
        assert!(old.ui.sidebar_collapsed && old.ui.trash_button);
        // 選べる値はそのまま
        for m in COMPACT_LIST_MODES {
            let s: AppSettings = serde_json::from_str(&format!(r#"{{"ui":{{"compactList":"{m}"}}}}"#)).unwrap();
            assert_eq!(s.sanitize().ui.compact_list, *m);
        }
        // 知らない値・大文字違い・文字でない値は "off"。設定全体は既定に戻さない（theme が残る）
        for bad in [r#""big""#, r#""Always""#, r#""""#, "5", "true", "null", "{}", "[\"always\"]"] {
            let s: AppSettings = serde_json::from_str(&format!(r#"{{"theme":"light","ui":{{"compactList":{bad},"lyricsRow":false}}}}"#))
                .unwrap_or_else(|e| panic!("{bad}: {e}"));
            let s = s.sanitize();
            assert_eq!(s.ui.compact_list, "off", "{bad}");
            assert_eq!(s.theme, "light", "{bad}");
            assert!(!s.ui.lyrics_row, "{bad}");
        }
        // 大きさ：範囲の外は端へ、小数は四捨五入、数でなければ既定
        for (v, want) in [
            ("14", 14), ("32", 32), ("24", 24), ("13", 14), ("0", 14), ("-5", 14), ("33", 32), ("1e9", 32), ("20.4", 20), ("20.6", 21),
            ("31.6", 32), ("\"24\"", 20), ("null", 20), ("true", 20), ("[]", 20), ("{}", 20),
        ] {
            let s: AppSettings = serde_json::from_str(&format!(r#"{{"theme":"light","ui":{{"sidebarIconPx":{v}}}}}"#))
                .unwrap_or_else(|e| panic!("{v}: {e}"));
            let s = s.sanitize();
            assert_eq!(s.ui.sidebar_icon_px, want, "{v}");
            assert_eq!(s.theme, "light", "{v}");
        }
        // 画面から来た値（コードで入れた値）も sanitize で同じに整える
        let mut m = AppSettings::default();
        m.ui.compact_list = "<script>".into();
        m.ui.sidebar_icon_px = 500;
        let m = m.sanitize();
        assert_eq!(m.ui.compact_list, "off");
        assert_eq!(m.ui.sidebar_icon_px, SIDEBAR_ICON_PX_MAX);
        let mut m = AppSettings::default();
        m.ui.sidebar_icon_px = 0;
        assert_eq!(m.sanitize().ui.sidebar_icon_px, SIDEBAR_ICON_PX_MIN);
        // 画面に送る名前（camelCase）
        let j = serde_json::to_string(&AppSettings::default()).unwrap();
        assert!(j.contains("\"compactList\":\"off\"") && j.contains("\"sidebarIconPx\":20"), "{j}");
    }

    /// v3.5.0：曲一覧の列に artist を足した。既定はオン。3.4.0 までの設定（artist のキーが無い・columns が無い）もオンで読み、
    /// 書いてある値（オフを含む）はそのまま。知らないキーは今までどおり残す
    #[test]
    fn track_columns_artist() {
        assert_eq!(TRACK_COLUMNS, &["artist", "album", "genre", "duration", "favorite"]);
        let d = AppSettings::default();
        assert_eq!(d.ui.columns.len(), TRACK_COLUMNS.len());
        assert!(d.ui.columns.values().all(|v| *v));
        assert_eq!(d.ui.columns.get("artist"), Some(&true));
        // 3.4.0 までの設定：artist が無い → オン。ほかの値は書いたまま
        let old: AppSettings =
            serde_json::from_str(r#"{"ui":{"columns":{"album":false,"genre":true,"duration":true,"favorite":false}}}"#).unwrap();
        let old = old.sanitize();
        assert_eq!(old.ui.columns.get("artist"), Some(&true));
        assert_eq!(old.ui.columns.get("album"), Some(&false));
        assert_eq!(old.ui.columns.get("favorite"), Some(&false));
        // 一部しか無い・columns が無い設定は、無いキーを全部オン
        for json in [r#"{"ui":{"columns":{"album":true}}}"#, r#"{"ui":{"columns":{}}}"#, r#"{"ui":{"sidebarCollapsed":true}}"#, "{}"] {
            let s: AppSettings = serde_json::from_str(json).unwrap();
            let s = s.sanitize();
            for c in TRACK_COLUMNS {
                assert_eq!(s.ui.columns.get(*c), Some(&true), "{json}: {c}");
            }
        }
        // 切った値は残る（オンで上書きしない）。知らないキーも残す（今までどおり）
        let off: AppSettings = serde_json::from_str(r#"{"ui":{"columns":{"artist":false,"future":false}}}"#).unwrap();
        let off = off.sanitize();
        assert_eq!(off.ui.columns.get("artist"), Some(&false));
        assert_eq!(off.ui.columns.get("future"), Some(&false));
        assert_eq!(off.ui.columns.get("album"), Some(&true));
        // 画面に送る名前
        let j = serde_json::to_string(&AppSettings::default()).unwrap();
        assert!(j.contains("\"columns\":{\"album\":true,\"artist\":true,"), "{j}");
    }

    #[test]
    fn roundtrip_in_db() {
        let db = Db::open_memory("settings_rt").unwrap();
        let mut s = AppSettings::default();
        s.theme = "light".into();
        s.hotkeys.insert("playPause".into(), "Ctrl+Alt+P".into());
        s.ui.lyrics_row = false;
        s.lyrics_online = true;
        s.ui.player_buttons.insert("lyrics".into(), false);
        s.ui.sidebar_items.insert("playlists".into(), false);
        s.ui.trash_button = true;
        s.ui.settings_tabs = vec!["keys".into(), "display".into()];
        s.ui.compact_list = "narrow".into();
        s.ui.sidebar_icon_px = 28;
        s.ui.columns.insert("artist".into(), false);
        save(&db, &s).unwrap();
        assert_eq!(load(&db), s);
        assert_eq!(load(&db).ui.columns.get("artist"), Some(&false));
        // v3.4.0：DB に範囲の外・知らない値が入っていても、読むときに整える（ほかの値は残す）
        let mut raw = serde_json::to_value(&s).unwrap();
        raw["ui"]["compactList"] = serde_json::json!("tiny");
        raw["ui"]["sidebarIconPx"] = serde_json::json!(99);
        // v3.5.0：3.4.0 までに保存した設定（columns に artist が無い）は、読むときにオン
        raw["ui"]["columns"].as_object_mut().unwrap().remove("artist");
        db::settings::set(&db, KEY, &raw).unwrap();
        let back = load(&db);
        assert_eq!(back.ui.columns.get("artist"), Some(&true));
        assert_eq!(back.ui.compact_list, "off");
        assert_eq!(back.ui.sidebar_icon_px, SIDEBAR_ICON_PX_MAX);
        assert_eq!(back.theme, "light");
        assert_eq!(back.ui.settings_tabs, s.ui.settings_tabs);
    }

    /// v3.1.1（ペネトレーションテスト v3.0.2 の4）：背景のパスは backgrounds の直下だけ
    #[test]
    fn background_must_be_inside_backgrounds() {
        let bg = std::path::Path::new(r"C:\Users\x\AppData\Roaming\jp.local.plplayer3\backgrounds");
        assert!(background_in_dir(r"C:\Users\x\AppData\Roaming\jp.local.plplayer3\backgrounds\bg-0123.png", bg));
        assert!(background_in_dir("c:/users/x/appdata/roaming/jp.local.plplayer3/backgrounds/bg-0123.png", bg));
        for bad in [
            r"C:\Users\x\.ssh\id_ed25519",
            r"C:\Users\x\AppData\Roaming\jp.local.plplayer3\library.db",
            r"C:\Users\x\AppData\Roaming\jp.local.plplayer3\backgrounds\..\library.db",
            r"C:\Users\x\AppData\Roaming\jp.local.plplayer3\backgrounds\sub\bg.png",
            r"C:\Users\x\AppData\Roaming\jp.local.plplayer3\backgrounds",
            r"backgrounds\bg.png",
            r"\\pl-player-test.invalid\share\bg.png",
            "",
        ] {
            assert!(!background_in_dir(bad, bg), "{bad}");
        }
    }

    /// v3.1.1 直し1回目：予約デバイス名の背景は受けない（書き出しで端末を開かない）
    #[test]
    fn background_rejects_reserved_device_names() {
        let bg = std::path::Path::new(r"C:\data\backgrounds");
        for bad in [
            "CON", "con", "NUL.png", "PRN.jpg", "AUX", "COM1", "com9.webp", "LPT1.png", "lpt0", "CONIN$", "CONOUT$.png",
            "CON .png", "COM\u{b9}.png", "bg.png.", "bg.png ",
        ] {
            assert!(is_reserved_device_name(bad), "{bad}");
            assert!(!background_in_dir(&format!(r"C:\data\backgrounds\{bad}"), bg), "{bad}");
        }
        // v3.1.1 直し2回目（検査2回目の指摘B）：`:` を含む名前（`NUL:`・`CON:`・代替データストリーム）
        for bad in ["NUL:", "CON:", "con:.png", "COM1:", "bg-0123.png:ads", "bg.png:Zone.Identifier:$DATA", "x:y"] {
            assert!(!background_in_dir(&format!(r"C:\data\backgrounds\{bad}"), bg), "{bad}");
        }
        assert!(is_reserved_device_name("NUL:") && is_reserved_device_name("CON:"));
        for ok in ["bg-0123456789abcdef.png", "console.png", "COM10.png", "CONX", "nul-bg.png", "LPT.png", "COMPANY.png", "com.png", "CONSOLE.png", "bg-0123.png"] {
            assert!(!is_reserved_device_name(ok), "{ok}");
            assert!(background_in_dir(&format!(r"C:\data\backgrounds\{ok}"), bg), "{ok}");
        }
    }
}
