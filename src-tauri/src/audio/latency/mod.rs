//! 遅れを測る（v3.8.0 段3。仕様「同時再生と配信モード」2.3 の段3）。
//!
//! Windows は機器ごとの鳴るまでの遅れを教えない（GetStreamLatency が 0。仕様 13.3）うえ、仮想ケーブルの出口に届くまでの遅れは
//! 開くたびに 16〜33ms と変わる（13.3）ので、耳で合わせるのは難しい。そこで、出力ごとにクリックを鳴らして録り、
//! 「届くまでの遅れ」を測る。測り方は計測の道具の `loopback_click_probe`（`tests\sync_probe.rs`）と同じ：
//!
//! - 出力ごとに、測る専用の流れを別に開いて、違う高さ（1kHz・3kHz・5kHz）の短いクリックを同じ時刻を狙って鳴らす。
//!   狙い方は本体の門（`sync::StreamTiming`）と同じ見積もりなので、測った遅れは「本体が合わせたつもりの所から、さらにどれだけ遅れて届くか」
//! - 録る所は、仮想ケーブルの入口（CABLE Input）なら対になる出口の録音デバイス（CABLE Output）、ほかはその出力のループバック。
//!   名前から自動で選び（`pick_capture`）、画面で選び直せる
//! - **今の遅らせ量は通らない**（測る専用の流れなので、遅らせ量も音量も掛けない）。だから、入っている遅らせ量に引きずられない
//! - 測れるのは Windows がデバイスへ渡すところまで。USB の先・HDMI の先・Bluetooth の機器の中の遅れは入らない
//!
//! 入れる値（`fill_values`）は「いちばん遅く届く出力に揃える」：出力 i の届くまでの遅れを d_i として「d の最大 − d_i」
//! （遅れる出力は早められないので、早く届く出力を遅らせる。2026-11-18 作者の確認済み）。
//!
//! 音を鳴らさずに確かめられる部分（クリックの見つけ方・録った流れの切り出し・録る所の選び方・まとめ方・入れる値）は
//! `detect.rs` とこのファイルにあり、デバイスを開く部分は `run.rs`。

pub mod detect;
mod run;

use serde::{Deserialize, Serialize};

use self::detect::HitMiss;
use super::devices::DeviceInfo;

pub use run::{diag_line, measure_with, ClickDiag, TargetDiag};

/// 測り方の値（ここ1か所にまとめる）
pub mod tuning {
    /// クリックの回数と間隔。間隔は「前 `LOOK_BEFORE_MS` ＋後 `LOOK_AFTER_MS` ＋クリックの長さ」より長くする（隣の回が窓に入らない）
    pub const CLICK_COUNT: usize = 6;
    pub const CLICK_INTERVAL_MS: u64 = 500;
    /// クリック1回の長さ（計測の道具と同じ 5ms）
    pub const CLICK_MS: f64 = 5.0;
    /// クリックの大きさ（dBFS）。計測の道具の loopback_click_probe と同じ -30（作者の実機で取れた大きさ）
    pub const CLICK_DB: f64 = -30.0;
    /// 出力ごとのクリックの高さ（1台目・2台目・3台目）
    pub const FREQS_HZ: [f64; 3] = [1000.0, 3000.0, 5000.0];
    /// 狙った時刻の前後どこまでを探すか。後ろは、測れる遅れの上限（これより遅く届くと「届きませんでした」）
    pub const LOOK_BEFORE_MS: u64 = 10;
    pub const LOOK_AFTER_MS: u64 = 400;
    /// 開くのを待つ上限（開く役ごと）
    pub const OPEN_TIMEOUT_MS: u64 = 5000;
    /// 開いてから狙い始めるまで待つ長さ（時刻の見積もりと仮想ケーブルの出口が落ち着くまで）と、その上限。
    /// 2026-11-18 の実機の試しで仮想ケーブルが毎回「そろわない」になり、2026-11-19 の診断（`diagnose`。3回ずつ）で原因が2つ分かった：
    /// - 待ちが短い（500ms：当てはめは約 0.5 秒ぶん）と、当てはめの傾きが外れる（+194〜+946ppm）。全部の回を始めに決めると、
    ///   先の回ほど直線的にずれた（34.5 → 37.2ms）。→ 回ごとに直前で狙う（`PLAN_AHEAD_MS`）
    /// - 仮想ケーブルの出口は、開いてから 1.3〜2.0 秒ごろに、届くまでの遅れが約 10ms 跳ぶことがある（40.2 → 29.8ms・43.6 → 33.3ms）。
    ///   待ち 500ms では 3回のうち2回で跳び、2000ms では跳ばなかったが跳ぶ時刻に近い。3000ms（計測の道具と同じ）では3回とも跳ばず、
    ///   ばらつき 0.39〜0.77ms・当てはめ 0.4〜2.7ppm だった。→ 3 秒待つ
    /// - ただし跳ぶ時刻は毎回同じではない。同じ日の 30 分の計測（sync_probe の loopback_click_probe）では、開いてから約 10 秒後に
    ///   33ms → 23ms と跳び、そのあと 30 分は 22.6〜23.0ms のままだった。3 秒待っても跳ぶ前の値を測ることがある
    ///   （作者の決定で、待ちは延ばさず「だいたいの値」と画面と README に書いて出す。次の版で考え直す）
    pub const WARMUP_MS: u64 = 3000;
    pub const WARMUP_MAX_MS: u64 = 5000;
    /// 狙いを決めてから最初のクリックまで（出力の輪にもう書いた所より先を狙うため）
    pub const FIRST_CLICK_LEAD_MS: u64 = 300;
    /// 回ごとの狙いは、その回の時刻のこれだけ前になってから決める（先の見積もりを短くする。計測の道具は約 50ms 前に決めていた。
    /// 出力の輪に書く所より先を狙えるよう、見回る間隔とバッファのぶんの余裕を取る）
    pub const PLAN_AHEAD_MS: u64 = 200;
    /// 最後の窓の終わりから、これだけ待っても録れていなければ「録れていない」で閉じる
    pub const CAPTURE_TAIL_MS: u64 = 1500;
    /// 見回る間隔
    pub const POLL_MS: u64 = 15;
    /// 取れた回がこれ未満なら「測れませんでした」
    pub const MIN_HITS: usize = 3;
    /// 中央値からこれ以上離れた回は外す（ms）。外したあとに MIN_HITS 回残らなければ「結果がそろいませんでした」
    pub const INLIER_MS: f64 = 2.0;
}

/// 測り方の選び（ふだんは `Default`。診断のテストで、前の作りと比べるために変える）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// 開いてから狙い始めるまで、時刻の記録を見る長さ（ms）
    pub warmup_ms: u64,
    /// 回ごとの狙いを、その回のどれだけ前に決めるか（ms）。大きくすると全部の回を始めに決める（前の作り）
    pub plan_ahead_ms: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self { warmup_ms: tuning::WARMUP_MS, plan_ahead_ms: tuning::PLAN_AHEAD_MS }
    }
}

/// 録る所
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureSource {
    /// その出力のループバック（Windows がその出力へ渡した音）
    Loopback,
    /// 録音デバイス（仮想ケーブルの出口など）
    Input { id: String },
}

/// 測る出力1つ
#[derive(Debug, Clone)]
pub struct Target {
    /// 開く出力デバイスの ID（None なら今の既定のデバイス）
    pub output_id: Option<String>,
    pub capture: CaptureSource,
    /// クリックの高さ（`tuning::FREQS_HZ` のどれか）
    pub freq_hz: f64,
}

/// 測れなかった理由（画面が日本語の文にする。`src\layout\latency.ts`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailCode {
    /// 出力のデバイスが見つからない（抜けた）
    OutputMissing,
    /// 出力をほかのアプリが独り占めしている
    OutputBusy,
    /// 出力の形式が使えない
    OutputFormat,
    /// 出力を開けない（その他）
    OutputOpen,
    /// 録るデバイスが見つからない
    CaptureMissing,
    CaptureBusy,
    CaptureFormat,
    CaptureOpen,
    /// 録るデバイスを開くのを Windows が断った（マイクへのアクセスの設定。仮想ケーブルの出口も録音デバイスとして開くため。検査1回目の指摘4）
    CaptureDenied,
    /// 開くのに時間がかかりすぎた
    OpenTimeout,
    /// 自前の時計とデバイスの時計がそろわない（13.1 の2）
    Clock,
    /// 出力から時刻の記録が来ない
    NoTiming,
    /// 録るデバイスから音が来ない（パケットが1つも来ない）
    CaptureSilent,
    /// 測っている途中でデバイスが止まった（抜けた・形式が変わった）
    StreamError,
    /// クリックを鳴らすのが間に合わなかった
    Late,
    /// 録音が途切れた・録れていない
    Gap,
    /// クリックが届かなかった（音が無い）
    NoSignal,
    /// ほかの音に埋もれた
    Weak,
    /// ほかの音とクリックを見分けられなかった（物音・形が違う・紛らわしい・端で切れた）
    Noisy,
    /// 回ごとの結果がそろわなかった
    Unstable,
}

/// クリック1回の結果がだめだった理由
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Miss {
    Detect(HitMiss),
    /// 鳴らすのが間に合わなかった
    Late,
}

impl Miss {
    pub fn code(self) -> FailCode {
        match self {
            Miss::Late => FailCode::Late,
            Miss::Detect(HitMiss::NotCaptured | HitMiss::Gap) => FailCode::Gap,
            Miss::Detect(HitMiss::NoSignal) => FailCode::NoSignal,
            Miss::Detect(HitMiss::Weak) => FailCode::Weak,
            Miss::Detect(HitMiss::Truncated | HitMiss::WrongShape | HitMiss::NotTone | HitMiss::Ambiguous) => FailCode::Noisy,
        }
    }
}

/// 測れた出力の結果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    /// 届くまでの遅れ（ms。そろった回の中央値）
    pub arrival_ms: f64,
    /// そろった回の最大 − 最小（ms）
    pub spread_ms: f64,
    /// そろった回の数
    pub hits: usize,
}

/// 測る途中で全体をやめた理由
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Abort {
    /// 曲を流し始めた（クリックと曲が混ざる）
    Playing,
    /// やめるを押した・窓を閉じた
    Cancelled,
}

/// 名前の中の語「Input」を「Output」に替えた名前（最後の1つだけ。前後が英数字に続くものは語ではないので替えない）。
/// 仮想ケーブルは、再生デバイス側（入口）が「CABLE Input …」、録音デバイス側（出口）が「CABLE Output …」と対になっている
pub fn paired_input_name(output_name: &str) -> Option<String> {
    let word = "Input";
    let mut at = None;
    for (i, _) in output_name.match_indices(word) {
        let before = output_name[..i].chars().next_back();
        let after = output_name[i + word.len()..].chars().next();
        let is_word = |c: Option<char>| c.map_or(true, |c| !c.is_ascii_alphanumeric());
        if is_word(before) && is_word(after) {
            at = Some(i);
        }
    }
    let i = at?;
    Some(format!("{}Output{}", &output_name[..i], &output_name[i + word.len()..]))
}

/// 録る所を名前から決める：対になる録音デバイスがあればそれ、無ければその出力のループバック（仕様 2.3 の段3）
pub fn pick_capture(output_name: &str, inputs: &[DeviceInfo]) -> CaptureSource {
    paired_input_name(output_name)
        .and_then(|want| inputs.iter().find(|d| d.name == want))
        .map(|d| CaptureSource::Input { id: d.id.clone() })
        .unwrap_or(CaptureSource::Loopback)
}

/// 回ごとの結果をまとめる。取れた回が `MIN_HITS` 未満なら、いちばん多かった理由で「測れませんでした」。
/// 中央値から `INLIER_MS` 以上離れた回は外し、残りが `MIN_HITS` 未満なら「そろわなかった」
pub fn summarize(results: &[Result<f64, Miss>]) -> Result<Measured, FailCode> {
    let hits: Vec<f64> = results.iter().filter_map(|r| r.as_ref().ok().copied()).filter(|v| v.is_finite()).collect();
    if hits.len() < tuning::MIN_HITS {
        let mut counts: Vec<(Miss, usize)> = vec![];
        for m in results.iter().filter_map(|r| r.as_ref().err().copied()) {
            match counts.iter_mut().find(|(k, _)| *k == m) {
                Some((_, n)) => *n += 1,
                None => counts.push((m, 1)),
            }
        }
        // いちばん多い理由（同じ数なら並びの小さいほう＝先に断った理由）
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        return Err(counts.first().map(|(m, _)| m.code()).unwrap_or(FailCode::NoSignal));
    }
    let med = median(&hits);
    let mut inl: Vec<f64> = hits.into_iter().filter(|v| (v - med).abs() <= tuning::INLIER_MS).collect();
    if inl.len() < tuning::MIN_HITS {
        return Err(FailCode::Unstable);
    }
    inl.sort_by(|a, b| a.total_cmp(b));
    let spread = inl[inl.len() - 1] - inl[0];
    Ok(Measured { arrival_ms: median(&inl), spread_ms: spread, hits: inl.len() })
}

fn median(v: &[f64]) -> f64 {
    let mut w = v.to_vec();
    w.sort_by(|a, b| a.total_cmp(b));
    let n = w.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        w[n / 2]
    } else {
        (w[n / 2 - 1] + w[n / 2]) / 2.0
    }
}

/// 入れる値（ms）：「測れた出力の d の最大 − d_i」を 1ms に丸め、遅らせ量の範囲（0〜1000）に収める。
/// 測れた出力が2つ未満なら、揃える相手がいないので、どれにも値を出さない（測れなかった出力には、いつも出さない）
pub fn fill_values(arrivals: &[Option<f64>]) -> Vec<Option<u32>> {
    let got: Vec<f64> = arrivals.iter().flatten().copied().filter(|v| v.is_finite()).collect();
    if got.len() < 2 {
        return vec![None; arrivals.len()];
    }
    let max = got.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let cap = super::sync::tuning::DELAY_MAX_MS as f64;
    arrivals
        .iter()
        .map(|a| a.filter(|v| v.is_finite()).map(|d| (max - d).round().clamp(0.0, cap) as u32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: &str, name: &str) -> DeviceInfo {
        DeviceInfo { id: id.into(), name: name.into(), is_default: false, kind: "speaker" }
    }

    /// 仮想ケーブルの入口 → 出口の名前。語でない「Input」（Inputs・XInput など）は替えない。最後の1つだけ替える
    #[test]
    fn paired_name_replaces_the_word_input() {
        assert_eq!(paired_input_name("CABLE Input (仮想ケーブル)").as_deref(), Some("CABLE Output (仮想ケーブル)"));
        assert_eq!(paired_input_name("CABLE-A Input (仮想ケーブル A)").as_deref(), Some("CABLE-A Output (仮想ケーブル A)"));
        assert_eq!(paired_input_name("Input Input").as_deref(), Some("Input Output"));
        assert_eq!(paired_input_name("スピーカー (架空のオーディオ)"), None);
        assert_eq!(paired_input_name("Inputs (架空)"), None);
        assert_eq!(paired_input_name("XInput 2"), None);
        assert_eq!(paired_input_name(""), None);
    }

    /// 録る所：対になる録音デバイスがあればそれ、無ければループバック。名前は全部が同じときだけ
    #[test]
    fn capture_is_picked_by_name() {
        let inputs = [input("in-1", "マイク (架空の機器)"), input("in-2", "CABLE Output (仮想ケーブル)")];
        assert_eq!(pick_capture("CABLE Input (仮想ケーブル)", &inputs), CaptureSource::Input { id: "in-2".into() });
        assert_eq!(pick_capture("ヘッドホン (架空の機器)", &inputs), CaptureSource::Loopback);
        // 出口が無い（入っていない・名前が違う）ならループバック
        assert_eq!(pick_capture("CABLE-B Input (仮想ケーブル B)", &inputs), CaptureSource::Loopback);
        assert_eq!(pick_capture("CABLE Input (仮想ケーブル)", &[]), CaptureSource::Loopback);
    }

    /// まとめ方：中央値。離れた回は外す。取れた回が足りなければ、いちばん多い理由
    #[test]
    fn summarize_uses_median_and_drops_outliers() {
        let ok = |v: f64| -> Result<f64, Miss> { Ok(v) };
        let r = summarize(&[ok(33.1), ok(33.4), ok(33.2), ok(33.3), ok(80.0), Err(Miss::Detect(HitMiss::Weak))]).unwrap();
        // 80.0 を外した4回の中央値
        assert!((r.arrival_ms - 33.25).abs() < 1e-9, "{r:?}");
        assert_eq!(r.hits, 4);
        assert!((r.spread_ms - 0.3).abs() < 1e-9);
        // 取れた回が2回だけ
        let few = [ok(1.0), ok(1.1), Err(Miss::Detect(HitMiss::NoSignal)), Err(Miss::Detect(HitMiss::NoSignal)), Err(Miss::Late)];
        assert_eq!(summarize(&few), Err(FailCode::NoSignal));
        // 理由の振り分け
        assert_eq!(summarize(&[Err(Miss::Detect(HitMiss::Gap))]), Err(FailCode::Gap));
        assert_eq!(summarize(&[Err(Miss::Detect(HitMiss::Ambiguous))]), Err(FailCode::Noisy));
        assert_eq!(summarize(&[]), Err(FailCode::NoSignal));
        // ばらばら（中央値の近くに3回残らない）
        assert_eq!(summarize(&[ok(1.0), ok(10.0), ok(20.0), ok(30.0), ok(40.0)]), Err(FailCode::Unstable));
        // 数でない値は数えない
        assert_eq!(summarize(&[ok(f64::NAN), ok(f64::INFINITY), ok(1.0)]), Err(FailCode::NoSignal));
    }

    /// 入れる値：いちばん遅く届く出力に揃える（d の最大 − d_i）。測れなかった出力には出さない。2つ未満ならどれにも出さない
    #[test]
    fn fill_values_align_to_the_slowest() {
        // ヘッドホン 0.4ms・仮想ケーブル 33.4ms → ヘッドホンを 33ms 遅らせ、仮想ケーブルは 0
        assert_eq!(fill_values(&[Some(0.4), Some(33.4)]), vec![Some(33), Some(0)]);
        // 3つ。真ん中が測れなかった
        assert_eq!(fill_values(&[Some(1.0), None, Some(16.6)]), vec![Some(16), None, Some(0)]);
        // 測れたのが1つだけ → 揃える相手がいない
        assert_eq!(fill_values(&[Some(1.0), None]), vec![None, None]);
        assert_eq!(fill_values(&[]), Vec::<Option<u32>>::new());
        // 負の遅れ（狙いより少し早い）も差で出す。上限 1000ms
        assert_eq!(fill_values(&[Some(-0.6), Some(0.4)]), vec![Some(1), Some(0)]);
        assert_eq!(fill_values(&[Some(0.0), Some(2500.0)]), vec![Some(1000), Some(0)]);
        // 数でない値は測れなかったものとして扱う
        assert_eq!(fill_values(&[Some(f64::NAN), Some(1.0), Some(3.0)]), vec![None, Some(2), Some(0)]);
    }

    /// クリックの回の間隔は、前後の窓とクリックの長さより長い（隣の回が窓に入らない）。高さは 1kHz ずつ以上離れる
    #[test]
    fn click_schedule_leaves_room_between_windows() {
        use tuning::*;
        assert!(((LOOK_BEFORE_MS + LOOK_AFTER_MS) as f64 + CLICK_MS) < CLICK_INTERVAL_MS as f64);
        assert!(MIN_HITS <= CLICK_COUNT);
        assert!(CLICK_COUNT <= 32, "鳴らせなかった回の印（u32）に入る");
        for w in FREQS_HZ.windows(2) {
            assert!(w[1] - w[0] >= 1000.0);
        }
        assert!(CLICK_DB <= -6.0);
        // 狙いを決めるのは回の時刻の前（見回る間隔より長く）。落ち着くまで待つ長さは上限より短い
        assert!(PLAN_AHEAD_MS > POLL_MS * 2 && PLAN_AHEAD_MS < CLICK_INTERVAL_MS);
        assert!(FIRST_CLICK_LEAD_MS <= PLAN_AHEAD_MS + 200);
        assert!(WARMUP_MS < WARMUP_MAX_MS);
    }
}
