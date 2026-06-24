use crate::metainfo::Metainfo;
use crate::peer::{Message, Peer, BLOCK};
use crate::sha1;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::net::SocketAddrV4;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct PeerView {
    pub label: String,
    pub have_frac: f32,
    pub active: bool,
}

// shared so the tui (or a cli print loop) can watch the download live
pub struct State {
    pub have: Vec<bool>,
    pub active_piece: Option<usize>,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub peers_total: usize,
    pub peers_tried: usize,
    pub peers: Vec<PeerView>,
    pub current: Option<SocketAddrV4>,
    pub rate: f64,
    pub finished: bool,
    pub log: Vec<String>,
}

impl State {
    pub fn new(meta: &Metainfo, peers: usize) -> Self {
        State {
            have: vec![false; meta.piece_count()],
            active_piece: None,
            done_bytes: 0,
            total_bytes: meta.total_len,
            peers_total: peers,
            peers_tried: 0,
            peers: Vec::new(),
            current: None,
            rate: 0.0,
            finished: false,
            log: Vec::new(),
        }
    }
    // used by the demo/simulator, which has no real torrent behind it
    pub fn blank(piece_count: usize, total_bytes: u64) -> Self {
        let mut s = State::new(&Metainfo {
            trackers: Vec::new(),
            info_hash: [0; 20],
            name: String::new(),
            piece_len: 0,
            total_len: total_bytes,
            pieces: Vec::new(),
        }, 0);
        s.have = vec![false; piece_count];
        s
    }

    pub fn pieces_done(&self) -> usize {
        self.have.iter().filter(|&&h| h).count()
    }
    pub fn note(&mut self, m: impl Into<String>) {
        self.log.push(m.into());
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }
}

pub fn run(meta: Arc<Metainfo>, peers: Vec<SocketAddrV4>, peer_id: [u8; 20], out: &Path, state: Arc<Mutex<State>>) {
    let mut file = match File::create(out).and_then(|f| {
        f.set_len(meta.total_len)?;
        Ok(f)
    }) {
        Ok(f) => f,
        Err(e) => {
            state.lock().unwrap().note(format!("cannot open {}: {e}", out.display()));
            return;
        }
    };

    let started = Instant::now();
    for addr in peers {
        if all_done(&state) {
            break;
        }
        state.lock().unwrap().peers_tried += 1;
        state.lock().unwrap().current = Some(addr);

        match grab_from_peer(&meta, addr, &peer_id, &mut file, &state, started) {
            Ok(_) => {}
            Err(e) => state.lock().unwrap().note(format!("{addr}: {e}")),
        }
    }

    let mut s = state.lock().unwrap();
    s.current = None;
    s.finished = true;
    let (done, total) = (s.pieces_done(), meta.piece_count());
    if done == total {
        s.note("complete — every piece verified");
    } else {
        s.note(format!("ran out of peers: {done}/{total} pieces"));
    }
}

fn all_done(state: &Arc<Mutex<State>>) -> bool {
    let s = state.lock().unwrap();
    s.pieces_done() == s.have.len()
}

fn grab_from_peer(
    meta: &Metainfo,
    addr: SocketAddrV4,
    peer_id: &[u8; 20],
    file: &mut File,
    state: &Arc<Mutex<State>>,
    started: Instant,
) -> Result<(), String> {
    let mut peer = Peer::connect(addr, &meta.info_hash, peer_id)?;
    peer.send(&Message::Interested)?;

    // soak up the early messages (bitfield/have) and wait for unchoke
    let deadline_msgs = 64;
    for _ in 0..deadline_msgs {
        match peer.recv()? {
            Message::Bitfield(b) => peer.bitfield = b,
            Message::Have(i) => set_bit(&mut peer.bitfield, i as usize),
            Message::Unchoke => {
                peer.choked = false;
                break;
            }
            Message::Choke => peer.choked = true,
            _ => {}
        }
    }
    if peer.choked {
        return Err("never got unchoked".into());
    }

    for index in 0..meta.piece_count() {
        if state.lock().unwrap().have[index] {
            continue;
        }
        if !peer.has_piece(index) {
            continue;
        }
        let size = meta.piece_size(index) as u32;
        let data = fetch_piece(&mut peer, index as u32, size)?;

        if sha1::hash(&data) != meta.pieces[index] {
            state.lock().unwrap().note(format!("piece {index} failed hash, refetching elsewhere"));
            continue;
        }
        file.seek(SeekFrom::Start(index as u64 * meta.piece_len)).map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;

        let mut s = state.lock().unwrap();
        s.have[index] = true;
        s.done_bytes += data.len() as u64;
        s.rate = s.done_bytes as f64 / started.elapsed().as_secs_f64().max(0.001);
    }
    Ok(())
}

fn fetch_piece(peer: &mut Peer, index: u32, size: u32) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; size as usize];
    let mut begin = 0u32;
    while begin < size {
        let length = BLOCK.min(size - begin);
        peer.send(&Message::Request { index, begin, length })?;
        // read until the matching block arrives
        loop {
            match peer.recv()? {
                Message::Piece { index: i, begin: b, block } if i == index && b == begin => {
                    buf[begin as usize..begin as usize + block.len()].copy_from_slice(&block);
                    break;
                }
                Message::Choke => return Err("choked mid-piece".into()),
                Message::Have(h) => set_bit(&mut peer.bitfield, h as usize),
                _ => {}
            }
        }
        begin += length;
    }
    Ok(buf)
}

fn set_bit(bitfield: &mut Vec<u8>, i: usize) {
    let byte = i / 8;
    if byte >= bitfield.len() {
        bitfield.resize(byte + 1, 0);
    }
    bitfield[byte] |= 1 << (7 - i % 8);
}
