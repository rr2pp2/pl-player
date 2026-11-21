//! 遅れを測る（v3.8.0。仕様「同時再生と配信モード」2.3 ）。画面の出力のプルダウンと「詳細」のミキサーの「遅れを測る…」から。
//!
//! - `latency_setup`：小窓を開いたときに、測る出力（オンで鳴らせるもの）と、録る所の候補（録音デバイスの一覧）・名前から選んだ録る所を返す
//! - `latency_measure`：測る。曲を流している間・ほかの測定が走っている間は断る（結果の `abort` で返す。エラーにはしない）。
//!   測る処理は `spawn_blocking` の別スレッドで走らせ、画面のスレッドも音声スレッドも止めない。鍵は持たない
//!   （司令塔の鍵は「流しているか」「枠の様子」を読む一瞬だけ。通知・コールバックは呼ばない）
//! - `latency_cancel`：やめる（窓を閉じたときも）
//! - `latency_apply`：「この値を入れる」。測ったときと同じデバイスの枠にだけ入れる
//!
//! 外へは通信しない。測る間の音（クリック）は、測る出力と同じデバイスへ鳴らすだけ。

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::AppState;
use crate::audio::devices::{list_input_devices, DeviceInfo};
use crate::audio::latency::{self, diag_line, fill_values, pick_capture, Abort, CaptureSource, FailCode, Options, Target};
use crate::audio::OutputStatus;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::playback::outputs::ID_MAX_CHARS;
use crate::playback::{Controller, MeasureSlot};

/// 測定の走り具合（同時に1つだけ・やめる）。
/// ：「やめる」は画面が付けた測定の番号（`run`。測るたびに増える）で指す。
/// コマンドは非同期なので、「測る」の直後の「やめる」が測定の始まりより先に着くことがある。
/// 番号で覚えておけば、先に着いた「やめる」も、あとから始まったその測定に効く（始まりで消さない）。古い番号の「やめる」は新しい測定を止めない
#[derive(Default)]
pub struct LatencyControl {
    flags: Arc<Flags>,
}

#[derive(Default)]
struct Flags {
    running: AtomicBool,
    /// やめるように言われた測定の番号のうち、いちばん大きいもの（0 は無し）
    cancelled: AtomicU64,
}

impl Flags {
    fn is_cancelled(&self, run: u64) -> bool {
        self.cancelled.load(Ordering::SeqCst) >= run
    }
}

/// 走っている印を、終わったとき（panic でも）に必ず下ろす
struct Running(Arc<Flags>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::SeqCst);
    }
}

impl LatencyControl {
    /// 始める。もう走っていれば None
    fn begin(&self) -> Option<Running> {
        if self.flags.running.swap(true, Ordering::SeqCst) {
            return None;
        }
        Some(Running(self.flags.clone()))
    }
    /// 番号 `run` まで（その番号とそれより古い測定）をやめる
    fn cancel(&self, run: u64) {
        self.flags.cancelled.fetch_max(run, Ordering::SeqCst);
    }
    fn is_cancelled(&self, run: u64) -> bool {
        self.flags.is_cancelled(run)
    }
}

/// 録音デバイス（録る所の候補）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputView {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRow {
    pub slot: usize,
    pub name: String,
    pub status: OutputStatus,
    /// 測る（オンで鳴らせる）か。オフ・切断中・開けません・重複は測らない
    pub measurable: bool,
    /// 名前から選んだ録る所
    pub auto_capture: CaptureSource,
    pub delay_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencySetup {
    pub rows: Vec<SetupRow>,
    pub inputs: Vec<InputView>,
}

/// 画面で選び直した録る所
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureChoice {
    pub slot: usize,
    pub capture: CaptureSource,
}

/// 全体をやめた・始めなかった理由
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortCode {
    /// 曲を流している（流し始めた）
    Playing,
    /// やめた
    Cancelled,
    /// ほかの測定が走っている
    Busy,
    /// 鳴らせるオンの出力先が2つ未満（揃える相手がいない）
    NotEnough,
    /// 測る途中で思わぬエラー
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRow {
    pub slot: usize,
    /// 測ったときの、その枠のデバイスの設定（入れるときに同じかを確かめる）
    pub device_id: Option<String>,
    pub name: String,
    pub status: OutputStatus,
    /// 測った（測ろうとした）か。オフ・切断中などは false
    pub measured: bool,
    pub capture: Option<CaptureSource>,
    /// 届くまでの遅れ（ms）
    pub arrival_ms: Option<f64>,
    /// そろった回の最大 − 最小（ms）
    pub spread_ms: Option<f64>,
    pub hits: usize,
    /// 入れる値（ms）。測れなかった出力・揃える相手がいないときは null
    pub fill_ms: Option<u32>,
    /// 測ったときの遅らせ量（ms）
    pub delay_ms: u32,
    pub fail: Option<FailCode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyReport {
    pub abort: Option<AbortCode>,
    pub rows: Vec<ReportRow>,
}

fn measurable(s: &MeasureSlot) -> bool {
    s.status == OutputStatus::Ok
}

fn input_views(list: Vec<DeviceInfo>) -> Vec<InputView> {
    list.into_iter().map(|d| InputView { id: d.id, name: d.name }).collect()
}

/// 録音デバイスの一覧（WASAPI の列挙は COM を使うので、画面のスレッドでは引かない）
async fn inputs() -> Vec<DeviceInfo> {
    tauri::async_runtime::spawn_blocking(list_input_devices).await.unwrap_or_default()
}

#[tauri::command]
pub async fn latency_setup(state: State<'_, AppState>) -> AppResult<LatencySetup> {
    let slots = state.controller.measure_slots();
    let list = inputs().await;
    let rows = slots
        .iter()
        .map(|s| SetupRow {
            slot: s.slot,
            name: s.name.clone(),
            status: s.status,
            measurable: measurable(s),
            auto_capture: pick_capture(&s.name, &list),
            delay_ms: s.delay_ms,
        })
        .collect();
    Ok(LatencySetup { rows, inputs: input_views(list) })
}

/// 測る出力と録る所を決める（純粋。テストで確かめる）。並びは枠の番号の順で、クリックの高さもこの順に割り当てる
fn plan_targets(slots: &[MeasureSlot], inputs: &[DeviceInfo], choices: &[CaptureChoice]) -> Vec<(usize, Target)> {
    slots
        .iter()
        .filter(|s| measurable(s))
        .take(latency::tuning::FREQS_HZ.len())
        .enumerate()
        .map(|(k, s)| {
            let chosen = choices.iter().find(|c| c.slot == s.slot).map(|c| c.capture.clone());
            let capture = chosen.unwrap_or_else(|| pick_capture(&s.name, inputs));
            (s.slot, Target { output_id: s.resolved_id.clone(), capture, freq_hz: latency::tuning::FREQS_HZ[k] })
        })
        .collect()
}

fn report_rows(slots: &[MeasureSlot], planned: &[(usize, Target)], outcomes: Option<&[Result<latency::Measured, FailCode>]>) -> Vec<ReportRow> {
    let arrivals: Vec<Option<f64>> =
        outcomes.map(|o| o.iter().map(|r| r.as_ref().ok().map(|m| m.arrival_ms)).collect()).unwrap_or_default();
    let fills = fill_values(&arrivals);
    slots
        .iter()
        .map(|s| {
            let idx = planned.iter().position(|(slot, _)| *slot == s.slot);
            let res = idx.and_then(|i| outcomes.and_then(|o| o.get(i)));
            ReportRow {
                slot: s.slot,
                device_id: s.device_id.clone(),
                name: s.name.clone(),
                status: s.status,
                measured: idx.is_some(),
                capture: idx.map(|i| planned[i].1.capture.clone()),
                arrival_ms: res.and_then(|r| r.as_ref().ok()).map(|m| m.arrival_ms),
                spread_ms: res.and_then(|r| r.as_ref().ok()).map(|m| m.spread_ms),
                hits: res.and_then(|r| r.as_ref().ok()).map(|m| m.hits).unwrap_or(0),
                fill_ms: idx.and_then(|i| fills.get(i).copied().flatten()),
                delay_ms: s.delay_ms,
                fail: res.and_then(|r| r.as_ref().err().copied()),
            }
        })
        .collect()
}

fn aborted(slots: &[MeasureSlot], code: AbortCode) -> LatencyReport {
    LatencyReport { abort: Some(code), rows: report_rows(slots, &[], None) }
}

fn check_choices(choices: &[CaptureChoice]) -> AppResult<()> {
    let bad = choices.iter().any(|c| match &c.capture {
        CaptureSource::Input { id } => id.trim().is_empty() || id.chars().count() > ID_MAX_CHARS,
        CaptureSource::Loopback => false,
    });
    if bad || choices.len() > crate::audio::SLOTS {
        return Err(AppError::new(ErrorKind::OutputInvalid, "invalid capture choice"));
    }
    Ok(())
}

/// 測る（通常は約 6 秒）。曲を流している間は始めない。測る間に流し始めたら、そこでやめる（クリックと曲が混ざるため）。
/// `run` は画面が付けた測定の番号（測るたびに増やす。`latency_cancel` で同じ番号を渡すとやめる。0 は使わない）
#[tauri::command]
pub async fn latency_measure(state: State<'_, AppState>, run: u64, choices: Vec<CaptureChoice>) -> AppResult<LatencyReport> {
    check_choices(&choices)?;
    if run == 0 {
        return Err(AppError::new(ErrorKind::OutputInvalid, "invalid run number"));
    }
    let controller: Arc<Controller> = state.controller.clone();
    let slots = controller.measure_slots();
    if controller.is_playing() {
        return Ok(aborted(&slots, AbortCode::Playing));
    }
    let Some(running) = state.latency.begin() else {
        return Ok(aborted(&slots, AbortCode::Busy));
    };
    // 始まる前に「やめる」が着いていた（「測る」の直後に押した）
    if state.latency.is_cancelled(run) {
        return Ok(aborted(&slots, AbortCode::Cancelled));
    }
    if slots.iter().filter(|s| measurable(s)).count() < 2 {
        return Ok(aborted(&slots, AbortCode::NotEnough));
    }
    let flags = running.0.clone();
    let task = tauri::async_runtime::spawn_blocking(move || {
        // 走っている印は、このスレッドが終わるときに下ろす（画面の待ちが先に切れても、測定が終わるまでは次を始めない）
        let _running = running;
        let list = list_input_devices();
        let planned = plan_targets(&slots, &list, &choices);
        let targets: Vec<Target> = planned.iter().map(|(_, t)| t.clone()).collect();
        let stop = || {
            if flags.is_cancelled(run) {
                Some(Abort::Cancelled)
            } else if controller.is_playing() {
                Some(Abort::Playing)
            } else {
                None
            }
        };
        match catch_unwind(AssertUnwindSafe(|| latency::measure_with(&targets, &stop, Options::default()))) {
            Ok(Ok(diags)) => {
                // 手元のログ（%APPDATA% の logs）に、出力ごと・回ごとの記録を残す。機器の名前は入れず、枠の番号と録る所の種類だけ
                // （2026-11-18 の実機の試しで仮想ケーブルが毎回「そろわない」になり、原因を絞るのに回ごとの値が要った）
                for ((slot, t), d) in planned.iter().zip(&diags) {
                    let kind = if t.capture == CaptureSource::Loopback { "loopback" } else { "input" };
                    tracing::info!("latency slot{slot} {kind} {:.0}Hz: {}", t.freq_hz, diag_line(d));
                }
                let outcomes: Vec<_> = diags.into_iter().map(|d| d.outcome).collect();
                LatencyReport { abort: None, rows: report_rows(&slots, &planned, Some(&outcomes)) }
            }
            Ok(Err(Abort::Playing)) => aborted(&slots, AbortCode::Playing),
            Ok(Err(Abort::Cancelled)) => aborted(&slots, AbortCode::Cancelled),
            Err(_) => {
                tracing::warn!("latency measure panicked");
                aborted(&slots, AbortCode::Failed)
            }
        }
    });
    task.await.map_err(|e| AppError::other(format!("latency task: {e}")))
}

/// やめる。`run` は `latency_measure` に渡した番号（その番号とそれより古い測定をやめる。まだ始まっていなくても効く）
#[tauri::command]
pub async fn latency_cancel(state: State<'_, AppState>, run: u64) -> AppResult<()> {
    state.latency.cancel(run);
    Ok(())
}

/// 「この値を入れる」の1つ
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyItem {
    pub slot: usize,
    /// 測ったときのデバイスの設定（null は既定のデバイス）
    pub device_id: Option<String>,
    pub delay_ms: u32,
}

/// 遅らせ量にまとめて入れる。入れた枠の番号を返す（測ってからデバイスが変わった枠には入れない）
#[tauri::command]
pub async fn latency_apply(state: State<'_, AppState>, items: Vec<ApplyItem>) -> AppResult<Vec<usize>> {
    if items.len() > crate::audio::SLOTS {
        return Err(AppError::new(ErrorKind::OutputInvalid, "too many items"));
    }
    let v: Vec<(usize, Option<String>, u32)> = items.into_iter().map(|i| (i.slot, i.device_id, i.delay_ms)).collect();
    state.controller.output_set_delays(&v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::latency::Measured;

    fn slot(slot: usize, name: &str, status: OutputStatus, delay_ms: u32) -> MeasureSlot {
        MeasureSlot {
            slot,
            device_id: Some(format!("id-{slot}")),
            resolved_id: Some(format!("id-{slot}")),
            name: name.into(),
            status,
            delay_ms,
        }
    }

    fn dev(id: &str, name: &str) -> DeviceInfo {
        DeviceInfo { id: id.into(), name: name.into(), is_default: false, kind: "speaker" }
    }

    /// 測るのはオンで鳴らせる出力だけ。録る所は選び直しがあればそれ、無ければ名前から。クリックの高さは測る順に 1k・3k・5k
    #[test]
    fn targets_are_the_sounding_outputs() {
        let slots = [
            slot(0, "ヘッドホン (架空の機器)", OutputStatus::Ok, 0),
            slot(1, "スピーカー (架空の機器)", OutputStatus::Off, 0),
            slot(2, "CABLE Input (仮想ケーブル)", OutputStatus::Ok, 12),
        ];
        let inputs = [dev("in-cable", "CABLE Output (仮想ケーブル)")];
        let p = plan_targets(&slots, &inputs, &[]);
        assert_eq!(p.iter().map(|(s, _)| *s).collect::<Vec<_>>(), vec![0, 2]);
        assert_eq!(p[0].1.capture, CaptureSource::Loopback);
        assert_eq!(p[1].1.capture, CaptureSource::Input { id: "in-cable".into() });
        assert_eq!((p[0].1.freq_hz, p[1].1.freq_hz), (1000.0, 3000.0));
        assert_eq!(p[0].1.output_id.as_deref(), Some("id-0"));
        // 選び直し：仮想ケーブルもループバックで
        let p = plan_targets(&slots, &inputs, &[CaptureChoice { slot: 2, capture: CaptureSource::Loopback }]);
        assert_eq!(p[1].1.capture, CaptureSource::Loopback);
        // 切断中・開けません・重複は測らない
        let bad = [
            slot(0, "a", OutputStatus::Disconnected, 0),
            slot(1, "b", OutputStatus::Unavailable, 0),
            slot(2, "c", OutputStatus::Duplicate, 0),
        ];
        assert!(plan_targets(&bad, &inputs, &[]).is_empty());
    }

    /// 結果の行：オフの行は測らず、測れなかった出力には入れる値を出さない。入れる値は測れた出力の中でいちばん遅いものに揃える
    #[test]
    fn report_rows_put_values_only_on_measured_outputs() {
        let slots = [
            slot(0, "ヘッドホン", OutputStatus::Ok, 5),
            slot(1, "スピーカー", OutputStatus::Off, 0),
            slot(2, "仮想ケーブル", OutputStatus::Ok, 0),
        ];
        let planned = plan_targets(&slots, &[], &[]);
        let ok = |d: f64| Ok(Measured { arrival_ms: d, spread_ms: 0.3, hits: 6 });
        let rows = report_rows(&slots, &planned, Some(&[ok(0.4), ok(33.4)]));
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[0].fill_ms, rows[0].delay_ms, rows[0].measured), (Some(33), 5, true));
        assert_eq!((rows[1].fill_ms, rows[1].measured, rows[1].fail), (None, false, None));
        assert_eq!(rows[2].fill_ms, Some(0));
        // 片方が測れなかった：揃える相手がいないので、どちらにも入れる値を出さない。理由は行に
        let rows = report_rows(&slots, &planned, Some(&[ok(0.4), Err(FailCode::NoSignal)]));
        assert_eq!((rows[0].arrival_ms, rows[0].fill_ms), (Some(0.4), None));
        assert_eq!((rows[2].fail, rows[2].fill_ms), (Some(FailCode::NoSignal), None));
        // やめたとき：どの行も測っていない
        let a = aborted(&slots, AbortCode::Playing);
        assert_eq!(a.abort, Some(AbortCode::Playing));
        assert!(a.rows.iter().all(|r| !r.measured && r.fill_ms.is_none()));
    }

    /// 走っている間は2つ目を始めない。終われば（panic でも）次を始められる
    #[test]
    fn only_one_measurement_at_a_time() {
        let c = LatencyControl::default();
        let a = c.begin().expect("first");
        assert!(c.begin().is_none());
        drop(a);
        let b = c.begin().expect("again");
        let flags = b.0.clone();
        let r = std::panic::catch_unwind(AssertUnwindSafe(move || {
            let _held = b;
            panic!("boom");
        }));
        assert!(r.is_err());
        assert!(!flags.running.load(Ordering::SeqCst));
        assert!(c.begin().is_some());
    }

    /// ：「やめる」は番号で効く。始まる前に着いた「やめる」も、その測定に効く（始まりで消えない）。
    /// 古い番号の「やめる」は、新しい測定を止めない。遅れて着いた古い「やめる」も新しい測定を止めない
    #[test]
    fn cancel_by_run_number_even_before_begin() {
        let c = LatencyControl::default();
        assert!(!c.is_cancelled(5));
        // 「測る」(5) の直後の「やめる」(5) が、測定の始まりより先に着く
        c.cancel(5);
        let a = c.begin().expect("begin");
        assert!(c.is_cancelled(5));
        assert!(a.0.is_cancelled(5));
        drop(a);
        // 次の測定 (6) は止まらない
        let b = c.begin().expect("next");
        assert!(!b.0.is_cancelled(6));
        // 前の測定への「やめる」が遅れて着いても止まらない
        c.cancel(4);
        assert!(!b.0.is_cancelled(6));
        c.cancel(6);
        assert!(b.0.is_cancelled(6));
    }

    /// 画面から来る録る所の選び直し：空の ID・長すぎる ID・多すぎる数は断る
    #[test]
    fn capture_choices_are_checked() {
        let ok = CaptureChoice { slot: 0, capture: CaptureSource::Input { id: "x".into() } };
        assert!(check_choices(&[ok.clone()]).is_ok());
        assert!(check_choices(&[CaptureChoice { slot: 0, capture: CaptureSource::Input { id: " ".into() } }]).is_err());
        assert!(check_choices(&[CaptureChoice { slot: 0, capture: CaptureSource::Input { id: "a".repeat(ID_MAX_CHARS + 1) } }]).is_err());
        assert!(check_choices(&[ok.clone(), ok.clone(), ok.clone(), ok]).is_err());
        // 画面からの形：{ kind: "input", id } と { kind: "loopback" }
        let v: Vec<CaptureChoice> =
            serde_json::from_str(r#"[{"slot":1,"capture":{"kind":"loopback"}},{"slot":2,"capture":{"kind":"input","id":"abc"}}]"#).unwrap();
        assert_eq!(v[0].capture, CaptureSource::Loopback);
        assert_eq!(v[1].capture, CaptureSource::Input { id: "abc".into() });
    }
}
