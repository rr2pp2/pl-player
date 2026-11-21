//! 出力デバイスの一覧（cpal / WASAPI）。
//!
//! v3.8.0（仕様 13.5 の8）：デバイスの鍵は cpal の ID の文字列（`wasapi:{…}`）。名前では比べない。
//! 既定のデバイスは cpal と同じ Console の役割に従う（`default_output_device`）。

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// 安定した ID（cpal の DeviceId を文字にしたもの）
    pub id: String,
    pub name: String,
    pub is_default: bool,
    /// v3.8.0：Windows が教える機器の種類から決めた、自動のアイコン（headphones / speaker / stream）。仕様 4.3
    pub kind: &'static str,
}

pub fn host() -> cpal::Host {
    cpal::default_host()
}

pub fn device_id(dev: &cpal::Device) -> Option<String> {
    dev.id().ok().map(|i| i.to_string())
}

pub fn device_name(dev: &cpal::Device) -> String {
    dev.description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "不明なデバイス".to_string())
}

/// 機器の種類 → 自動のアイコン。種類が分からないもの（HDMI など）はスピーカー（仕様 4.3）
pub fn kind_icon(t: cpal::DeviceType) -> &'static str {
    use cpal::DeviceType as D;
    match t {
        D::Headphones | D::Headset | D::Earpiece | D::Handset | D::HearingAid => "headphones",
        D::Virtual => "stream",
        _ => "speaker",
    }
}

pub fn device_kind(dev: &cpal::Device) -> &'static str {
    dev.description().map(|d| kind_icon(d.device_type())).unwrap_or("speaker")
}

pub fn list_output_devices() -> Vec<DeviceInfo> {
    let host = host();
    let default_id = host.default_output_device().as_ref().and_then(device_id);
    let mut out = vec![];
    if let Ok(devs) = host.output_devices() {
        for d in devs {
            let Some(id) = device_id(&d) else { continue };
            out.push(DeviceInfo {
                is_default: Some(&id) == default_id.as_ref(),
                name: device_name(&d),
                kind: device_kind(&d),
                id,
            });
        }
    }
    out
}

/// 録音デバイスの一覧（v3.8.0。遅れを測るときの録る所。仮想ケーブルの出口など）
pub fn list_input_devices() -> Vec<DeviceInfo> {
    let host = host();
    let default_id = host.default_input_device().as_ref().and_then(device_id);
    let mut out = vec![];
    if let Ok(devs) = host.input_devices() {
        for d in devs {
            let Some(id) = device_id(&d) else { continue };
            out.push(DeviceInfo {
                is_default: Some(&id) == default_id.as_ref(),
                name: device_name(&d),
                kind: device_kind(&d),
                id,
            });
        }
    }
    out
}

/// ID で録音デバイスを探す
pub fn find_input(id: &str) -> Option<cpal::Device> {
    let host = host();
    if let Ok(parsed) = id.parse::<cpal::DeviceId>() {
        if let Some(d) = host.device_by_id(&parsed) {
            return Some(d);
        }
    }
    host.input_devices().ok()?.find(|d| device_id(d).as_deref() == Some(id))
}

/// 今の既定の出力デバイスの ID（Console の役割）
pub fn default_output_id() -> Option<String> {
    host().default_output_device().as_ref().and_then(device_id)
}

/// ID でデバイスを探す。None なら既定のデバイス。
pub fn find_output(id: Option<&str>) -> Option<cpal::Device> {
    let host = host();
    match id {
        None => host.default_output_device(),
        Some(id) => {
            if let Ok(parsed) = id.parse::<cpal::DeviceId>() {
                if let Some(d) = host.device_by_id(&parsed) {
                    return Some(d);
                }
            }
            host.output_devices()
                .ok()?
                .find(|d| device_id(d).as_deref() == Some(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自動のアイコン：ヘッドホンの仲間・仮想（配信）・それ以外はスピーカー
    #[test]
    fn icon_from_device_type() {
        assert_eq!(kind_icon(cpal::DeviceType::Headphones), "headphones");
        assert_eq!(kind_icon(cpal::DeviceType::Headset), "headphones");
        assert_eq!(kind_icon(cpal::DeviceType::Virtual), "stream");
        assert_eq!(kind_icon(cpal::DeviceType::Speaker), "speaker");
        assert_eq!(kind_icon(cpal::DeviceType::Unknown), "speaker");
    }
}
