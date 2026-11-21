//! 出力先の操作（v3.8.0。仕様「同時再生と配信モード」2章・4.4・6章・7章・13.5）。
//!
//! 画面・ホットキー・トレイの出力先の操作は全部ここを通る。設定（`OutputsDoc`）を変えたら、
//! 音声スレッドへ枠の設定と音量を送り、DB の `outputs` に書き、画面へ全部を送る（`pl://outputs`）。
//! 出力ごとの音量つまみだけは、動かしている間は保存しない（離したとき `output_commit_volume`）。

use serde::Serialize;

use super::controller::{Controller, Inner, Notice};
use super::outputs::{self, DeviceRef, Mode, OpError, OutputsDoc, PerMode};
use crate::audio::{Command, OutputBank, OutputStatus, UnavailableReason, SLOTS};
use crate::db;
use crate::error::{AppError, AppResult};

/// 枠ごとの、音声スレッドが知らせた今の状態
#[derive(Debug, Clone, Default)]
pub struct OutRuntime {
    pub status: Option<OutputStatus>,
    pub reason: Option<UnavailableReason>,
    /// 実際のデバイスの名前（既定のデバイスなら、今の既定のデバイスの名前）
    pub name: Option<String>,
    pub resolved_id: Option<String>,
    pub kind: Option<&'static str>,
}

/// 画面へ送る出力先の全部（`pl://outputs` と `outputs_get`）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputsView {
    /// 表示のモード（normal / detail）
    pub mode: Mode,
    /// 今のモードの全体の音量
    pub master: f32,
    pub masters: PerMode,
    pub muted: bool,
    /// 枠3つ（空の枠は null）
    pub slots: Vec<Option<SlotView>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotView {
    pub slot: usize,
    /// null なら既定のデバイス（Windows に従う）
    pub device_id: Option<String>,
    /// 保存してある名前（切断中の行に出す。3.7.0 から引き継いだ行は空＝「前に使っていたデバイス」）
    pub saved_name: String,
    /// 実際のデバイスの名前（開けた・一覧で見つかったとき。既定のデバイスなら今の既定の名前）
    pub device_name: Option<String>,
    pub on: bool,
    pub delay_ms: u32,
    /// 選び直したアイコン（null なら自動）
    pub icon: Option<String>,
    /// 自動のアイコン（Windows が教える機器の種類から。分からなければ speaker）
    pub auto_icon: &'static str,
    /// 今のモードの出力ごとの音量
    pub volume: f32,
    pub volumes: PerMode,
    /// 全体の音量に従う（詳細でだけ効く。通常ではいつも従う）
    pub follow_master: bool,
    /// off / ok / disconnected / unavailable / duplicate（6.4）
    pub status: OutputStatus,
    /// 開けませんの理由：format / busy / other
    pub reason: Option<UnavailableReason>,
}

/// 遅れを測るときに見る、枠1つの今の様子（v3.8.0）
#[derive(Debug, Clone)]
pub struct MeasureSlot {
    pub slot: usize,
    /// 設定のデバイス（null は既定のデバイス）。入れるときに、測ったときと同じかを確かめる
    pub device_id: Option<String>,
    /// 開くデバイス（既定のデバイスの枠なら、今の既定のデバイス。分からなければ設定のまま）
    pub resolved_id: Option<String>,
    /// 実際のデバイスの名前（無ければ保存してある名前。録る所を名前から選ぶのに使う）
    pub name: String,
    pub status: OutputStatus,
    pub delay_ms: u32,
}

fn view_of(i: &Inner) -> OutputsView {
    let slots = (0..SLOTS)
        .map(|n| {
            let s = i.outputs.slot(n)?;
            let rt = &i.out_rt[n];
            let status = if !s.on {
                OutputStatus::Off
            } else {
                rt.status.filter(|st| *st != OutputStatus::Off).unwrap_or(OutputStatus::Ok)
            };
            Some(SlotView {
                slot: n,
                device_id: s.device.id.clone(),
                saved_name: s.device.name.clone(),
                device_name: rt.name.clone(),
                on: s.on,
                delay_ms: s.delay_ms,
                icon: s.icon.clone(),
                auto_icon: rt.kind.unwrap_or("speaker"),
                volume: s.volume.get(i.mode),
                volumes: s.volume,
                follow_master: s.follow_master,
                status,
                reason: if status == OutputStatus::Unavailable { rt.reason } else { None },
            })
        })
        .collect();
    OutputsView { mode: i.mode, master: i.outputs.master.get(i.mode), masters: i.outputs.master, muted: i.muted, slots }
}

impl Controller {
    /// 枠ごとの実際に掛ける倍率を音声スレッドへ（ロックを持ったまま呼ぶ）
    pub(super) fn apply_gains(&self, i: &Inner) {
        for (n, g) in i.outputs.gains(i.mode, i.muted).iter().enumerate() {
            self.engine.set_output_gain(n, *g);
        }
    }

    pub fn outputs_view(&self) -> OutputsView {
        view_of(&self.lock())
    }

    pub(super) fn publish_outputs(&self) {
        let v = self.outputs_view();
        self.each(|l| l.on_outputs(&v));
    }

    pub fn save_outputs(&self) {
        let d = self.lock().outputs.clone();
        if let Err(e) = outputs::save(&self.db, &d) {
            tracing::warn!("save outputs: {e}");
        }
    }

    /// メーター（13.5 の13）
    pub fn meter_bank(&self) -> std::sync::Arc<OutputBank> {
        self.engine.bank()
    }

    pub fn mode(&self) -> Mode {
        self.lock().mode
    }

    /// 出力先の設定を変える：音声スレッドへ送り、保存し、画面へ送る
    pub(super) fn update_outputs(&self, f: impl FnOnce(&mut OutputsDoc) -> Result<(), OpError>) -> AppResult<()> {
        self.update_outputs_reset(f, None)
    }

    /// `reset` の枠の、音声スレッドが知らせた状態も消す（外した・差し替えた）。消すのは音声スレッドへ送る前・同じロックの中
    /// （送ったあとに消すと、先に届いた新しい状態まで消しうる）
    fn update_outputs_reset(
        &self,
        f: impl FnOnce(&mut OutputsDoc) -> Result<(), OpError>,
        reset: Option<usize>,
    ) -> AppResult<()> {
        let saved = {
            let mut i = self.lock();
            let mut d = i.outputs.clone();
            f(&mut d).map_err(AppError::from)?;
            i.outputs = d;
            if let Some(n) = reset.filter(|n| *n < SLOTS) {
                i.out_rt[n] = OutRuntime::default();
            }
            self.engine.send(Command::SetOutputs(i.outputs.specs()));
            self.apply_gains(&i);
            i.outputs.clone()
        };
        if let Err(e) = outputs::save(&self.db, &saved) {
            tracing::warn!("save outputs: {e}");
        }
        self.publish();
        self.publish_outputs();
        Ok(())
    }

    /// 音声スレッドからの、枠の状態の知らせ
    pub(super) fn on_output_status(
        &self,
        slot: usize,
        status: OutputStatus,
        reason: Option<UnavailableReason>,
        name: Option<String>,
        resolved_id: Option<String>,
        kind: Option<&'static str>,
    ) {
        if slot >= SLOTS {
            return;
        }
        let rename = {
            let mut i = self.lock();
            i.out_rt[slot] = OutRuntime { status: Some(status), reason, name: name.clone(), resolved_id: resolved_id.clone(), kind };
            // 名前を覚え直す（切断中の行に出すため。ID で開けたデバイスだけ。既定のデバイスの枠は名前を持たない）
            match (i.outputs.slots.get_mut(slot).and_then(|s| s.as_mut()), name) {
                (Some(s), Some(n)) if s.device.id.is_some() && s.device.id == resolved_id && s.device.name != n => {
                    s.device.name = n;
                    true
                }
                _ => false,
            }
        };
        if rename {
            self.save_outputs();
        }
        if slot == 0 {
            self.publish();
        }
        self.publish_outputs();
    }

    /// 出力先を足す（6.3）。足した直後はオフ。前に覚えていれば、その設定を戻す
    pub fn output_add(&self, device: DeviceRef) -> AppResult<usize> {
        let mut at = 0;
        self.update_outputs(|d| {
            at = d.add(device)?;
            Ok(())
        })?;
        Ok(at)
    }

    /// 外す（確認なしですぐ。設定は覚える）
    pub fn output_remove(&self, slot: usize) -> AppResult<()> {
        self.update_outputs_reset(|d| d.remove(slot, db::now_ms()), Some(slot))
    }

    /// オン/オフ（2.4：出力ごとのミュートと同じもの。オフはデバイスを手放す）
    pub fn output_set_on(&self, slot: usize, on: bool) -> AppResult<()> {
        self.update_outputs(|d| {
            d.slot_mut(slot)?.on = on;
            Ok(())
        })
    }

    /// ショートカット「出力 n のオン/オフ」（7章）：空の枠・切断中の枠は何もしない。
    /// 開けません・重複の枠は切り替える（オンにしても鳴らない。13.5 の7）
    pub fn toggle_output(&self, slot: usize) {
        let cur = {
            let i = self.lock();
            let disconnected = i.out_rt.get(slot).and_then(|r| r.status) == Some(OutputStatus::Disconnected);
            match i.outputs.slot(slot) {
                Some(s) if !(s.on && disconnected) => Some(s.on),
                _ => None,
            }
        };
        if let Some(on) = cur {
            let _ = self.output_set_on(slot, !on);
        }
    }

    /// 出力ごとの音量（今のモードの組）。動かしている間は保存しない。ミュートは外さない（13.5 の9）
    pub fn output_set_volume(&self, slot: usize, v: f32) -> AppResult<()> {
        if !v.is_finite() {
            return Err(OpError::BadValue.into());
        }
        {
            let mut i = self.lock();
            let mode = i.mode;
            i.outputs.slot_mut(slot).map_err(AppError::from)?.volume.set(mode, v.clamp(0.0, 1.0));
            self.apply_gains(&i);
        }
        self.publish_outputs();
        Ok(())
    }

    /// つまみを離した：保存する
    pub fn output_commit_volume(&self) {
        self.save_outputs();
    }

    /// 全体の音量のつまみを離した（3.7.0 の playback_commit_volume）：状態と出力先の設定（全体の音量2組）を保存
    pub fn commit_volume(&self) {
        self.save_state();
        self.save_outputs();
    }

    /// 遅らせ量（0〜1000ms）。音声スレッドが約150ms 後にその出力だけ入り直す
    pub fn output_set_delay(&self, slot: usize, ms: u32) -> AppResult<()> {
        self.update_outputs(|d| d.set_delay(slot, ms))
    }

    /// 遅れを測った結果をまとめて入れる（v3.8.0）。`(枠, 測ったときのデバイスの設定, 遅らせ量)`。
    /// 測ってから入れるまでに、その枠のデバイスが差し替えられた・外された枠には入れない（別の機器の値になるため）。
    /// 入れた枠を返す。1つも入らなければ何も送らない
    pub fn output_set_delays(&self, items: &[(usize, Option<String>, u32)]) -> AppResult<Vec<usize>> {
        // 1つも入らないなら、音声スレッドへ送り直さない（試しに当てて確かめる）
        let would = self.lock().outputs.clone().apply_measured_delays(items).map_err(AppError::from)?;
        if would.is_empty() {
            return Ok(vec![]);
        }
        let mut applied = vec![];
        self.update_outputs(|d| {
            applied = d.apply_measured_delays(items)?;
            Ok(())
        })?;
        Ok(applied)
    }

    /// 遅れを測るときに見る、枠ごとの今の様子（v3.8.0）。状態の決め方は画面へ送るもの（`view_of`）と同じ
    pub fn measure_slots(&self) -> Vec<MeasureSlot> {
        let i = self.lock();
        let v = view_of(&i);
        v.slots
            .into_iter()
            .flatten()
            .map(|s| {
                let rt = &i.out_rt[s.slot];
                MeasureSlot {
                    slot: s.slot,
                    resolved_id: rt.resolved_id.clone().or_else(|| s.device_id.clone()),
                    device_id: s.device_id,
                    name: s.device_name.filter(|n| !n.is_empty()).unwrap_or(s.saved_name),
                    status: s.status,
                    delay_ms: s.delay_ms,
                }
            })
            .collect()
    }

    /// 曲を流しているか（遅れを測るのは、流していないときだけ。2.3 ）
    pub fn is_playing(&self) -> bool {
        self.lock().status == super::controller::Status::Playing
    }

    /// 全体の音量に従う（詳細でだけ効く）
    pub fn output_set_follow(&self, slot: usize, follow: bool) -> AppResult<()> {
        self.update_outputs(|d| {
            d.slot_mut(slot)?.follow_master = follow;
            Ok(())
        })
    }

    /// アイコンの選び直し（null で自動に戻す）
    pub fn output_set_icon(&self, slot: usize, icon: Option<String>) -> AppResult<()> {
        self.update_outputs(|d| d.set_icon(slot, icon))
    }

    /// デバイスの差し替え（6.2）。前のデバイスの設定は覚える
    pub fn output_set_device(&self, slot: usize, device: DeviceRef) -> AppResult<()> {
        // 同じデバイスを選び直しただけなら、音声スレッドは何も知らせ直さないので、枠の状態も消さない。
        // 実際に差し替えたときは、音声スレッドが必ず知らせ直す（同じ名前の別デバイスでも）
        let changes = self.lock().outputs.changes_device(slot, &device);
        let reset = changes.then_some(slot);
        self.update_outputs_reset(|d| d.replace_device(slot, device, db::now_ms()), reset)
    }

    /// 表示のモード（通常／詳細）を変える。音量の組をそのモードのものへ入れ替える（2.4）。
    /// ミュートは外さない。モードは DB の `mode` に保存する（再起動しても同じモードで開く。5章）
    pub fn set_mode(&self, m: Mode) {
        {
            let mut i = self.lock();
            if i.mode == m {
                return;
            }
            i.mode = m;
            i.volume = i.outputs.master.get(m);
            self.apply_gains(&i);
        }
        if let Err(e) = outputs::save_mode(&self.db, m) {
            tracing::warn!("save mode: {e}");
        }
        self.save_state();
        self.publish();
        self.publish_outputs();
    }

    /// ショートカット・グローバルホットキー「詳細表示の切替」（13.5 の11）
    pub fn toggle_detail(&self) {
        let m = self.mode();
        self.set_mode(m.toggled());
    }

    /// 設定の zip から読んだ出力先の設定（13.5 の9）：一時停止して、全部オフで入れる。表示のモードは変えない
    pub fn import_outputs(&self, doc: OutputsDoc) {
        self.pause();
        let r = self.update_outputs(|d| {
            *d = doc;
            d.all_off();
            Ok(())
        });
        if r.is_ok() {
            {
                let mut i = self.lock();
                i.volume = i.outputs.master.get(i.mode);
                i.out_rt = Default::default();
                self.apply_gains(&i);
            }
            self.save_state();
            self.publish();
            self.publish_outputs();
            self.notice(Notice::info("outputs_imported", None, None));
        }
    }
}
