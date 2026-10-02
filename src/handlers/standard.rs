//! The subset of Flutter's `StandardMessageCodec` that stock plugins'
//! Pigeon channels use: null, booleans, integers, strings, lists, maps and
//! Pigeon's custom types (an extra type byte before the encoded value).

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    /// A Pigeon type: its type byte (128 and up) and its encoded value.
    Custom(u8, Box<Value>),
}

impl Value {
    pub fn str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }
    pub fn list(&self) -> Option<&[Value]> {
        if let Value::List(l) = self { Some(l) } else { None }
    }
    pub fn bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self { Some(*b) } else { None }
    }
    /// The value inside a Pigeon type, or the value itself.
    pub fn inner(&self) -> &Value {
        if let Value::Custom(_, v) = self { v } else { self }
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let (head, rest) = (self.0.get(..n)?, self.0.get(n..)?);
        self.0 = rest;
        Some(head)
    }
    fn size(&mut self) -> Option<usize> {
        Some(match self.take(1)?[0] {
            254 => u16::from_le_bytes(self.take(2)?.try_into().ok()?) as usize,
            255 => u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize,
            n => n as usize,
        })
    }
    fn value(&mut self, depth: u32) -> Option<Value> {
        if depth > 32 {
            return None;
        }
        Some(match self.take(1)?[0] {
            0 => Value::Null,
            1 => Value::Bool(true),
            2 => Value::Bool(false),
            3 => Value::Int(i32::from_le_bytes(self.take(4)?.try_into().ok()?) as i64),
            4 => Value::Int(i64::from_le_bytes(self.take(8)?.try_into().ok()?)),
            7 => {
                let n = self.size()?;
                Value::Str(String::from_utf8(self.take(n)?.to_vec()).ok()?)
            }
            12 => {
                let n = self.size()?;
                let mut items = Vec::with_capacity(n.min(256));
                for _ in 0..n {
                    items.push(self.value(depth + 1)?);
                }
                Value::List(items)
            }
            13 => {
                let n = self.size()?;
                let mut items = Vec::with_capacity(n.min(256));
                for _ in 0..n {
                    items.push((self.value(depth + 1)?, self.value(depth + 1)?));
                }
                Value::Map(items)
            }
            t @ 128.. => Value::Custom(t, Box::new(self.value(depth + 1)?)),
            _ => return None,
        })
    }
}

pub fn decode(bytes: &[u8]) -> Option<Value> {
    let mut r = Reader(bytes);
    let v = r.value(0)?;
    r.0.is_empty().then_some(v)
}

/// A `StandardMethodCodec` call: the method name, then its arguments.
pub fn decode_call(bytes: &[u8]) -> Option<(String, Value)> {
    let mut r = Reader(bytes);
    let Value::Str(method) = r.value(0)? else { return None };
    let args = r.value(0)?;
    r.0.is_empty().then_some((method, args))
}

fn put_size(out: &mut Vec<u8>, n: usize) {
    if n < 254 {
        out.push(n as u8);
    } else if n <= 0xffff {
        out.push(254);
        out.extend_from_slice(&(n as u16).to_le_bytes());
    } else {
        out.push(255);
        out.extend_from_slice(&(n as u32).to_le_bytes());
    }
}

pub fn encode_into(out: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Null => out.push(0),
        Value::Bool(b) => out.push(if *b { 1 } else { 2 }),
        Value::Int(i) => match i32::try_from(*i) {
            Ok(small) => {
                out.push(3);
                out.extend_from_slice(&small.to_le_bytes());
            }
            Err(_) => {
                out.push(4);
                out.extend_from_slice(&i.to_le_bytes());
            }
        },
        Value::Str(s) => {
            out.push(7);
            put_size(out, s.len());
            out.extend_from_slice(s.as_bytes());
        }
        Value::List(items) => {
            out.push(12);
            put_size(out, items.len());
            items.iter().for_each(|i| encode_into(out, i));
        }
        Value::Map(items) => {
            out.push(13);
            put_size(out, items.len());
            for (k, v) in items {
                encode_into(out, k);
                encode_into(out, v);
            }
        }
        Value::Custom(t, inner) => {
            out.push(*t);
            encode_into(out, inner);
        }
    }
}

pub fn encode(v: &Value) -> Vec<u8> {
    let mut out = vec![];
    encode_into(&mut out, v);
    out
}

/// A Pigeon reply: `[result]`.
pub fn pigeon_success(result: Value) -> Vec<u8> {
    encode(&Value::List(vec![result]))
}

/// A Pigeon error reply: `[code, message, details]`.
pub fn pigeon_error(code: &str, message: &str) -> Vec<u8> {
    encode(&Value::List(vec![Value::Str(code.into()), Value::Str(message.into()), Value::Null]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let long = "x".repeat(300);
        let v = Value::List(vec![
            Value::Custom(129, Box::new(Value::Int(2))),
            Value::Str(long),
            Value::Int(1 << 40),
            Value::Bool(false),
            Value::Null,
            Value::Map(vec![(Value::Str("k".into()), Value::Int(-1))]),
        ]);
        assert_eq!(decode(&encode(&v)), Some(v));
        assert_eq!(decode(&[7, 5, b'a']), None);
        assert_eq!(decode(&[0, 0]), None);
        assert_eq!(pigeon_success(Value::List(vec![])), [12, 1, 12, 0]);
    }
}
