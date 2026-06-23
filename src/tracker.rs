use crate::bencode::{self, Value};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::time::Duration;

pub struct Announce {
    pub peers: Vec<SocketAddrV4>,
    pub interval: u64,
}

// percent-encode raw bytes (info_hash / peer_id go into the query as-is)
pub fn urlencode(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            s.push(b as char);
        } else {
            s.push('%');
            s.push_str(&format!("{b:02X}"));
        }
    }
    s
}

pub fn announce(
    url: &str,
    info_hash: &[u8; 20],
    peer_id: &[u8; 20],
    port: u16,
    left: u64,
) -> Result<Announce, String> {
    // http trackers only - udp/https would need their own transport
    let rest = url.strip_prefix("http://").ok_or("only http trackers supported")?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, tport) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().unwrap_or(80)),
        None => (hostport, 80u16),
    };

    let sep = if path.contains('?') { '&' } else { '?' };
    let query = format!(
        "{path}{sep}info_hash={}&peer_id={}&port={port}&uploaded=0&downloaded=0&left={left}&compact=1&event=started",
        urlencode(info_hash),
        urlencode(peer_id),
    );

    let mut stream = TcpStream::connect((host, tport)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
    let req = format!(
        "GET {query} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: swarm/0.1\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    let body = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| &buf[i + 4..])
        .ok_or("no http body in tracker reply")?;

    let v = bencode::decode(body)?;
    if let Some(f) = v.get("failure reason").and_then(|x| x.as_str()) {
        return Err(format!("tracker said: {f}"));
    }
    let interval = v.get("interval").and_then(|x| x.as_int()).unwrap_or(1800) as u64;
    let peers = match v.get("peers") {
        Some(Value::Bytes(b)) => compact_peers(b),
        Some(Value::List(l)) => dict_peers(l),
        _ => Vec::new(),
    };
    Ok(Announce { peers, interval })
}

// the common "compact" form: 6 bytes per peer (4 ip + 2 port, big endian)
fn compact_peers(b: &[u8]) -> Vec<SocketAddrV4> {
    b.chunks_exact(6)
        .map(|c| {
            let ip = Ipv4Addr::new(c[0], c[1], c[2], c[3]);
            let port = u16::from_be_bytes([c[4], c[5]]);
            SocketAddrV4::new(ip, port)
        })
        .collect()
}

fn dict_peers(list: &[Value]) -> Vec<SocketAddrV4> {
    let mut out = Vec::new();
    for p in list {
        let ip = p.get("ip").and_then(|v| v.as_str());
        let port = p.get("port").and_then(|v| v.as_int());
        if let (Some(ip), Some(port)) = (ip, port) {
            if let Ok(addr) = ip.parse::<Ipv4Addr>() {
                out.push(SocketAddrV4::new(addr, port as u16));
            }
        }
    }
    out
}

pub fn random_peer_id() -> [u8; 20] {
    // "-SW0001-" + 12 bytes seeded off the clock; uniqueness is all we need
    let mut id = *b"-SW0001-000000000000";
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545F4914F6CDD1D)
        | 1;
    for slot in id[8..].iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *slot = b'0' + (x % 10) as u8;
    }
    id
}
