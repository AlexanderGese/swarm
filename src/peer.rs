use std::io::{Read, Write};
use std::net::{SocketAddr, SocketAddrV4, TcpStream};
use std::time::Duration;

pub const BLOCK: u32 = 16 * 1024;

#[derive(Debug)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request { index: u32, begin: u32, length: u32 },
    Piece { index: u32, begin: u32, block: Vec<u8> },
    Cancel { index: u32, begin: u32, length: u32 },
}

impl Message {
    fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();
        match self {
            Message::KeepAlive => {}
            Message::Choke => body.push(0),
            Message::Unchoke => body.push(1),
            Message::Interested => body.push(2),
            Message::NotInterested => body.push(3),
            Message::Have(i) => {
                body.push(4);
                body.extend_from_slice(&i.to_be_bytes());
            }
            Message::Bitfield(b) => {
                body.push(5);
                body.extend_from_slice(b);
            }
            Message::Request { index, begin, length }
            | Message::Cancel { index, begin, length } => {
                body.push(if matches!(self, Message::Request { .. }) { 6 } else { 8 });
                body.extend_from_slice(&index.to_be_bytes());
                body.extend_from_slice(&begin.to_be_bytes());
                body.extend_from_slice(&length.to_be_bytes());
            }
            Message::Piece { index, begin, block } => {
                body.push(7);
                body.extend_from_slice(&index.to_be_bytes());
                body.extend_from_slice(&begin.to_be_bytes());
                body.extend_from_slice(block);
            }
        }
        let mut out = (body.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }
}

pub struct Peer {
    stream: TcpStream,
    pub bitfield: Vec<u8>,
    pub choked: bool,
    pub addr: SocketAddrV4,
}

impl Peer {
    pub fn connect(addr: SocketAddrV4, info_hash: &[u8; 20], peer_id: &[u8; 20]) -> Result<Peer, String> {
        let mut stream = TcpStream::connect_timeout(&SocketAddr::V4(addr), Duration::from_secs(8))
            .map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(Duration::from_secs(20))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(20))).ok();

        let mut hs = vec![19u8];
        hs.extend_from_slice(b"BitTorrent protocol");
        hs.extend_from_slice(&[0u8; 8]);
        hs.extend_from_slice(info_hash);
        hs.extend_from_slice(peer_id);
        stream.write_all(&hs).map_err(|e| e.to_string())?;

        let mut resp = [0u8; 68];
        stream.read_exact(&mut resp).map_err(|e| e.to_string())?;
        if resp[0] != 19 || &resp[1..20] != b"BitTorrent protocol" {
            return Err("not a bittorrent peer".into());
        }
        if &resp[28..48] != info_hash {
            return Err("peer is on a different torrent".into());
        }

        Ok(Peer { stream, bitfield: Vec::new(), choked: true, addr })
    }

    pub fn send(&mut self, msg: &Message) -> Result<(), String> {
        self.stream.write_all(&msg.encode()).map_err(|e| e.to_string())
    }

    pub fn recv(&mut self) -> Result<Message, String> {
        let mut len = [0u8; 4];
        self.stream.read_exact(&mut len).map_err(|e| e.to_string())?;
        let len = u32::from_be_bytes(len) as usize;
        if len == 0 {
            return Ok(Message::KeepAlive);
        }
        let mut body = vec![0u8; len];
        self.stream.read_exact(&mut body).map_err(|e| e.to_string())?;
        let id = body[0];
        let p = &body[1..];
        let be = |s: &[u8]| u32::from_be_bytes([s[0], s[1], s[2], s[3]]);
        Ok(match id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => Message::Have(be(p)),
            5 => Message::Bitfield(p.to_vec()),
            6 => Message::Request { index: be(p), begin: be(&p[4..]), length: be(&p[8..]) },
            7 => Message::Piece { index: be(p), begin: be(&p[4..]), block: p[8..].to_vec() },
            8 => Message::Cancel { index: be(p), begin: be(&p[4..]), length: be(&p[8..]) },
            other => return Err(format!("unknown message id {other}")),
        })
    }

    pub fn has_piece(&self, i: usize) -> bool {
        let byte = i / 8;
        let bit = 7 - (i % 8);
        self.bitfield.get(byte).map(|b| b >> bit & 1 == 1).unwrap_or(false)
    }
}
