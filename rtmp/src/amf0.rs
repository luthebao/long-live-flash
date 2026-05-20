//! Minimal AMF0 encoder + decoder. Just the markers the RTMP `connect`
//! command and its `_result` reply use: Number, Boolean, String, Object,
//! Null, Undefined, ECMA Array, Strict Array, Long String.
//!
//! Spec: Adobe AMF0 spec 2007, §2.x.
//!
//! Mirrors `src/amf0.odin`.

use crate::RtmpError;

const AMF0_NUMBER: u8 = 0x00;
const AMF0_BOOLEAN: u8 = 0x01;
const AMF0_STRING: u8 = 0x02;
const AMF0_OBJECT: u8 = 0x03;
const AMF0_NULL: u8 = 0x05;
const AMF0_UNDEFINED: u8 = 0x06;
const AMF0_ECMA_ARRAY: u8 = 0x08;
const AMF0_OBJECT_END: u8 = 0x09;
const AMF0_STRICT_ARRAY: u8 = 0x0a;
const AMF0_LONG_STRING: u8 = 0x0c;

#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    Bool(bool),
    String(String),
    Object(Vec<(String, Value)>),
    EcmaArray(Vec<(String, Value)>),
    StrictArray(Vec<Value>),
    Null,
    Undefined,
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        if let Value::String(s) = self { Some(s) } else { None }
    }
    #[allow(dead_code)] // public API: used by tests and future NetStream code
    pub fn as_number(&self) -> Option<f64> {
        if let Value::Number(n) = self { Some(*n) } else { None }
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(kv) | Value::EcmaArray(kv) => {
                kv.iter().find(|(k, _)| k == key).map(|(_, v)| v)
            }
            _ => None,
        }
    }
}

// --- encoder --------------------------------------------------------------

pub fn encode(buf: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Number(n) => {
            buf.push(AMF0_NUMBER);
            buf.extend_from_slice(&n.to_be_bytes());
        }
        Value::Bool(b) => {
            buf.push(AMF0_BOOLEAN);
            buf.push(if *b { 1 } else { 0 });
        }
        Value::String(s) => write_string(buf, s),
        Value::Object(kv) => {
            buf.push(AMF0_OBJECT);
            write_object_inner(buf, kv);
        }
        Value::EcmaArray(kv) => {
            buf.push(AMF0_ECMA_ARRAY);
            buf.extend_from_slice(&(kv.len() as u32).to_be_bytes());
            write_object_inner(buf, kv);
        }
        Value::StrictArray(items) => {
            buf.push(AMF0_STRICT_ARRAY);
            buf.extend_from_slice(&(items.len() as u32).to_be_bytes());
            for it in items {
                encode(buf, it);
            }
        }
        Value::Null => buf.push(AMF0_NULL),
        Value::Undefined => buf.push(AMF0_UNDEFINED),
    }
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    if bytes.len() > u16::MAX as usize {
        buf.push(AMF0_LONG_STRING);
        buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    } else {
        buf.push(AMF0_STRING);
        buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    }
    buf.extend_from_slice(bytes);
}

fn write_utf8(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn write_object_inner(buf: &mut Vec<u8>, kv: &[(String, Value)]) {
    for (k, v) in kv {
        write_utf8(buf, k);
        encode(buf, v);
    }
    buf.extend_from_slice(&[0x00, 0x00, AMF0_OBJECT_END]);
}

// --- decoder --------------------------------------------------------------

pub struct Cursor<'a> {
    pub buf: &'a [u8],
    pub off: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, off: 0 }
    }
    #[allow(dead_code)] // public API: used by tests
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.off)
    }
    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.off..]
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], RtmpError> {
        if self.off + n > self.buf.len() {
            return Err(RtmpError::AmfEof);
        }
        let slice = &self.buf[self.off..self.off + n];
        self.off += n;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, RtmpError> {
        Ok(self.take(1)?[0])
    }
    fn u16_be(&mut self) -> Result<u16, RtmpError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32_be(&mut self) -> Result<u32, RtmpError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f64_be(&mut self) -> Result<f64, RtmpError> {
        let b = self.take(8)?;
        Ok(f64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn utf8(&mut self) -> Result<String, RtmpError> {
        let n = self.u16_be()? as usize;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| RtmpError::AmfBadString)
    }
    fn long_utf8(&mut self) -> Result<String, RtmpError> {
        let n = self.u32_be()? as usize;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| RtmpError::AmfBadString)
    }
}

pub fn decode(cur: &mut Cursor<'_>) -> Result<Value, RtmpError> {
    let marker = cur.u8()?;
    match marker {
        AMF0_NUMBER => Ok(Value::Number(cur.f64_be()?)),
        AMF0_BOOLEAN => Ok(Value::Bool(cur.u8()? != 0)),
        AMF0_STRING => Ok(Value::String(cur.utf8()?)),
        AMF0_LONG_STRING => Ok(Value::String(cur.long_utf8()?)),
        AMF0_OBJECT => Ok(Value::Object(decode_object_inner(cur)?)),
        AMF0_ECMA_ARRAY => {
            let _count = cur.u32_be()?; // hint, ignored — read until OBJECT_END
            Ok(Value::EcmaArray(decode_object_inner(cur)?))
        }
        AMF0_STRICT_ARRAY => {
            let n = cur.u32_be()? as usize;
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(decode(cur)?);
            }
            Ok(Value::StrictArray(items))
        }
        AMF0_NULL => Ok(Value::Null),
        AMF0_UNDEFINED => Ok(Value::Undefined),
        AMF0_OBJECT_END => Err(RtmpError::AmfBadMarker(marker)),
        _ => Err(RtmpError::AmfBadMarker(marker)),
    }
}

fn decode_object_inner(cur: &mut Cursor<'_>) -> Result<Vec<(String, Value)>, RtmpError> {
    let mut out = Vec::new();
    loop {
        let key = cur.utf8()?;
        if key.is_empty() {
            let m = cur.u8()?;
            if m != AMF0_OBJECT_END {
                return Err(RtmpError::AmfBadMarker(m));
            }
            return Ok(out);
        }
        let val = decode(cur)?;
        out.push((key, val));
    }
}

// --- convenience ----------------------------------------------------------

pub fn object_kv<I>(pairs: I) -> Value
where
    I: IntoIterator<Item = (&'static str, Value)>,
{
    Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

pub fn s(v: &str) -> Value {
    Value::String(v.into())
}
pub fn n(v: f64) -> Value {
    Value::Number(v)
}
pub fn b(v: bool) -> Value {
    Value::Bool(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_string() {
        let mut buf = Vec::new();
        encode(&mut buf, &s("connect"));
        let mut c = Cursor::new(&buf);
        let v = decode(&mut c).unwrap();
        assert_eq!(v.as_str().unwrap(), "connect");
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn roundtrip_number() {
        let mut buf = Vec::new();
        encode(&mut buf, &n(1.0));
        let mut c = Cursor::new(&buf);
        let v = decode(&mut c).unwrap();
        assert_eq!(v.as_number().unwrap(), 1.0);
    }

    #[test]
    fn roundtrip_object() {
        let v = object_kv([
            ("app", s("live")),
            ("flashver", s("WIN 32,0,0,114")),
            ("fpad", b(false)),
            ("capabilities", n(239.0)),
        ]);
        let mut buf = Vec::new();
        encode(&mut buf, &v);
        let mut c = Cursor::new(&buf);
        let d = decode(&mut c).unwrap();
        assert_eq!(d.get("app").unwrap().as_str().unwrap(), "live");
        assert_eq!(d.get("capabilities").unwrap().as_number().unwrap(), 239.0);
    }
}
