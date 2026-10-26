//! The protobuf wire format, as far as `profile.proto` needs it: varints, length-delimited
//! fields (messages, strings, packed repeated numbers) and skipping of 32 / 64-bit fields.

/// A field of a message: its number and value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed64(u64),
    Fixed32(u32),
}

impl Value<'_> {
    pub fn as_u64(&self) -> u64 {
        match *self {
            Value::Varint(v) | Value::Fixed64(v) => v,
            Value::Fixed32(v) => v as u64,
            Value::Bytes(_) => 0,
        }
    }

    pub fn as_i64(&self) -> i64 {
        self.as_u64() as i64
    }
}

/// Reads the fields of one message.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

pub fn varint(data: &[u8], pos: &mut usize) -> Result<u64, String> {
    let mut out: u64 = 0;
    let mut shift = 0;
    loop {
        let b = *data
            .get(*pos)
            .ok_or_else(|| "truncated varint".to_string())?;
        *pos += 1;
        if shift >= 64 {
            return Err("varint too long".into());
        }
        out |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(out);
        }
        shift += 7;
    }
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    /// The next field, `None` at the end.
    pub fn next_field(&mut self) -> Result<Option<(u32, Value<'a>)>, String> {
        if self.pos >= self.data.len() {
            return Ok(None);
        }
        let key = varint(self.data, &mut self.pos)?;
        let field = (key >> 3) as u32;
        let value = match key & 7 {
            0 => Value::Varint(varint(self.data, &mut self.pos)?),
            1 => {
                let b = self
                    .data
                    .get(self.pos..self.pos + 8)
                    .ok_or("truncated fixed64")?;
                self.pos += 8;
                Value::Fixed64(u64::from_le_bytes(b.try_into().expect("8 bytes")))
            }
            2 => {
                let len = varint(self.data, &mut self.pos)? as usize;
                let end = self
                    .pos
                    .checked_add(len)
                    .filter(|e| *e <= self.data.len())
                    .ok_or("truncated length-delimited field")?;
                let b = &self.data[self.pos..end];
                self.pos = end;
                Value::Bytes(b)
            }
            5 => {
                let b = self
                    .data
                    .get(self.pos..self.pos + 4)
                    .ok_or("truncated fixed32")?;
                self.pos += 4;
                Value::Fixed32(u32::from_le_bytes(b.try_into().expect("4 bytes")))
            }
            w => return Err(format!("unsupported wire type {w}")),
        };
        Ok(Some((field, value)))
    }
}

/// Numbers of a repeated field: packed (one length-delimited field) or one per field.
pub fn push_numbers(value: Value<'_>, out: &mut Vec<u64>) -> Result<(), String> {
    match value {
        Value::Bytes(b) => {
            let mut pos = 0;
            while pos < b.len() {
                out.push(varint(b, &mut pos)?);
            }
        }
        other => out.push(other.as_u64()),
    }
    Ok(())
}

/// Writing (the profile builder of tests and the demo profile).
pub mod write {
    pub fn varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    pub fn key(out: &mut Vec<u8>, field: u32, wire: u8) {
        varint(out, (u64::from(field) << 3) | u64::from(wire));
    }

    pub fn uint(out: &mut Vec<u8>, field: u32, v: u64) {
        if v != 0 {
            key(out, field, 0);
            varint(out, v);
        }
    }

    pub fn bytes(out: &mut Vec<u8>, field: u32, b: &[u8]) {
        key(out, field, 2);
        varint(out, b.len() as u64);
        out.extend_from_slice(b);
    }

    pub fn packed(out: &mut Vec<u8>, field: u32, values: &[u64]) {
        let mut b = Vec::new();
        for v in values {
            varint(&mut b, *v);
        }
        bytes(out, field, &b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_fields() {
        let mut b = Vec::new();
        write::uint(&mut b, 1, 300);
        write::bytes(&mut b, 2, b"hi");
        write::packed(&mut b, 3, &[1, 128, 2]);
        b.extend_from_slice(&[(4 << 3) | 1, 1, 0, 0, 0, 0, 0, 0, 0, (5 << 3) | 5, 2, 0, 0, 0]);
        let mut r = Reader::new(&b);
        assert_eq!(r.next_field().unwrap(), Some((1, Value::Varint(300))));
        assert_eq!(r.next_field().unwrap(), Some((2, Value::Bytes(b"hi"))));
        let (_, v) = r.next_field().unwrap().unwrap();
        let mut nums = Vec::new();
        push_numbers(v, &mut nums).unwrap();
        assert_eq!(nums, [1, 128, 2]);
        assert_eq!(r.next_field().unwrap(), Some((4, Value::Fixed64(1))));
        assert_eq!(r.next_field().unwrap(), Some((5, Value::Fixed32(2))));
        assert_eq!(r.next_field().unwrap(), None);
        assert!(Reader::new(&[0x12, 5, 1]).next_field().is_err());
        assert!(Reader::new(&[0x80]).next_field().is_err());
    }
}
