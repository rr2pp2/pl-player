//! 出力先の設定（v3.8.0。仕様「同時再生と配信モード」2章・6.2・6.3・8章・13.5 の8〜11）。
//!
//! - DB の settings 表のキー `outputs` に JSON で置く。**書くのは Rust だけ**（画面は細かい操作のコマンドを送るだけ）。
//!   AppSettings には入れない（画面が設定を丸ごと保存する作りなので、Rust が変えた出力の状態を古い値で上書きしうる）
//! - 表示のモード（通常／詳細）は、さらに別のキー `mode`。設定の zip には入れない
//! - 枠は3つ（番号は固定。空の枠がありうる）。デバイスの鍵は cpal の ID の文字列、既定のデバイスは ID 無し（覚えの鍵は `default`）
//! - 音量の設定一式は2組（通常用・詳細用）。全体の音量と、出力ごとの音量。「全体の音量に従う」は詳細でだけ効く
//!   （通常ではいつも従う。2.4）。ミュートは今までどおり `playback_state`
//! - 外した出力先の覚え：デバイスごと、新しく外したものから 32 件・期限なし。同じデバイスを足したら戻して消す
//!
//! ここは純粋な形と決まりだけ（DB の読み書き以外は、デバイスにも音声スレッドにも触らない）

use serde::{Deserialize, Serialize};

use crate::audio::{OutputSpec, OutputSpecs, SLOTS};
use crate::db::{self, state::SavedPlayback, Db};
use crate::error::{AppError, AppResult, ErrorKind};

pub const OUTPUTS_KEY: &str = "outputs";
pub const MODE_KEY: &str = "mode";
/// 形の版（zip の outputs.json もこれ。知らない新しい版は断る）
pub const OUTPUTS_VERSION: u32 = 1;
/// 外した出力先を覚える数（13.5 の10。暫定）
pub const REMEMBER_MAX: usize = 32;
pub const NAME_MAX_CHARS: usize = 256;
/// zip の outputs.json の大きさの上限。3枠＋覚え 32 件を、名前と ID を上限の長さにして
/// 整形して書いても 150KB ほど。その数倍の余裕
pub const OUTPUTS_JSON_MAX_BYTES: usize = 1 << 20;
pub const ID_MAX_CHARS: usize = 512;
pub const DELAY_MAX_MS: u32 = crate::audio::sync::tuning::DELAY_MAX_MS;
/// 選び直せるアイコン（4.3）。無い（null）なら Windows が教える機器の種類から
pub const ICONS: &[&str] = &["headphones", "speaker", "stream", "monitor"];
/// 既定のデバイスの覚えの鍵
pub const DEFAULT_KEY: &str = "default";

/// 表示のモード（通常／詳細）。コンパクトは表示の設定（AppSettings）の側なので、ここでは通常と同じ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Normal,
    Detail,
}

impl Mode {
    pub fn toggled(self) -> Self {
        match self {
            Mode::Normal => Mode::Detail,
            Mode::Detail => Mode::Normal,
        }
    }
}

/// 通常用・詳細用の2組
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerMode {
    pub normal: f32,
    pub detail: f32,
}

impl PerMode {
    pub fn both(v: f32) -> Self {
        Self { normal: v, detail: v }
    }
    pub fn get(&self, m: Mode) -> f32 {
        match m {
            Mode::Normal => self.normal,
            Mode::Detail => self.detail,
        }
    }
    pub fn set(&mut self, m: Mode, v: f32) {
        match m {
            Mode::Normal => self.normal = v,
            Mode::Detail => self.detail = v,
        }
    }
}

impl Default for PerMode {
    fn default() -> Self {
        Self::both(1.0)
    }
}

/// どのデバイスか。名前は切断中の行に出すためだけ（比べない。13.5 の8）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceRef {
    /// None なら既定のデバイス（Windows に従う）
    pub id: Option<String>,
    pub name: String,
}

impl DeviceRef {
    pub fn key(&self) -> String {
        device_key(self.id.as_deref())
    }
}

pub fn device_key(id: Option<&str>) -> String {
    id.unwrap_or(DEFAULT_KEY).to_string()
}

fn default_true() -> bool {
    true
}

/// 枠1つ
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub device: DeviceRef,
    #[serde(default)]
    pub on: bool,
    #[serde(default)]
    pub delay_ms: u32,
    /// 選び直したアイコン。null なら自動
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub volume: PerMode,
    /// 全体の音量に従う（詳細でだけ効く）
    #[serde(default = "default_true")]
    pub follow_master: bool,
}

impl Slot {
    /// 足した直後（6.3）：オフ・音量 100%・全体の音量に従う
    pub fn fresh(device: DeviceRef) -> Self {
        Self { device, on: false, delay_ms: 0, icon: None, volume: PerMode::both(1.0), follow_master: true }
    }
}

/// 外した出力先の覚え（13.5 の10。オン/オフは覚えない）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remembered {
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub delay_ms: u32,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub volume: PerMode,
    #[serde(default = "default_true")]
    pub follow_master: bool,
    #[serde(default)]
    pub removed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputsDoc {
    #[serde(default = "version_one")]
    pub version: u32,
    /// 枠3つ（空の枠は null）
    #[serde(default)]
    pub slots: Vec<Option<Slot>>,
    /// 全体の音量（通常用・詳細用）
    #[serde(default)]
    pub master: PerMode,
    #[serde(default)]
    pub remembered: Vec<Remembered>,
}

fn version_one() -> u32 {
    OUTPUTS_VERSION
}

impl Default for OutputsDoc {
    fn default() -> Self {
        Self { version: OUTPUTS_VERSION, slots: vec![None; SLOTS], master: PerMode::both(0.8), remembered: vec![] }
    }
}

/// 出力の操作を断った理由
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpError {
    /// 出力先は3つまで
    Full,
    /// ほかの出力先で使っているデバイス
    Duplicate,
    /// 空の枠・範囲の外
    NoSlot,
    /// 知らないアイコン・範囲の外の値
    BadValue,
}

impl From<OpError> for AppError {
    fn from(e: OpError) -> Self {
        let detail = match e {
            OpError::Full => "outputs are limited to 3",
            OpError::Duplicate => "the device is already used by another output",
            OpError::NoSlot => "no such output",
            OpError::BadValue => "invalid value",
        };
        AppError::new(ErrorKind::OutputInvalid, detail)
    }
}

fn clamp_vol(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn icon_ok(i: &Option<String>) -> Option<String> {
    i.as_ref().filter(|x| ICONS.contains(&x.as_str())).cloned()
}

impl OutputsDoc {
    /// 3.7.0 から（13.5 の9）：今のデバイス（無ければ既定）を出力先1・オンで入れ、今の音量を通常用・詳細用の両方の全体の音量に
    pub fn migrate_from(saved: &SavedPlayback) -> Self {
        let vol = if saved.volume.is_finite() { (saved.volume as f32).clamp(0.0, 1.0) } else { 0.8 };
        let mut d = OutputsDoc { master: PerMode::both(vol), ..Default::default() };
        let id = saved.device_id.clone().filter(|s| !s.trim().is_empty());
        let mut slot = Slot::fresh(DeviceRef { id, name: String::new() });
        slot.on = true;
        d.slots[0] = Some(slot);
        d
    }

    /// DB から読んだ値を整える（読めない値は直す。断らない）。zip の検査は `from_zip`
    pub fn sanitize(mut self) -> Self {
        self.version = OUTPUTS_VERSION;
        self.slots.resize(SLOTS, None);
        self.slots.truncate(SLOTS);
        let mut seen: Vec<String> = vec![];
        for s in self.slots.iter_mut() {
            let dup = match s {
                Some(x) => {
                    let k = x.device.key();
                    let d = seen.contains(&k);
                    seen.push(k);
                    d
                }
                None => false,
            };
            if dup {
                *s = None;
            }
            if let Some(x) = s.as_mut() {
                x.delay_ms = x.delay_ms.min(DELAY_MAX_MS);
                x.volume.normal = clamp_vol(x.volume.normal);
                x.volume.detail = clamp_vol(x.volume.detail);
                x.icon = icon_ok(&x.icon);
                x.device.name = truncate_chars(&x.device.name, NAME_MAX_CHARS);
                if x.device.id.as_deref().is_some_and(|i| i.trim().is_empty()) {
                    x.device.id = None;
                }
            }
        }
        self.master.normal = clamp_vol(self.master.normal);
        self.master.detail = clamp_vol(self.master.detail);
        let mut keys: Vec<String> = vec![];
        self.remembered.retain(|r| !r.key.is_empty() && !keys.contains(&r.key) && {
            keys.push(r.key.clone());
            true
        });
        self.remembered.truncate(REMEMBER_MAX);
        for r in self.remembered.iter_mut() {
            r.delay_ms = r.delay_ms.min(DELAY_MAX_MS);
            r.volume.normal = clamp_vol(r.volume.normal);
            r.volume.detail = clamp_vol(r.volume.detail);
            r.icon = icon_ok(&r.icon);
            r.name = truncate_chars(&r.name, NAME_MAX_CHARS);
        }
        self
    }

    /// 設定の zip の outputs.json を読む（13.5 の9）。**全部を確かめ、どれかが外れたら断る**（zip 全体を断る）。
    /// 確かめる値：形の版・枠は3つまで・同じデバイスが2つの枠に無い・遅らせ量 0〜1000・音量 0〜1・覚えは32件まで・
    /// 名前と ID の長さ。知らないアイコンは自動（null）に読み替える。読み込んだ出力先は全部オフにする
    pub fn from_zip(data: &[u8]) -> Result<Self, String> {
        if data.len() > OUTPUTS_JSON_MAX_BYTES {
            return Err(format!("too large ({} bytes)", data.len()));
        }
        let d: OutputsDoc = serde_json::from_slice(data).map_err(|e| e.to_string())?;
        if d.version == 0 || d.version > OUTPUTS_VERSION {
            return Err(format!("version {}", d.version));
        }
        if d.slots.len() > SLOTS {
            return Err(format!("{} slots", d.slots.len()));
        }
        if d.remembered.len() > REMEMBER_MAX {
            return Err(format!("{} remembered outputs", d.remembered.len()));
        }
        let vol_ok = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        let per_ok = |p: &PerMode| vol_ok(p.normal) && vol_ok(p.detail);
        if !per_ok(&d.master) {
            return Err("master volume out of range".into());
        }
        let id_ok = |id: &Option<String>| match id {
            None => true,
            Some(s) => !s.trim().is_empty() && s.chars().count() <= ID_MAX_CHARS,
        };
        let mut seen: Vec<String> = vec![];
        for s in d.slots.iter().flatten() {
            if !id_ok(&s.device.id) || s.device.name.chars().count() > NAME_MAX_CHARS {
                return Err("device id or name too long".into());
            }
            let k = s.device.key();
            if seen.contains(&k) {
                return Err("the same device in two slots".into());
            }
            seen.push(k);
            if s.delay_ms > DELAY_MAX_MS || !per_ok(&s.volume) {
                return Err("delay or volume out of range".into());
            }
        }
        for r in &d.remembered {
            if r.key.is_empty() || r.key.chars().count() > ID_MAX_CHARS || r.name.chars().count() > NAME_MAX_CHARS {
                return Err("remembered key or name invalid".into());
            }
            if r.delay_ms > DELAY_MAX_MS || !per_ok(&r.volume) {
                return Err("remembered delay or volume out of range".into());
            }
        }
        let mut d = d.sanitize();
        for s in d.slots.iter_mut().flatten() {
            s.on = false;
        }
        Ok(d)
    }

    pub fn slot(&self, i: usize) -> Option<&Slot> {
        self.slots.get(i).and_then(|s| s.as_ref())
    }

    pub fn slot_mut(&mut self, i: usize) -> Result<&mut Slot, OpError> {
        self.slots.get_mut(i).and_then(|s| s.as_mut()).ok_or(OpError::NoSlot)
    }

    fn used_by_other(&self, key: &str, except: Option<usize>) -> bool {
        self.slots
            .iter()
            .enumerate()
            .any(|(i, s)| Some(i) != except && s.as_ref().is_some_and(|x| x.device.key() == key))
    }

    /// 覚えに入れる（同じ鍵は入れ替え、新しいものを先頭に、32 件まで）
    fn remember(&mut self, s: &Slot, now_ms: i64) {
        let key = s.device.key();
        self.remembered.retain(|r| r.key != key);
        self.remembered.insert(
            0,
            Remembered {
                key,
                name: s.device.name.clone(),
                delay_ms: s.delay_ms,
                icon: s.icon.clone(),
                volume: s.volume,
                follow_master: s.follow_master,
                removed_at: now_ms,
            },
        );
        self.remembered.truncate(REMEMBER_MAX);
    }

    /// 覚えから取り出す（取り出したら消す）
    fn take_remembered(&mut self, key: &str) -> Option<Remembered> {
        let i = self.remembered.iter().position(|r| r.key == key)?;
        Some(self.remembered.remove(i))
    }

    /// 出力先を足す（6.3）。空いているいちばん小さい番号の枠へ。前に覚えていれば戻す（オン/オフは覚えないのでオフ）
    pub fn add(&mut self, device: DeviceRef) -> Result<usize, OpError> {
        let key = device.key();
        if self.used_by_other(&key, None) {
            return Err(OpError::Duplicate);
        }
        let i = self.slots.iter().position(|s| s.is_none()).ok_or(OpError::Full)?;
        let mut s = Slot::fresh(device);
        if let Some(r) = self.take_remembered(&key) {
            s.delay_ms = r.delay_ms;
            s.icon = r.icon;
            s.volume = r.volume;
            s.follow_master = r.follow_master;
            if s.device.name.is_empty() {
                s.device.name = r.name;
            }
        }
        self.slots[i] = Some(s);
        Ok(i)
    }

    /// 出力先を外す（6.3）。確認なしですぐ外し、設定は覚えておく
    pub fn remove(&mut self, i: usize, now_ms: i64) -> Result<(), OpError> {
        let s = self.slots.get_mut(i).and_then(|s| s.take()).ok_or(OpError::NoSlot)?;
        self.remember(&s, now_ms);
        Ok(())
    }

    /// デバイスを差し替える（6.2・13.5 の10）：前のデバイスの設定を覚えに入れ、差し替え先を覚えていればその設定、
    /// 無ければ今の行の値を引き継ぐ。オン/オフは今の行のまま
    pub fn replace_device(&mut self, i: usize, device: DeviceRef, now_ms: i64) -> Result<(), OpError> {
        let key = device.key();
        let cur = self.slot(i).cloned().ok_or(OpError::NoSlot)?;
        if cur.device.key() == key {
            // 同じデバイス：名前だけ
            if !device.name.is_empty() {
                self.slot_mut(i)?.device.name = device.name;
            }
            return Ok(());
        }
        if self.used_by_other(&key, Some(i)) {
            return Err(OpError::Duplicate);
        }
        self.remember(&cur, now_ms);
        let mut s = cur.clone();
        s.device = device;
        if let Some(r) = self.take_remembered(&key) {
            s.delay_ms = r.delay_ms;
            s.icon = r.icon;
            s.volume = r.volume;
            s.follow_master = r.follow_master;
            if s.device.name.is_empty() {
                s.device.name = r.name;
            }
        }
        self.slots[i] = Some(s);
        Ok(())
    }

    /// `device` へ差し替えると、その枠のデバイスが実際に変わるか（同じデバイスを選び直しただけなら false）
    pub fn changes_device(&self, i: usize, device: &DeviceRef) -> bool {
        self.slot(i).is_some_and(|s| s.device.key() != device.key())
    }

    pub fn set_delay(&mut self, i: usize, ms: u32) -> Result<(), OpError> {
        if ms > DELAY_MAX_MS {
            return Err(OpError::BadValue);
        }
        self.slot_mut(i)?.delay_ms = ms;
        Ok(())
    }

    /// 遅れを測った結果をまとめて入れる（v3.8.0）。`(枠, 測ったときのデバイスの設定, 遅らせ量)`。
    /// 測ったときと同じデバイスの枠にだけ入れる（差し替えた・外した枠に、別の機器の値を入れない）。入れた枠を返す。
    /// 範囲の外の値が1つでもあれば、どれも入れずに断る
    pub fn apply_measured_delays(&mut self, items: &[(usize, Option<String>, u32)]) -> Result<Vec<usize>, OpError> {
        if items.iter().any(|(_, _, ms)| *ms > DELAY_MAX_MS) {
            return Err(OpError::BadValue);
        }
        let mut applied = vec![];
        for (i, id, ms) in items {
            if applied.contains(i) || !self.slot(*i).is_some_and(|s| &s.device.id == id) {
                continue;
            }
            self.slot_mut(*i)?.delay_ms = *ms;
            applied.push(*i);
        }
        Ok(applied)
    }

    pub fn set_icon(&mut self, i: usize, icon: Option<String>) -> Result<(), OpError> {
        if icon.as_ref().is_some_and(|x| !ICONS.contains(&x.as_str())) {
            return Err(OpError::BadValue);
        }
        self.slot_mut(i)?.icon = icon;
        Ok(())
    }

    /// 実際に掛ける倍率（2.4）：「全体 × 出力ごと」の2乗。ミュートは全体に従う出力だけに効く。
    /// 通常のモードでは、どの出力も全体に従う
    pub fn gain(&self, i: usize, mode: Mode, muted: bool) -> f32 {
        let Some(s) = self.slot(i) else { return 0.0 };
        let follow = mode == Mode::Normal || s.follow_master;
        if follow && muted {
            return 0.0;
        }
        let master = if follow { self.master.get(mode) } else { 1.0 };
        let v = (master * s.volume.get(mode)).clamp(0.0, 1.0);
        v * v
    }

    pub fn gains(&self, mode: Mode, muted: bool) -> [f32; SLOTS] {
        [self.gain(0, mode, muted), self.gain(1, mode, muted), self.gain(2, mode, muted)]
    }

    /// 音声スレッドへ渡す形
    pub fn specs(&self) -> OutputSpecs {
        let one = |i: usize| {
            self.slot(i).map(|s| OutputSpec { device_id: s.device.id.clone(), on: s.on, delay_ms: s.delay_ms })
        };
        [one(0), one(1), one(2)]
    }

    /// 全部オフにする（zip を読み込んだとき）
    pub fn all_off(&mut self) {
        for s in self.slots.iter_mut().flatten() {
            s.on = false;
        }
    }
}

/// DB から読む。無ければ 3.7.0 の `playback_state` から作って保存する（13.5 の9）。
/// 読めない・知らない新しい版なら、`playback_state` から作ったものを使うが、その場では書き戻さない
/// （新しい版から戻したときに、その版の設定をすぐ消さないため。出力先を変えたときに書く）
pub fn load_or_migrate(db: &Db, saved: &SavedPlayback) -> OutputsDoc {
    if let Ok(Some(v)) = db::settings::get(db, OUTPUTS_KEY) {
        match serde_json::from_value::<OutputsDoc>(v) {
            Ok(d) if d.version <= OUTPUTS_VERSION => return d.sanitize(),
            Ok(d) => tracing::warn!("outputs: unknown version {}; using the playback state", d.version),
            Err(e) => tracing::warn!("outputs: unreadable ({e}); using the playback state"),
        }
        return OutputsDoc::migrate_from(saved);
    }
    let d = OutputsDoc::migrate_from(saved);
    let _ = save(db, &d);
    d
}

pub fn save(db: &Db, d: &OutputsDoc) -> AppResult<()> {
    db::settings::set(db, OUTPUTS_KEY, &serde_json::to_value(d)?)
}

pub fn load_mode(db: &Db) -> Mode {
    db::settings::get(db, MODE_KEY)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<Mode>(v).ok())
        .unwrap_or_default()
}

pub fn save_mode(db: &Db, m: Mode) -> AppResult<()> {
    db::settings::set(db, MODE_KEY, &serde_json::to_value(m)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str) -> DeviceRef {
        DeviceRef { id: Some(id.into()), name: format!("テスト用デバイス {id}") }
    }

    fn saved(volume: f64, device: Option<&str>) -> SavedPlayback {
        SavedPlayback { volume, device_id: device.map(|s| s.to_string()), ..Default::default() }
    }

    /// 遅れを測った結果を入れる（v3.8.0）：測ったときと同じデバイスの枠にだけ。外した枠・差し替えた枠には入れない。
    /// 範囲の外の値があれば、どれも入れない
    #[test]
    fn measured_delays_go_only_to_the_same_device() {
        let mut d = OutputsDoc::migrate_from(&saved(0.8, Some("wasapi:{test-a}")));
        d.add(dev("wasapi:{test-b}")).unwrap();
        d.add(DeviceRef { id: None, name: String::new() }).unwrap();
        let items = vec![
            (0, Some("wasapi:{test-a}".to_string()), 33),
            (1, Some("wasapi:{test-x}".to_string()), 12),
            (2, None, 5),
        ];
        assert_eq!(d.apply_measured_delays(&items).unwrap(), vec![0, 2]);
        assert_eq!((d.slot(0).unwrap().delay_ms, d.slot(1).unwrap().delay_ms, d.slot(2).unwrap().delay_ms), (33, 0, 5));
        // 外した枠
        d.remove(1, 0).unwrap();
        assert_eq!(d.apply_measured_delays(&[(1, Some("wasapi:{test-b}".into()), 9)]).unwrap(), Vec::<usize>::new());
        // 範囲の外が混ざれば、どれも入れない
        assert!(d.apply_measured_delays(&[(0, Some("wasapi:{test-a}".into()), 1), (2, None, DELAY_MAX_MS + 1)]).is_err());
        assert_eq!(d.slot(0).unwrap().delay_ms, 33);
    }

    /// 3.7.0 から：今のデバイスが出力先1にオンで入り、今の音量が通常用・詳細用の両方の全体の音量に入る。
    /// デバイスが無ければ既定のデバイス。名前は分からない（切断中なら「前に使っていたデバイス」と出す）
    #[test]
    fn migrate_from_370() {
        let d = OutputsDoc::migrate_from(&saved(0.6, Some("wasapi:{test-a}")));
        let s = d.slot(0).unwrap();
        assert!(s.on);
        assert_eq!(s.device.id.as_deref(), Some("wasapi:{test-a}"));
        assert_eq!(s.device.name, "");
        assert_eq!(s.volume, PerMode::both(1.0));
        assert!(s.follow_master && s.delay_ms == 0 && s.icon.is_none());
        assert_eq!(d.master, PerMode::both(0.6));
        assert!(d.slot(1).is_none() && d.slot(2).is_none());
        assert_eq!(d.slots.len(), SLOTS);
        // 既定のデバイス・音量が数でない
        let d = OutputsDoc::migrate_from(&saved(f64::NAN, None));
        assert_eq!(d.slot(0).unwrap().device.id, None);
        assert_eq!(d.master, PerMode::both(0.8));
        // 1つだけオンで、3.7.0 と同じ大きさ（全体 × 100% の2乗）
        let d = OutputsDoc::migrate_from(&saved(0.5, None));
        assert_eq!(d.gains(Mode::Normal, false), [0.25, 0.0, 0.0]);
        assert_eq!(d.gains(Mode::Normal, true), [0.0, 0.0, 0.0]);
    }

    /// DB への保存と引き継ぎ：`outputs` が無ければ playback_state から作って保存し、2回目はそれを読む。
    /// 壊れていたら作り直す。`mode` は別のキー（既定は通常）
    #[test]
    fn load_or_migrate_and_mode_keys() {
        let db = Db::open_memory("outputs_migrate").unwrap();
        assert!(db::settings::get(&db, OUTPUTS_KEY).unwrap().is_none());
        let d = load_or_migrate(&db, &saved(0.7, Some("wasapi:{test-b}")));
        assert!(db::settings::get(&db, OUTPUTS_KEY).unwrap().is_some());
        let mut changed = d.clone();
        changed.slot_mut(0).unwrap().delay_ms = 120;
        save(&db, &changed).unwrap();
        // 2回目は保存したもの（playback_state の値では作り直さない）
        assert_eq!(load_or_migrate(&db, &saved(0.1, None)), changed);
        // 壊れている・知らない新しい版：playback_state から作るが、DB の値はその場では書き換えない
        let broken = serde_json::json!({"slots": "broken"});
        db::settings::set(&db, OUTPUTS_KEY, &broken).unwrap();
        assert_eq!(load_or_migrate(&db, &saved(0.3, None)).master, PerMode::both(0.3));
        assert_eq!(db::settings::get(&db, OUTPUTS_KEY).unwrap().unwrap(), broken);
        let newer = serde_json::json!({"version": 99, "slots": [], "master": {"normal": 0.1, "detail": 0.1}});
        db::settings::set(&db, OUTPUTS_KEY, &newer).unwrap();
        assert_eq!(load_or_migrate(&db, &saved(0.4, None)).master, PerMode::both(0.4));
        assert_eq!(db::settings::get(&db, OUTPUTS_KEY).unwrap().unwrap(), newer);
        assert_eq!(load_mode(&db), Mode::Normal);
        save_mode(&db, Mode::Detail).unwrap();
        assert_eq!(load_mode(&db), Mode::Detail);
        assert_eq!(db::settings::get(&db, MODE_KEY).unwrap().unwrap(), serde_json::json!("detail"));
        db::settings::set(&db, MODE_KEY, &serde_json::json!(5)).unwrap();
        assert_eq!(load_mode(&db), Mode::Normal);
    }

    /// 足した直後はオフ・100%・全体に従う。空いているいちばん小さい枠へ。3つまで。同じデバイスは入れない
    #[test]
    fn add_fills_free_slots_and_refuses_duplicates() {
        let mut d = OutputsDoc::migrate_from(&saved(0.8, None));
        assert_eq!(d.add(DeviceRef { id: None, name: String::new() }), Err(OpError::Duplicate));
        assert_eq!(d.add(dev("a")), Ok(1));
        let s = d.slot(1).unwrap();
        assert!(!s.on && s.volume == PerMode::both(1.0) && s.follow_master);
        assert_eq!(d.add(dev("a")), Err(OpError::Duplicate));
        assert_eq!(d.add(dev("b")), Ok(2));
        assert_eq!(d.add(dev("c")), Err(OpError::Full));
        d.remove(1, 10).unwrap();
        assert_eq!(d.add(dev("c")), Ok(1));
        assert_eq!(d.remove(1, 0), Ok(()));
        assert_eq!(d.remove(1, 0), Err(OpError::NoSlot));
    }

    /// 外した出力先の覚え：外すと覚え、同じデバイスを足すと戻して覚えから消す（オン/オフは戻さない）。
    /// 新しく外したものから 32 件まで、期限なし
    #[test]
    fn remembered_outputs_are_restored_and_capped_at_32() {
        let mut d = OutputsDoc::default();
        let i = d.add(dev("a")).unwrap();
        {
            let s = d.slot_mut(i).unwrap();
            s.on = true;
            s.delay_ms = 250;
            s.icon = Some("stream".into());
            s.volume = PerMode { normal: 0.4, detail: 0.9 };
            s.follow_master = false;
        }
        d.remove(i, 1_000).unwrap();
        assert_eq!(d.remembered.len(), 1);
        assert_eq!(d.remembered[0].key, "a");
        assert_eq!(d.remembered[0].removed_at, 1_000);
        let j = d.add(DeviceRef { id: Some("a".into()), name: String::new() }).unwrap();
        let s = d.slot(j).unwrap();
        assert_eq!((s.delay_ms, s.icon.as_deref(), s.volume, s.follow_master), (250, Some("stream"), PerMode { normal: 0.4, detail: 0.9 }, false));
        assert!(!s.on);
        // 名前が分からなければ覚えの名前
        assert_eq!(s.device.name, "テスト用デバイス a");
        assert!(d.remembered.is_empty());
        // 33 件外すと、いちばん古いものが消える
        let mut d = OutputsDoc::default();
        for n in 0..33 {
            let i = d.add(dev(&format!("dev{n}"))).unwrap();
            d.remove(i, n as i64).unwrap();
        }
        assert_eq!(d.remembered.len(), REMEMBER_MAX);
        assert_eq!(d.remembered[0].key, "dev32");
        assert!(d.remembered.iter().all(|r| r.key != "dev0"));
        // 同じデバイスを2回外しても、覚えは1件（新しいほう）
        let i = d.add(dev("dev5")).unwrap();
        d.slot_mut(i).unwrap().delay_ms = 7;
        d.remove(i, 99).unwrap();
        assert_eq!(d.remembered.iter().filter(|r| r.key == "dev5").count(), 1);
        assert_eq!(d.remembered[0].delay_ms, 7);
        // 既定のデバイスの鍵は default
        let i = d.add(DeviceRef::default()).unwrap();
        d.remove(i, 0).unwrap();
        assert_eq!(d.remembered[0].key, DEFAULT_KEY);
    }

    /// デバイスの差し替え：前のデバイスの設定を覚えに入れる。差し替え先を覚えていればその設定、無ければ今の行の値。
    /// 元のデバイスに戻すと値も戻る。オン/オフは今の行のまま。ほかの枠のデバイスへは差し替えない
    #[test]
    fn replace_device_remembers_the_previous_one() {
        let mut d = OutputsDoc::default();
        let i = d.add(dev("a")).unwrap();
        d.add(dev("b")).unwrap();
        {
            let s = d.slot_mut(i).unwrap();
            s.on = true;
            s.delay_ms = 40;
        }
        assert_eq!(d.replace_device(i, dev("b"), 0), Err(OpError::Duplicate));
        d.replace_device(i, dev("c"), 5).unwrap();
        let s = d.slot(i).unwrap();
        assert_eq!(s.device.id.as_deref(), Some("c"));
        assert_eq!(s.delay_ms, 40); // 覚えが無いので今の行の値
        assert!(s.on);
        assert_eq!(d.remembered[0].key, "a");
        d.slot_mut(i).unwrap().delay_ms = 900;
        d.replace_device(i, dev("a"), 6).unwrap();
        assert_eq!(d.slot(i).unwrap().delay_ms, 40); // a の値が戻る
        assert_eq!(d.remembered[0].key, "c");
        assert_eq!(d.remembered[0].delay_ms, 900);
        assert!(d.remembered.iter().all(|r| r.key != "a"));
        assert_eq!(d.replace_device(2, dev("x"), 0), Err(OpError::NoSlot));
    }

    /// ：同じデバイスを選び直しただけなら「差し替え」ではない（司令塔は枠の状態を消さない）。
    /// 名前が同じでも ID が違えば差し替え。既定のデバイス同士は同じ
    #[test]
    fn reselecting_the_same_device_is_not_a_change() {
        let mut d = OutputsDoc::default();
        let i = d.add(dev("a")).unwrap();
        assert!(!d.changes_device(i, &DeviceRef { id: Some("a".into()), name: "別の名前".into() }));
        assert!(d.changes_device(i, &DeviceRef { id: Some("b".into()), name: "テスト用デバイス a".into() }));
        assert!(d.changes_device(i, &DeviceRef::default()));
        let j = d.add(DeviceRef::default()).unwrap();
        assert!(!d.changes_device(j, &DeviceRef::default()));
        assert!(!d.changes_device(2, &dev("a")), "empty slot");
        // 選び直しても、設定は何も変わらない
        let before = d.clone();
        d.replace_device(i, DeviceRef { id: Some("a".into()), name: String::new() }, 9).unwrap();
        assert_eq!(d, before);
    }

    /// 実際の大きさ：通常では「全体 × 出力ごと」の2乗、ミュートは全部に効く。詳細では組が別で、
    /// 「全体の音量に従う」を切った出力は全体の音量とミュートで変わらない
    #[test]
    fn gains_by_mode_follow_and_mute() {
        let mut d = OutputsDoc::default();
        d.master = PerMode { normal: 0.8, detail: 0.5 };
        let i = d.add(dev("a")).unwrap();
        let s = d.slot_mut(i).unwrap();
        s.volume = PerMode { normal: 0.6, detail: 1.0 };
        s.follow_master = false;
        // 例：全体 80% × 出力 60% ＝ 48% の2乗。通常では「従う」を切っていても従う
        assert!((d.gain(i, Mode::Normal, false) - 0.48f32 * 0.48).abs() < 1e-6);
        assert_eq!(d.gain(i, Mode::Normal, true), 0.0);
        // 詳細：従わない → 全体 50% もミュートも効かない
        assert!((d.gain(i, Mode::Detail, false) - 1.0).abs() < 1e-6);
        assert!((d.gain(i, Mode::Detail, true) - 1.0).abs() < 1e-6);
        d.slot_mut(i).unwrap().follow_master = true;
        assert!((d.gain(i, Mode::Detail, false) - 0.25).abs() < 1e-6);
        assert_eq!(d.gain(i, Mode::Detail, true), 0.0);
        // 空の枠は 0
        assert_eq!(d.gain(2, Mode::Normal, false), 0.0);
    }

    fn zip_doc() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "slots": [
                {"device": {"id": "wasapi:{test-a}", "name": "テスト用ヘッドホン"}, "on": true, "delayMs": 30, "icon": "headphones", "volume": {"normal": 0.5, "detail": 0.7}, "followMaster": false},
                null,
                {"device": {"id": null, "name": ""}, "on": true, "delayMs": 0, "icon": "hologram", "volume": {"normal": 1.0, "detail": 1.0}, "followMaster": true}
            ],
            "master": {"normal": 0.8, "detail": 0.6},
            "remembered": [{"key": "wasapi:{test-z}", "name": "外したデバイス", "delayMs": 5, "icon": null, "volume": {"normal": 1.0, "detail": 1.0}, "followMaster": true, "removedAt": 1}]
        })
    }

    /// zip の outputs.json：全部オフで入れる。デバイス・音量・遅らせ量・アイコン・覚えは zip のとおり。知らないアイコンは自動
    #[test]
    fn zip_import_turns_everything_off() {
        let d = OutputsDoc::from_zip(&serde_json::to_vec(&zip_doc()).unwrap()).unwrap();
        assert!(d.slots.iter().flatten().all(|s| !s.on));
        let a = d.slot(0).unwrap();
        assert_eq!((a.delay_ms, a.icon.as_deref(), a.follow_master), (30, Some("headphones"), false));
        assert_eq!(a.volume, PerMode { normal: 0.5, detail: 0.7 });
        assert!(d.slot(1).is_none());
        assert_eq!(d.slot(2).unwrap().icon, None);
        assert_eq!(d.master, PerMode { normal: 0.8, detail: 0.6 });
        assert_eq!(d.remembered.len(), 1);
        // 書き出しの形のまま読み戻せる
        let again = OutputsDoc::from_zip(&serde_json::to_vec(&d).unwrap()).unwrap();
        assert_eq!(again, d);
    }

    /// zip の outputs.json の検査：外れた値が1つでもあれば断る
    #[test]
    fn zip_import_rejects_bad_values() {
        let bad = |f: &dyn Fn(&mut serde_json::Value)| {
            let mut v = zip_doc();
            f(&mut v);
            OutputsDoc::from_zip(&serde_json::to_vec(&v).unwrap())
        };
        assert!(bad(&|_| {}).is_ok());
        assert!(bad(&|v| v["slots"].as_array_mut().unwrap().push(serde_json::Value::Null)).is_err(), "4 slots");
        assert!(bad(&|v| v["slots"][1] = v["slots"][0].clone()).is_err(), "same device twice");
        assert!(bad(&|v| { v["slots"][1] = v["slots"][2].clone(); }).is_err(), "default twice");
        assert!(bad(&|v| v["slots"][0]["delayMs"] = 1001.into()).is_err(), "delay");
        assert!(bad(&|v| v["slots"][0]["delayMs"] = (-1).into()).is_err(), "negative delay");
        assert!(bad(&|v| v["slots"][0]["volume"]["detail"] = 1.5.into()).is_err(), "volume");
        assert!(bad(&|v| v["master"]["normal"] = (-0.1).into()).is_err(), "master");
        assert!(bad(&|v| v["slots"][0]["device"]["name"] = "あ".repeat(NAME_MAX_CHARS + 1).into()).is_err(), "name");
        assert!(bad(&|v| v["slots"][0]["device"]["id"] = "x".repeat(ID_MAX_CHARS + 1).into()).is_err(), "id");
        assert!(bad(&|v| v["slots"][0]["device"]["id"] = " ".into()).is_err(), "blank id");
        assert!(bad(&|v| v["version"] = 2.into()).is_err(), "version");
        assert!(bad(&|v| v["slots"][0]["on"] = "yes".into()).is_err(), "type");
        assert!(bad(&|v| {
            let r = v["remembered"][0].clone();
            v["remembered"] = serde_json::Value::Array(vec![r; REMEMBER_MAX + 1]);
        })
        .is_err(), "33 remembered");
        assert!(bad(&|v| v["remembered"][0]["delayMs"] = 5000.into()).is_err(), "remembered delay");
        assert!(OutputsDoc::from_zip(b"{").is_err());
        // 大きすぎる outputs.json は、ばらす前に断る（覚えを大量に並べたもの）
        let mut huge = br#"{"version":1,"remembered":["#.to_vec();
        while huge.len() <= OUTPUTS_JSON_MAX_BYTES {
            huge.extend_from_slice(br#"{"key":"k"},"#);
        }
        huge.extend_from_slice(br#"{"key":"z"}]}"#);
        let e = OutputsDoc::from_zip(&huge).unwrap_err();
        assert!(e.starts_with("too large"), "{e}");
        // 上限いっぱいの名前・ID・覚え 32 件を整形して書いても、上限の中に収まる
        let mut v = zip_doc();
        let long = |c: &str, n: usize| c.repeat(n);
        v["slots"][0]["device"]["id"] = long("鍵", ID_MAX_CHARS).into();
        v["slots"][0]["device"]["name"] = long("名", NAME_MAX_CHARS).into();
        let mut list = vec![];
        for n in 0..REMEMBER_MAX {
            let mut r = v["remembered"][0].clone();
            r["key"] = format!("{n}{}", long("鍵", ID_MAX_CHARS - 3)).into();
            r["name"] = long("名", NAME_MAX_CHARS).into();
            list.push(r);
        }
        v["remembered"] = serde_json::Value::Array(list);
        let bytes = serde_json::to_vec_pretty(&v).unwrap();
        assert!(bytes.len() < OUTPUTS_JSON_MAX_BYTES / 4, "{}", bytes.len());
        assert!(OutputsDoc::from_zip(&bytes).is_ok());
        // 名前ちょうどの長さ・覚えちょうど 32 件は通る
        assert!(bad(&|v| v["slots"][0]["device"]["name"] = "あ".repeat(NAME_MAX_CHARS).into()).is_ok());
        assert!(bad(&|v| {
            let mut list = vec![];
            for n in 0..REMEMBER_MAX {
                let mut r = v["remembered"][0].clone();
                r["key"] = format!("k{n}").into();
                list.push(r);
            }
            v["remembered"] = serde_json::Value::Array(list);
        })
        .is_ok());
    }

    /// 音声スレッドへ渡す形：空の枠は None、オン/オフ・遅らせ量・デバイス
    #[test]
    fn specs_for_the_audio_thread() {
        let mut d = OutputsDoc::migrate_from(&saved(0.8, Some("x")));
        d.add(dev("y")).unwrap();
        d.set_delay(1, 300).unwrap();
        assert_eq!(d.set_delay(1, 1001), Err(OpError::BadValue));
        assert_eq!(d.set_icon(1, Some("hologram".into())), Err(OpError::BadValue));
        d.set_icon(1, Some("monitor".into())).unwrap();
        let s = d.specs();
        assert_eq!(s[0], Some(OutputSpec { device_id: Some("x".into()), on: true, delay_ms: 0 }));
        assert_eq!(s[1], Some(OutputSpec { device_id: Some("y".into()), on: false, delay_ms: 300 }));
        assert_eq!(s[2], None);
    }
}
