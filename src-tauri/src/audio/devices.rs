//! 出力デバイスの一覧（cpal / WASAPI）。

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// 安定した ID（cpal の DeviceId を文字にしたもの）
    pub id: String,
    pub name: String,
    pub is_default: bool,
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
                id,
            });
        }
    }
    out
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
