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
    /// 曲一覧の列（表示するか）
    pub columns: BTreeMap<String, bool>,
    /// v3.0.2：再生バーの下の「流れる歌詞の列」を出すか。既定はオン（古い設定・v2 の書き出しに無ければオン）
    pub lyrics_row: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        let mut columns = BTreeMap::new();
        for c in ["album", "genre", "duration", "favorite"] {
            columns.insert(c.to_string(), true);
        }
        Self {
            sidebar_collapsed: false,
            columns,
            lyrics_row: true,
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

    #[test]
    fn roundtrip_in_db() {
        let db = Db::open_memory("settings_rt").unwrap();
        let mut s = AppSettings::default();
        s.theme = "light".into();
        s.hotkeys.insert("playPause".into(), "Ctrl+Alt+P".into());
        s.ui.lyrics_row = false;
        s.lyrics_online = true;
        save(&db, &s).unwrap();
        assert_eq!(load(&db), s);
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
