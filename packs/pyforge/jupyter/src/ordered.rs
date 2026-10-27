//! JSON values that keep the key order of the text they were read from, for round-trip
//! stable notebooks (serde_json's own maps are sorted unless the workspace-wide
//! `preserve_order` feature is on, which other crates must not get).

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde_json::{Number, Value};

/// A JSON value with ordered objects.
#[derive(Debug, Clone, PartialEq)]
pub enum OValue {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<OValue>),
    Object(OMap),
}

/// An object as its key-value pairs in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OMap(pub Vec<(String, OValue)>);

impl OMap {
    pub fn get(&self, key: &str) -> Option<&OValue> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Replace the value of `key` in place, or append it.
    pub fn insert(&mut self, key: &str, value: OValue) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value,
            None => self.0.push((key.to_string(), value)),
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.0.retain(|(k, _)| k != key);
    }

    pub fn keys_sorted(&self) -> bool {
        self.0.windows(2).all(|w| w[0].0 <= w[1].0)
    }

    pub fn sort(&mut self) {
        self.0.sort_by(|a, b| a.0.cmp(&b.0));
    }
}

impl OValue {
    pub fn as_object(&self) -> Option<&OMap> {
        match self {
            OValue::Object(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<OValue>> {
        match self {
            OValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            OValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            OValue::Number(n) => n.as_u64(),
            _ => None,
        }
    }

    /// The same value as a `serde_json::Value` (order is lost).
    pub fn to_value(&self) -> Value {
        match self {
            OValue::Null => Value::Null,
            OValue::Bool(b) => Value::Bool(*b),
            OValue::Number(n) => Value::Number(n.clone()),
            OValue::String(s) => Value::String(s.clone()),
            OValue::Array(a) => Value::Array(a.iter().map(OValue::to_value).collect()),
            OValue::Object(m) => {
                Value::Object(m.0.iter().map(|(k, v)| (k.clone(), v.to_value())).collect())
            }
        }
    }
}

impl From<&Value> for OValue {
    fn from(v: &Value) -> Self {
        match v {
            Value::Null => OValue::Null,
            Value::Bool(b) => OValue::Bool(*b),
            Value::Number(n) => OValue::Number(n.clone()),
            Value::String(s) => OValue::String(s.clone()),
            Value::Array(a) => OValue::Array(a.iter().map(OValue::from).collect()),
            Value::Object(m) => OValue::Object(OMap(
                m.iter()
                    .map(|(k, v)| (k.clone(), OValue::from(v)))
                    .collect(),
            )),
        }
    }
}

impl Serialize for OValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            OValue::Null => s.serialize_unit(),
            OValue::Bool(b) => s.serialize_bool(*b),
            OValue::Number(n) => n.serialize(s),
            OValue::String(t) => s.serialize_str(t),
            OValue::Array(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            OValue::Object(m) => {
                let mut map = s.serialize_map(Some(m.0.len()))?;
                for (k, v) in &m.0 {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

struct OVisitor;

impl<'de> Visitor<'de> for OVisitor {
    type Value = OValue;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<OValue, E> {
        Ok(OValue::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<OValue, E> {
        Ok(OValue::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<OValue, E> {
        Ok(OValue::Number(v.into()))
    }

    fn visit_f64<E>(self, v: f64) -> Result<OValue, E> {
        Ok(Number::from_f64(v).map_or(OValue::Null, OValue::Number))
    }

    fn visit_str<E>(self, v: &str) -> Result<OValue, E> {
        Ok(OValue::String(v.to_string()))
    }

    fn visit_string<E>(self, v: String) -> Result<OValue, E> {
        Ok(OValue::String(v))
    }

    fn visit_unit<E>(self) -> Result<OValue, E> {
        Ok(OValue::Null)
    }

    fn visit_none<E>(self) -> Result<OValue, E> {
        Ok(OValue::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<OValue, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element()? {
            out.push(v);
        }
        Ok(OValue::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<OValue, A::Error> {
        let mut out = OMap::default();
        while let Some((k, v)) = map.next_entry::<String, OValue>()? {
            out.insert(&k, v);
        }
        Ok(OValue::Object(out))
    }
}

impl<'de> Deserialize<'de> for OValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<OValue, D::Error> {
        d.deserialize_any(OVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_key_order() {
        let v: OValue =
            serde_json::from_str(r#"{"z": 1, "a": [true, null, "x", 1.5], "m": {}}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"z":1,"a":[true,null,"x",1.5],"m":{}}"#
        );
        let m = v.as_object().unwrap();
        assert!(!m.keys_sorted());
        assert_eq!(v.to_value()["z"], 1);
    }
}
