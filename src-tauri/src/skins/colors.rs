//! 手で選ぶ色（v3.10.0）。設定の `skin.colors`（色の名前 → 値）に置く。
//!
//! - 名前はスキンで変えられる色と同じ 15 個（`tokens::COLOR_VARS`。新しい表は作らない）
//! - 値の形もスキンの色と同じ確かめ（`tokens::color_token`）を通し、形をそろえて置く。任意の CSS の文字列は受けない
//! - Dark・Light 共通の1組。重なり順は テーマの既定 < スキンのそのテーマの色 < 手の色（当てるのは画面。src\skins\tokens.ts）
//! - 変えるのは「色」の欄の窓口（`skins_color_set`・`skins_colors_clear`）と「既定の見た目に戻す」だけ。
//!   画面から来た設定の保存（`settings_set`）では変わらない（commands::settings::from_screen）
//! - 設定のバックアップ（zip）に入る。zip の settings.json に項目が無ければ、この PC の手の色を変えない（settings::backup）

use std::collections::BTreeMap;

use super::tokens::{color_token, COLOR_VARS};
use crate::error::{AppError, AppResult, ErrorKind};

/// 手で選んだ色（色の名前 → 形をそろえた値）
pub type Colors = BTreeMap<String, String>;

/// 断るときの知らせに入れる文字の長さ（名前と値は画面から来るので、長い文字をそのまま知らせに出さない）
const SHOWN_MAX: usize = 64;

fn shown(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(SHOWN_MAX).collect();
    if s.chars().count() > SHOWN_MAX {
        format!("{t}…")
    } else {
        t
    }
}

/// 1つの色を確かめて形をそろえる。名前が 15 個のどれでもない・値の形が違えば None
pub fn check(name: &str, value: &str) -> Option<String> {
    if !COLOR_VARS.contains(&name) {
        return None;
    }
    color_token(name, value)
}

/// 手の色の全部を整える（読み込み・設定の保存・zip の読み込みのとき）：知らない名前と形の違う値を捨て、値の形をそろえる
pub fn normalize(c: &Colors) -> Colors {
    c.iter().filter_map(|(k, v)| check(k, v).map(|ok| (k.clone(), ok))).collect()
}

/// JSON の値から読む。オブジェクトでなければ空、文字でない値は捨てる（形の違う1つの値で、設定全体を既定に戻さないため）。
/// 名前と値の確かめは `normalize`（`AppSettings::sanitize` が呼ぶ）
pub fn from_json(v: &serde_json::Value) -> Colors {
    v.as_object()
        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default()
}

/// 1つの色を変える（`None` なら外す＝スキンの色か既定の色に戻る）。名前か値が決まりに合わなければ、何も変えずに断る
pub fn set(c: &mut Colors, name: &str, value: Option<&str>) -> AppResult<()> {
    if !COLOR_VARS.contains(&name) {
        return Err(AppError::new(ErrorKind::SkinColorInvalid, format!("unknown color: {}", shown(name))));
    }
    match value {
        None => {
            c.remove(name);
        }
        Some(v) => {
            let ok = check(name, v)
                .ok_or_else(|| AppError::new(ErrorKind::SkinColorInvalid, format!("{}: {}", name, shown(v))))?;
            c.insert(name.to_string(), ok);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 名前は 15 個（スキンの色と同じ表）だけ。値はスキンの色と同じ確かめを通し、形をそろえる
    #[test]
    fn names_and_values_are_checked_like_skin_colors() {
        assert_eq!(check("--c-bg", "#ABC").as_deref(), Some("#aabbcc"));
        assert_eq!(check("--c-hover", "rgba(255,255,255,.5)").as_deref(), Some("rgba(255, 255, 255, 0.5)"));
        assert_eq!(check("--c-accent", "rgb(1, 2, 3)").as_deref(), Some("#010203"));
        assert_eq!(check("--c-panel-rgb", "1,2,3").as_deref(), Some("1, 2, 3"));
        // *-rgb は r, g, b だけ。ほかの色に r, g, b は書けない
        assert_eq!(check("--c-panel-rgb", "#000000"), None);
        assert_eq!(check("--c-bg", "1, 2, 3"), None);
        // 任意の CSS の文字列・知らない名前・寸法の変数は受けない
        for (n, v) in [
            ("--c-bg", "red"),
            ("--c-bg", "#000; background: url(x)"),
            ("--c-bg", "var(--c-primary)"),
            ("--c-bg", "url(https://example.invalid/x.png)"),
            ("--c-bg", ""),
            ("--c-whatever", "#000000"),
            ("--row-h", "#000000"),
            ("--ui-alpha", "#000000"),
            ("c-bg", "#000000"),
        ] {
            assert_eq!(check(n, v), None, "{n} {v}");
        }
        for v in COLOR_VARS {
            let sample = if v.ends_with("-rgb") { "10, 20, 30" } else { "#102030" };
            assert!(check(v, sample).is_some(), "{v}");
        }
    }

    /// 整えるとき：知らない名前・形の違う値は捨て、ほかは残して形をそろえる
    #[test]
    fn normalize_drops_bad_entries_and_keeps_the_rest() {
        let mut c = Colors::new();
        c.insert("--c-bg".into(), "#FFF".into());
        c.insert("--c-border".into(), "rgba(0,0,0,0.3)".into());
        c.insert("--c-primary".into(), "white".into());
        c.insert("--c-unknown".into(), "#000000".into());
        c.insert("--row-h".into(), "80px".into());
        let n = normalize(&c);
        assert_eq!(n.len(), 2);
        assert_eq!(n["--c-bg"], "#ffffff");
        assert_eq!(n["--c-border"], "rgba(0, 0, 0, 0.3)");
        // 何度通しても同じ
        assert_eq!(normalize(&n), n);
    }

    /// JSON から読む：オブジェクトでなければ空・文字でない値は捨てる
    #[test]
    fn from_json_is_lenient() {
        assert!(from_json(&json!(null)).is_empty());
        assert!(from_json(&json!("x")).is_empty());
        assert!(from_json(&json!([1, 2])).is_empty());
        let c = from_json(&json!({"--c-bg": "#000000", "--c-primary": 5, "--c-accent": null, "--c-ok": "#00ff00"}));
        assert_eq!(c.len(), 2);
        assert_eq!(c["--c-ok"], "#00ff00");
    }

    /// 「戻す」の単位（docs\着せ替え_仕様.md の v3.10.0 の節の表）：スキンを替えても手の色は残る・行の「戻す」はその1色・
    /// 全部戻すは全部・「既定の見た目に戻す」は手の色も外してスキンを既定に（立ち絵は残す）
    #[test]
    fn what_each_operation_keeps() {
        use crate::settings::store::AppSettings;
        use crate::skins::runtime::{reset_look_settings, with_active};
        let mut s = AppSettings::default();
        s.skin.active_skin_id = "quiet-paper".into();
        s.skin.set_avatar("avatar:0123456789abcdef");
        s.skin.character_scale = 1.5;
        set(&mut s.skin.colors, "--c-primary", Some("#ff0000")).unwrap();
        set(&mut s.skin.colors, "--c-bg", Some("#000000")).unwrap();
        // スキンを替える：手の色・立ち絵はそのまま
        let t = with_active(s.clone(), "other-skin");
        assert_eq!(t.skin.active_skin_id, "other-skin");
        assert_eq!(t.skin.colors, s.skin.colors);
        assert_eq!(t.skin.avatar, s.skin.avatar);
        let t = with_active(s.clone(), crate::skins::names::BUILTIN_ID);
        assert_eq!(t.skin.colors, s.skin.colors);
        // 行の「戻す」：その1色だけ
        let mut one = s.skin.colors.clone();
        set(&mut one, "--c-primary", None).unwrap();
        assert_eq!(one.keys().collect::<Vec<_>>(), vec!["--c-bg"]);
        // 「既定の見た目に戻す」：手の色も全部外し、スキンは既定。立ち絵の選択と大きさは残す
        let r = reset_look_settings(s.clone());
        assert!(r.skin.colors.is_empty());
        assert_eq!(r.skin.active_skin_id, crate::skins::names::BUILTIN_ID);
        assert_eq!(r.skin.avatar, "avatar:0123456789abcdef");
        assert!((r.skin.character_scale - 1.5).abs() < 1e-6);
        assert_eq!(r.theme, s.theme);
    }

    /// 1つ変える・外す。決まりに合わなければ何も変えずに断る（知らせの文は長さを切る）
    #[test]
    fn set_and_remove() {
        let mut c = Colors::new();
        set(&mut c, "--c-accent", Some("#ABCDEF")).unwrap();
        set(&mut c, "--c-overlay-rgb", Some(" 4 , 5 , 6 ")).unwrap();
        assert_eq!(c["--c-accent"], "#abcdef");
        assert_eq!(c["--c-overlay-rgb"], "4, 5, 6");
        let before = c.clone();
        for (n, v) in [("--c-accent", "expression(alert(1))"), ("--c-nope", "#000000"), ("--c-overlay-rgb", "#000000")] {
            let e = set(&mut c, n, Some(v)).unwrap_err();
            assert_eq!(e.kind, ErrorKind::SkinColorInvalid, "{n}");
            assert_eq!(c, before, "{n}");
        }
        let long = "x".repeat(10_000);
        let e = set(&mut c, &long, Some("#000000")).unwrap_err();
        assert!(e.detail.chars().count() < 100, "{}", e.detail.len());
        set(&mut c, "--c-accent", None).unwrap();
        assert!(!c.contains_key("--c-accent"));
        // 無い色を外しても断らない
        set(&mut c, "--c-accent", None).unwrap();
        assert_eq!(c.len(), 1);
    }
}
