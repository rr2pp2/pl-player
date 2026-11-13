//! skin.json を厳しく読む（v3.2.0）：同じキーが2つある・入れ子が深すぎる JSON は断る。
//! serde_json の `Value` へ読むと後のキーで黙って上書きされるので、読みながら確かめる。

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

/// 入れ子の深さの上限（配列・オブジェクト）
pub const MAX_DEPTH: usize = 16;

struct Strict {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for Strict {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict {
    type Value = Value;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("JSON")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v).map(Value::Number).ok_or_else(|| E::custom("bad number"))
    }
    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(A::Error::custom(format!("nested deeper than {MAX_DEPTH}")));
        }
        let mut out = Vec::new();
        while let Some(v) = seq.next_element_seed(Strict { depth: self.depth + 1 })? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(A::Error::custom(format!("nested deeper than {MAX_DEPTH}")));
        }
        let mut out = Map::new();
        while let Some(k) = map.next_key::<String>()? {
            if out.contains_key(&k) {
                // キー名はスキンの中から来た文字列なので、見えない文字・長さを `shown` で揃える
                return Err(A::Error::custom(format!("duplicate key: {}", super::manifest::from_skin(&k))));
            }
            let v = map.next_value_seed(Strict { depth: self.depth + 1 })?;
            out.insert(k, v);
        }
        Ok(Value::Object(out))
    }
}

/// UTF-8 の JSON を読む。先頭の BOM は外す。後ろに余計なものがあれば断る
pub fn parse_strict(bytes: &[u8]) -> Result<Value, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())?;
    let mut de = serde_json::Deserializer::from_str(text);
    let v = Strict { depth: 0 }.deserialize(&mut de).map_err(|e| e.to_string())?;
    de.end().map_err(|e| e.to_string())?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_json() {
        assert_eq!(parse_strict(br#"{"a":1,"b":[true,null,"x"]}"#).unwrap()["b"][2], "x");
        assert!(parse_strict(b"\xEF\xBB\xBF{}").is_ok());
        assert!(parse_strict(br#"{"a":1,"a":2}"#).unwrap_err().contains("duplicate"));
        assert!(parse_strict(br#"{"x":{"a":1,"a":2}}"#).unwrap_err().contains("duplicate"));
        assert!(parse_strict(br#"{"a":1} x"#).is_err());
        assert!(parse_strict(br#"{"a":"#).is_err());
        assert!(parse_strict(b"\xff\xfe{}").is_err());
        let deep = format!("{}{}", "[".repeat(17), "]".repeat(17));
        assert!(parse_strict(deep.as_bytes()).unwrap_err().contains("deeper"));
        let ok = format!("{}{}", "[".repeat(16), "]".repeat(16));
        assert!(parse_strict(ok.as_bytes()).is_ok());
    }
}
