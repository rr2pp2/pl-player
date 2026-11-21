//! 色と文字の値の確かめ（v3.2.0）。スキンが変えてよい CSS 変数と、その値の形だけを受ける。
//! 画面側（src\skins\tokens.ts）も、当てる前に同じ決まりで確かめ直す（二重の守り）。
//! 値は形をそろえて返す（`#abc` → `#aabbcc`、`rgb(…)` → `#rrggbb`、`rgba(…)` → `rgba(r, g, b, a)`、`r,g,b` → `r, g, b`）。

/// テーマごとに変えてよい色（src\theme\dark.css・light.css と同じ名前）。
/// v3.7.0 で `--c-outline`（操作部の輪郭）と `--c-favorite`（お気に入りのオンの星）を足した（13 → 15）。
/// 3.6.0 までの版は、この2つを知らない名前として警告して使わない（取り込みは断らない）
pub const COLOR_VARS: [&str; 15] = [
    "--c-bg",
    "--c-panel-rgb",
    "--c-overlay-rgb",
    "--c-primary",
    "--c-secondary",
    "--c-accent",
    "--c-accent-contrast",
    "--c-hover",
    "--c-selected",
    "--c-border",
    "--c-outline",
    "--c-input",
    "--c-danger",
    "--c-ok",
    "--c-favorite",
];

/// 文字の大きさ（既定値 px）。スキンは既定から ±2px まで（行の高さ 60px は変えないので、大きくしすぎるとはみ出す。v3.7.0 で 64 → 60px）
pub const FONT_SIZE_VARS: [(&str, u32); 6] = [
    ("--fs-title", 17),
    ("--fs-title-list", 16),
    ("--fs-artist", 13),
    ("--fs-body", 14),
    ("--fs-settings", 14),
    ("--fs-small", 12),
];
pub const FONT_SIZE_RANGE: u32 = 2;

/// 角丸（0〜24px）
pub const RADIUS_VARS: [&str; 3] = ["--radius", "--radius-card", "--radius-small"];
pub const RADIUS_MAX: u32 = 24;

/// 動きの時間（100〜250ms）
pub const DURATION_VARS: [&str; 4] = ["--dur-hover", "--dur-panel", "--dur-page", "--dur-bg"];
pub const DURATION_MIN: u32 = 100;
pub const DURATION_MAX: u32 = 250;

/// スキンから変えさせない（レイアウトの寸法と、使う人の背景の設定）。書いてあれば警告して使わない。
/// v3.8.0：再生バーの真ん中の列の幅 `--player-center-w` を足した（画面の src\skins\tokens.ts の LOCKED_VARS と同じ並び。
/// 3列と1段の切り替えの計算（src\layout\playerLayout.ts）がこの幅を前提にするため）。
/// v3.8.0：出力ボタン・出力のプルダウン・ミキサー・メーターの寸法（`--player-output-w` `--output-popup-w` `--mixer-w`
/// `--mixer-strip-min-w` `--meter-h`）を足した（再生バーの3列・1段の計算と、詳細の左右・上下の計算 src\layout\detailLayout.ts が前提にするため）
pub const LOCKED_VARS: [&str; 17] = [
    "--sidebar-w",
    "--sidebar-w-collapsed",
    "--row-h",
    "--row-art",
    "--player-h",
    "--player-center-w",
    "--player-output-w",
    "--output-popup-w",
    "--mixer-w",
    "--mixer-strip-min-w",
    "--meter-h",
    "--topbar-h",
    "--lyrics-row-h",
    "--lyrics-row-anchor",
    "--bg-dim",
    "--bg-blur",
    "--ui-alpha",
];

/// 影の段階（テーマごと）
pub const SHADOW_PRESETS: [&str; 3] = ["none", "soft", "standard"];

/// `--ease` に使える名前
pub const EASE_PRESETS: [&str; 3] = ["standard", "linear", "ease-out"];

fn byte(s: &str) -> Option<u8> {
    let s = s.trim();
    if s.is_empty() || s.len() > 3 || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse::<u16>().ok().filter(|v| *v <= 255).map(|v| v as u8)
}

fn alpha(s: &str) -> Option<String> {
    let s = s.trim();
    // 0・1・0.5・.5 の形（小数は3桁まで）
    let ok = match s.split_once('.') {
        None => s == "0" || s == "1",
        Some((i, f)) => (i.is_empty() || i == "0" || i == "1") && !f.is_empty() && f.len() <= 3 && f.bytes().all(|c| c.is_ascii_digit()),
    };
    if !ok {
        return None;
    }
    let v: f64 = if s.starts_with('.') { format!("0{s}") } else { s.to_string() }.parse().ok()?;
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    Some(format!("{}", (v * 1000.0).round() / 1000.0))
}

fn triple(s: &str) -> Option<[u8; 3]> {
    let p: Vec<&str> = s.split(',').collect();
    if p.len() != 3 {
        return None;
    }
    Some([byte(p[0])?, byte(p[1])?, byte(p[2])?])
}

/// 色の値（`#rgb`・`#rrggbb`・`rgb(r, g, b)`・`rgba(r, g, b, a)`）。形をそろえて返す
pub fn color(v: &str) -> Option<String> {
    let t = v.trim();
    if let Some(h) = t.strip_prefix('#') {
        if !h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let full = match h.len() {
            3 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 => h.to_string(),
            _ => return None,
        };
        return Some(format!("#{}", full.to_ascii_lowercase()));
    }
    if let Some(inner) = t.strip_prefix("rgba(").and_then(|x| x.strip_suffix(')')) {
        let p: Vec<&str> = inner.split(',').collect();
        if p.len() != 4 {
            return None;
        }
        let [r, g, b] = triple(&p[..3].join(","))?;
        return Some(format!("rgba({r}, {g}, {b}, {})", alpha(p[3])?));
    }
    if let Some(inner) = t.strip_prefix("rgb(").and_then(|x| x.strip_suffix(')')) {
        let [r, g, b] = triple(inner)?;
        return Some(format!("#{r:02x}{g:02x}{b:02x}"));
    }
    None
}

/// `--c-panel-rgb`・`--c-overlay-rgb` の形（`r, g, b`。不透明度は別の変数で決まる）
pub fn rgb_triple(v: &str) -> Option<String> {
    let [r, g, b] = triple(v)?;
    Some(format!("{r}, {g}, {b}"))
}

/// テーマの色の1つを確かめる
pub fn color_token(name: &str, v: &str) -> Option<String> {
    if name.ends_with("-rgb") {
        rgb_triple(v)
    } else {
        color(v)
    }
}

/// 数と単位（`14`・`"14px"`）。範囲の外なら None
fn number_with_unit(v: &serde_json::Value, unit: &str, min: u32, max: u32) -> Option<String> {
    let n = match v {
        serde_json::Value::Number(n) => n.as_u64()?,
        serde_json::Value::String(s) => {
            let d = s.trim().strip_suffix(unit)?;
            if d.is_empty() || d.len() > 4 || !d.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            d.parse().ok()?
        }
        _ => return None,
    };
    (n >= min as u64 && n <= max as u64).then(|| format!("{n}{unit}"))
}

/// どの変数でもない（色はテーマの中、ほかは共通の tokens の中）
#[derive(Debug, PartialEq, Eq)]
pub enum CommonToken {
    Ok(String),
    /// 値の形・範囲が違う（断る）
    BadValue,
    /// 寸法など、スキンから変えさせないもの（警告して使わない）
    Locked,
    /// 知らない名前（警告して使わない）
    Unknown,
}

/// 共通の tokens（文字の大きさ・角丸・動き）を1つ確かめる
pub fn common_token(name: &str, v: &serde_json::Value) -> CommonToken {
    if LOCKED_VARS.contains(&name) {
        return CommonToken::Locked;
    }
    let r = if let Some((_, def)) = FONT_SIZE_VARS.iter().find(|(n, _)| *n == name) {
        number_with_unit(v, "px", def - FONT_SIZE_RANGE, def + FONT_SIZE_RANGE)
    } else if RADIUS_VARS.contains(&name) {
        number_with_unit(v, "px", 0, RADIUS_MAX)
    } else if DURATION_VARS.contains(&name) {
        number_with_unit(v, "ms", DURATION_MIN, DURATION_MAX)
    } else if name == "--ease" {
        v.as_str().filter(|s| EASE_PRESETS.contains(s)).map(|s| s.to_string())
    } else {
        return CommonToken::Unknown;
    };
    r.map(CommonToken::Ok).unwrap_or(CommonToken::BadValue)
}

/// フォントの名前（PC にあるものの名前だけ）：文字・数字・空白・`.` `_` `-`、1〜64 文字。前後の空白は不可
pub fn font_name(s: &str) -> bool {
    let n = s.chars().count();
    (1..=64).contains(&n)
        && s.trim() == s
        && s.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn colors() {
        assert_eq!(color("#ABC").as_deref(), Some("#aabbcc"));
        assert_eq!(color("#7c6cff").as_deref(), Some("#7c6cff"));
        assert_eq!(color(" rgb(1, 2, 255) ").as_deref(), Some("#0102ff"));
        assert_eq!(color("rgba(255,255,255,0.07)").as_deref(), Some("rgba(255, 255, 255, 0.07)"));
        assert_eq!(color("rgba(0, 0, 0, .5)").as_deref(), Some("rgba(0, 0, 0, 0.5)"));
        assert_eq!(color("rgba(0, 0, 0, 1)").as_deref(), Some("rgba(0, 0, 0, 1)"));
        for bad in [
            "red",
            "#12345",
            "#1234567",
            "#ggg",
            "rgb(256, 0, 0)",
            "rgb(-1, 0, 0)",
            "rgba(0, 0, 0, 1.5)",
            "rgba(0, 0, 0, 0.1234)",
            "rgb(0, 0, 0); background: url(x)",
            "url(https://example.invalid/x.png)",
            "var(--c-bg)",
            "rgba(0,0,0)",
            "",
        ] {
            assert_eq!(color(bad), None, "{bad}");
        }
        assert_eq!(color_token("--c-panel-rgb", "15,15,22").as_deref(), Some("15, 15, 22"));
        assert_eq!(color_token("--c-panel-rgb", "#000000"), None);
        assert_eq!(color_token("--c-bg", "15, 15, 22"), None);
    }

    #[test]
    fn common_tokens() {
        assert_eq!(common_token("--fs-body", &json!(15)), CommonToken::Ok("15px".into()));
        assert_eq!(common_token("--fs-body", &json!("16px")), CommonToken::Ok("16px".into()));
        assert_eq!(common_token("--fs-body", &json!("17px")), CommonToken::BadValue);
        assert_eq!(common_token("--fs-title", &json!(28)), CommonToken::BadValue);
        assert_eq!(common_token("--fs-small", &json!(10)), CommonToken::Ok("10px".into()));
        assert_eq!(common_token("--fs-small", &json!("12em")), CommonToken::BadValue);
        assert_eq!(common_token("--radius", &json!(0)), CommonToken::Ok("0px".into()));
        assert_eq!(common_token("--radius", &json!(25)), CommonToken::BadValue);
        assert_eq!(common_token("--dur-hover", &json!("150ms")), CommonToken::Ok("150ms".into()));
        assert_eq!(common_token("--dur-hover", &json!(50)), CommonToken::BadValue);
        assert_eq!(common_token("--ease", &json!("linear")), CommonToken::Ok("linear".into()));
        assert_eq!(common_token("--ease", &json!("cubic-bezier(0,0,0,0)")), CommonToken::BadValue);
        assert_eq!(common_token("--row-h", &json!(80)), CommonToken::Locked);
        assert_eq!(common_token("--ui-alpha", &json!(1)), CommonToken::Locked);
        assert_eq!(common_token("--whatever", &json!(1)), CommonToken::Unknown);
    }

    /// v3.7.0：スキンから変えられる色は 15 個。名前は画面のテーマ（dark.css・light.css）の `--c-*` と同じ（足し忘れ・消し忘れが無い）
    #[test]
    fn color_vars_match_the_theme_files() {
        assert_eq!(COLOR_VARS.len(), 15);
        assert!(COLOR_VARS.contains(&"--c-outline"));
        assert!(COLOR_VARS.contains(&"--c-favorite"));
        let names = |css: &str| {
            let mut v: Vec<String> = css
                .lines()
                .filter_map(|l| l.trim().strip_prefix("--c-").map(|r| format!("--c-{}", r.split(':').next().unwrap_or("").trim())))
                .collect();
            v.sort();
            v
        };
        let mut want: Vec<String> = COLOR_VARS.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(names(include_str!("../../../src/theme/dark.css")), want);
        assert_eq!(names(include_str!("../../../src/theme/light.css")), want);
        // 新しい2つの値の形も、ほかの色と同じ確かめを通る
        assert_eq!(color_token("--c-outline", "rgba(255,255,255,.44)").as_deref(), Some("rgba(255, 255, 255, 0.44)"));
        assert_eq!(color_token("--c-favorite", "#FFD23F").as_deref(), Some("#ffd23f"));
        assert_eq!(color_token("--c-favorite", "gold"), None);
    }

    /// v3.8.0：スキンから変えさせない変数は、画面（src\skins\tokens.ts の LOCKED_VARS）と同じ名前・同じ並び。
    /// 再生バーの真ん中の列の幅（--player-center-w）を足した。スキンに書いてあれば警告して使わない
    #[test]
    fn locked_vars_match_the_screen() {
        let ts = include_str!("../../../src/skins/tokens.ts");
        let start = ts.find("export const LOCKED_VARS = [").expect("LOCKED_VARS in tokens.ts");
        let end = start + ts[start..].find("];").expect("end of LOCKED_VARS");
        let screen: Vec<&str> = ts[start..end].lines().skip(1).filter_map(|l| l.trim().strip_prefix('"')?.split('"').next()).collect();
        assert_eq!(screen, LOCKED_VARS.to_vec());
        assert!(LOCKED_VARS.contains(&"--player-center-w"));
        assert_eq!(common_token("--player-center-w", &json!("100px")), CommonToken::Locked);
        // v3.8.0：出力ボタン・プルダウン・ミキサー・メーターの寸法
        for v in ["--player-output-w", "--output-popup-w", "--mixer-w", "--mixer-strip-min-w", "--meter-h"] {
            assert_eq!(common_token(v, &json!("10px")), CommonToken::Locked, "{v}");
        }
        // 寸法の変数は、画面のテーマの変数（tokens.css）に既定の値がある
        let css = include_str!("../../../src/theme/tokens.css");
        for v in [
            "--row-h",
            "--row-art",
            "--player-h",
            "--player-center-w",
            "--player-output-w",
            "--output-popup-w",
            "--mixer-w",
            "--mixer-strip-min-w",
            "--meter-h",
            "--topbar-h",
            "--lyrics-row-h",
        ] {
            assert!(css.contains(&format!("{v}:")), "{v}");
        }
    }

    #[test]
    fn fonts() {
        assert!(font_name("Yu Gothic UI"));
        assert!(font_name("BIZ UDPゴシック"));
        for bad in ["", " Meiryo", "a\"b", "x;y", "url(x)", "a,b", &"a".repeat(65)] {
            assert!(!font_name(bad), "{bad}");
        }
    }
}
