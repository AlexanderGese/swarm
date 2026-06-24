use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Dict(BTreeMap<Vec<u8>, Value>),
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        if let Value::Int(n) = self { Some(*n) } else { None }
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        if let Value::Bytes(b) = self { Some(b) } else { None }
    }
    pub fn as_str(&self) -> Option<String> {
        self.as_bytes().map(|b| String::from_utf8_lossy(b).into_owned())
    }
    pub fn as_list(&self) -> Option<&[Value]> {
        if let Value::List(l) = self { Some(l) } else { None }
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        if let Value::Dict(d) = self {
            d.get(key.as_bytes())
        } else {
            None
        }
    }
}

pub struct Parser<'a> {
    data: &'a [u8],
    pub pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Parser { data, pos: 0 }
    }

    fn byte(&self) -> Result<u8, String> {
        self.data.get(self.pos).copied().ok_or_else(|| "unexpected end".to_string())
    }

    pub fn value(&mut self) -> Result<Value, String> {
        match self.byte()? {
            b'i' => self.int(),
            b'l' => self.list(),
            b'd' => self.dict(),
            b'0'..=b'9' => Ok(Value::Bytes(self.bytes()?)),
            c => Err(format!("bad byte {c:?} at {}", self.pos)),
        }
    }

    fn int(&mut self) -> Result<Value, String> {
        self.pos += 1; // 'i'
        let end = self.find(b'e')?;
        let s = std::str::from_utf8(&self.data[self.pos..end]).map_err(|_| "bad int")?;
        let n = s.parse::<i64>().map_err(|_| "bad int")?;
        self.pos = end + 1;
        Ok(Value::Int(n))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let colon = self.find(b':')?;
        let len: usize = std::str::from_utf8(&self.data[self.pos..colon])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or("bad length")?;
        let start = colon + 1;
        let end = start + len;
        if end > self.data.len() {
            return Err("string runs past end".into());
        }
        self.pos = end;
        Ok(self.data[start..end].to_vec())
    }

    fn list(&mut self) -> Result<Value, String> {
        self.pos += 1; // 'l'
        let mut out = Vec::new();
        while self.byte()? != b'e' {
            out.push(self.value()?);
        }
        self.pos += 1; // 'e'
        Ok(Value::List(out))
    }

    fn dict(&mut self) -> Result<Value, String> {
        self.pos += 1; // 'd'
        let mut map = BTreeMap::new();
        while self.byte()? != b'e' {
            let key = self.bytes()?;
            let val = self.value()?;
            map.insert(key, val);
        }
        self.pos += 1; // 'e'
        Ok(Value::Dict(map))
    }

    fn find(&self, target: u8) -> Result<usize, String> {
        self.data[self.pos..]
            .iter()
            .position(|&b| b == target)
            .map(|p| self.pos + p)
            .ok_or_else(|| format!("expected {:?}", target as char))
    }
}

pub fn decode(data: &[u8]) -> Result<Value, String> {
    Parser::new(data).value()
}

// raw byte span of a key's value inside the top-level dict. needed for the
// info hash - we must sha1 the bytes exactly as they appear in the file, not a
// re-encoding (key order / formatting could differ).
pub fn member_span(data: &[u8], key: &[u8]) -> Option<(usize, usize)> {
    let mut p = Parser::new(data);
    if p.byte().ok()? != b'd' {
        return None;
    }
    p.pos += 1;
    while p.byte().ok()? != b'e' {
        let k = p.bytes().ok()?;
        let start = p.pos;
        p.value().ok()?;
        let end = p.pos;
        if k == key {
            return Some((start, end));
        }
    }
    None
}

pub fn encode(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write(v, &mut out);
    out
}

fn write(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Int(n) => {
            out.push(b'i');
            out.extend_from_slice(n.to_string().as_bytes());
            out.push(b'e');
        }
        Value::Bytes(b) => {
            out.extend_from_slice(b.len().to_string().as_bytes());
            out.push(b':');
            out.extend_from_slice(b);
        }
        Value::List(l) => {
            out.push(b'l');
            for it in l {
                write(it, out);
            }
            out.push(b'e');
        }
        Value::Dict(d) => {
            out.push(b'd');
            for (k, val) in d {
                write(&Value::Bytes(k.clone()), out);
                write(val, out);
            }
            out.push(b'e');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips() {
        for raw in [&b"i42e"[..], b"4:spam", b"l4:spami7ee", b"d3:bar4:spam3:fooi42ee"] {
            let v = decode(raw).unwrap();
            assert_eq!(encode(&v), raw, "{}", String::from_utf8_lossy(raw));
        }
    }

    #[test]
    fn reads_fields() {
        let v = decode(b"d3:fooi7e3:bar4:spame").unwrap();
        assert_eq!(v.get("foo").unwrap().as_int(), Some(7));
        assert_eq!(v.get("bar").unwrap().as_str().as_deref(), Some("spam"));
    }
}
