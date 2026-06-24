// A fake swarm so you can watch how pieces come together without needing a live
// torrent + seeders. Builds N peers holding random subsets of the pieces and
// pulls them in rarest-first, the way a real client prioritises. Same model
// drives the web demo.
use crate::download::{PeerView, State};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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

pub fn run(state: Arc<Mutex<State>>, piece_sizes: Vec<u64>, n_peers: usize, seed: u64, delay_ms: u64) {
    let pc = piece_sizes.len();
    let mut rng = Rng::new(seed);

    // each peer gets a random bitfield; roughly a quarter are full seeders
    let mut peer_bits: Vec<Vec<bool>> = Vec::with_capacity(n_peers);
    {
        let mut s = state.lock().unwrap();
        s.peers_total = n_peers;
        s.peers_tried = n_peers;
        for k in 0..n_peers {
            let seeder = rng.below(4) == 0;
            let bits: Vec<bool> = (0..pc).map(|_| seeder || rng.below(100) < 55).collect();
            let frac = bits.iter().filter(|&&b| b).count() as f32 / pc as f32;
            let label = format!("10.0.{}.{}:{}", rng.below(255), rng.below(255), 6881 + k);
            s.peers.push(PeerView { label, have_frac: frac, active: false });
            peer_bits.push(bits);
        }
    }

    let started = Instant::now();
    loop {
        // rarest missing piece that some peer actually has
        let target = {
            let s = state.lock().unwrap();
            if s.finished {
                return;
            }
            let mut best = (usize::MAX, None);
            for i in 0..pc {
                if s.have[i] {
                    continue;
                }
                let rarity = peer_bits.iter().filter(|b| b[i]).count();
                if rarity > 0 && rarity < best.0 {
                    best = (rarity, Some(i));
                }
            }
            best.1
        };

        let Some(i) = target else {
            let mut s = state.lock().unwrap();
            s.active_piece = None;
            s.finished = true;
            for p in s.peers.iter_mut() {
                p.active = false;
            }
            s.note("all pieces in — file reassembled");
            return;
        };

        let provider = peer_bits.iter().position(|b| b[i]).unwrap();
        {
            let mut s = state.lock().unwrap();
            s.active_piece = Some(i);
            for (k, p) in s.peers.iter_mut().enumerate() {
                p.active = k == provider;
            }
        }
        thread::sleep(Duration::from_millis(delay_ms.max(1)));
        {
            let mut s = state.lock().unwrap();
            s.have[i] = true;
            s.done_bytes += piece_sizes[i];
            s.rate = s.done_bytes as f64 / started.elapsed().as_secs_f64().max(0.001);
            if i % 7 == 0 {
                let label = s.peers[provider].label.clone();
                s.note(format!("piece {i} from {label}"));
            }
        }
    }
}
