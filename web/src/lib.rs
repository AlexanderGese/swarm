mod bencode;
mod metainfo;
mod sha1;

use wasm_bindgen::prelude::*;

struct Rng(u64);
impl Rng {
    fn new(s: u64) -> Self {
        Rng(s | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// the same fake-swarm model as the native demo, but stepped from JS instead of
// a thread. new() builds the peers, step() pulls in the next (rarest) piece,
// render() hands the page a json snapshot to draw.
#[wasm_bindgen]
pub struct Sim {
    pc: usize,
    peers: Vec<Vec<bool>>,
    labels: Vec<String>,
    fracs: Vec<f32>,
    have: Vec<bool>,
    active: i32,
    provider: i32,
}

#[wasm_bindgen]
impl Sim {
    #[wasm_bindgen(constructor)]
    pub fn new(pieces: usize, n_peers: usize, seed: u64) -> Sim {
        let mut rng = Rng::new(seed);
        let mut peers = Vec::new();
        let mut labels = Vec::new();
        let mut fracs = Vec::new();
        for _ in 0..n_peers {
            let seeder = rng.below(4) == 0;
            let bits: Vec<bool> = (0..pieces).map(|_| seeder || rng.below(100) < 55).collect();
            let f = bits.iter().filter(|&&b| b).count() as f32 / pieces as f32;
            labels.push(format!("10.0.{}.{}", rng.below(255), rng.below(255)));
            fracs.push(f);
            peers.push(bits);
        }
        Sim { pc: pieces, peers, labels, fracs, have: vec![false; pieces], active: -1, provider: -1 }
    }

    pub fn step(&mut self) -> bool {
        let mut best = (usize::MAX, -1i32);
        for i in 0..self.pc {
            if self.have[i] {
                continue;
            }
            let rarity = self.peers.iter().filter(|b| b[i]).count();
            if rarity > 0 && rarity < best.0 {
                best = (rarity, i as i32);
            }
        }
        if best.1 < 0 {
            self.active = -1;
            self.provider = -1;
            return false;
        }
        let i = best.1 as usize;
        self.have[i] = true;
        self.active = i as i32;
        self.provider = self.peers.iter().position(|b| b[i]).unwrap() as i32;
        true
    }

    pub fn render(&self) -> String {
        let have: String = self.have.iter().map(|&h| if h { '1' } else { '0' }).collect();
        let mut peers = String::from("[");
        for (k, (l, f)) in self.labels.iter().zip(&self.fracs).enumerate() {
            if k > 0 {
                peers.push(',');
            }
            let active = if k as i32 == self.provider { "true" } else { "false" };
            peers.push_str(&format!("{{\"l\":\"{l}\",\"f\":{f:.3},\"a\":{active}}}"));
        }
        peers.push(']');
        let done = self.have.iter().filter(|&&h| h).count();
        format!(
            "{{\"have\":\"{have}\",\"active\":{},\"done\":{done},\"total\":{},\"peers\":{peers}}}",
            self.active, self.pc
        )
    }
}

// decode an uploaded .torrent and hand back the headline facts
#[wasm_bindgen]
pub fn inspect(bytes: &[u8]) -> String {
    match metainfo::parse(bytes) {
        Ok(m) => format!(
            "{{\"ok\":true,\"name\":\"{}\",\"hash\":\"{}\",\"size\":{},\"pieces\":{},\"piece_len\":{},\"trackers\":{}}}",
            jesc(&m.name),
            sha1::hex(&m.info_hash),
            m.total_len,
            m.piece_count(),
            m.piece_len,
            json_strs(&m.trackers)
        ),
        Err(e) => format!("{{\"ok\":false,\"error\":\"{}\"}}", jesc(&e)),
    }
}

fn jesc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            _ => o.push(c),
        }
    }
    o
}

fn json_strs(v: &[String]) -> String {
    let mut o = String::from("[");
    for (i, s) in v.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push('"');
        o.push_str(&jesc(s));
        o.push('"');
    }
    o.push(']');
    o
}
