//! 設定の保存（store）と、書き出し・読み込み（backup）。

pub mod backup;
pub mod store;
pub(crate) mod zip;

/// テスト用：zip と同じ CRC-32（PNG の CRC もこれ）
#[cfg(test)]
pub(crate) fn test_crc32(data: &[u8]) -> u32 {
    zip::crc32(data)
}
