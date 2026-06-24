mod bencode;
mod download;
mod metainfo;
mod peer;
mod sha1;
mod sim;
mod tracker;
mod tui;

use clap::{Parser, Subcommand};
use metainfo::Metainfo;
use std::net::SocketAddrV4;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "swarm", version, about = "A small BitTorrent client")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show what's inside a .torrent
    Info { torrent: String },
    /// Announce to the tracker and list the peers it returns
    Peers { torrent: String },
    /// Download a torrent to disk
    Get {
        torrent: String,
        #[arg(short, long)]
        out: Option<String>,
        /// Show the live swarm view instead of a plain progress line
        #[arg(long)]
        tui: bool,
    },
    /// Animated swarm visualizer (a simulated swarm, no network needed)
    Demo {
        #[arg(long, default_value_t = 120)]
        pieces: usize,
        #[arg(long, default_value_t = 14)]
        peers: usize,
        #[arg(long, default_value_t = 110)]
        delay: u64,
    },
}

fn load(path: &str) -> Result<Metainfo, String> {
    let data = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    metainfo::parse(&data)
}

fn announce_any(meta: &Metainfo, peer_id: &[u8; 20]) -> Result<Vec<SocketAddrV4>, String> {
    let mut last = "no usable trackers".to_string();
    for t in &meta.trackers {
        match tracker::announce(t, &meta.info_hash, peer_id, 6881, meta.total_len) {
            Ok(a) if !a.peers.is_empty() => {
                eprintln!("{t} -> {} peers", a.peers.len());
                return Ok(a.peers);
            }
            Ok(_) => last = format!("{t}: 0 peers"),
            Err(e) => {
                eprintln!("{t} -> {e}");
                last = e;
            }
        }
    }
    Err(last)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let r = match cli.cmd {
        Cmd::Info { torrent } => cmd_info(&torrent),
        Cmd::Peers { torrent } => cmd_peers(&torrent),
        Cmd::Get { torrent, out, tui } => cmd_get(&torrent, out, tui),
        Cmd::Demo { pieces, peers, delay } => cmd_demo(pieces, peers, delay),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("\x1b[31merror:\x1b[0m {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_info(torrent: &str) -> Result<(), String> {
    let m = load(torrent)?;
    println!("name:      {}", m.name);
    println!("info hash: {}", sha1::hex(&m.info_hash));
    println!("size:      {} bytes", m.total_len);
    println!("pieces:    {} x {} bytes", m.piece_count(), m.piece_len);
    println!("trackers:  {}", m.trackers.join("\n           "));
    Ok(())
}

fn cmd_peers(torrent: &str) -> Result<(), String> {
    let m = load(torrent)?;
    let pid = tracker::random_peer_id();
    let peers = announce_any(&m, &pid)?;
    for p in peers {
        println!("{p}");
    }
    Ok(())
}

fn cmd_get(torrent: &str, out: Option<String>, use_tui: bool) -> Result<(), String> {
    let meta = load(torrent)?;
    let pid = tracker::random_peer_id();
    let peers = announce_any(&meta, &pid)?;
    let outpath = out.unwrap_or_else(|| meta.name.clone());

    let meta = Arc::new(meta);
    let state = Arc::new(Mutex::new(download::State::new(&meta, peers.len())));

    let (m2, s2) = (meta.clone(), state.clone());
    let outp = std::path::PathBuf::from(&outpath);
    let worker = std::thread::spawn(move || download::run(m2, peers, pid, &outp, s2));

    if use_tui {
        tui::run(meta.name.clone(), state.clone()).map_err(|e| e.to_string())?;
        worker.join().ok();
        let s = state.lock().unwrap();
        return if s.pieces_done() == s.have.len() {
            Ok(())
        } else {
            Err(format!("incomplete: {}/{} pieces", s.pieces_done(), s.have.len()))
        };
    }

    loop {
        std::thread::sleep(Duration::from_millis(500));
        let s = state.lock().unwrap();
        let pct = if s.total_bytes > 0 {
            s.done_bytes as f64 / s.total_bytes as f64 * 100.0
        } else {
            0.0
        };
        eprint!(
            "\r{pct:5.1}% · {}/{} pieces · {:6.0} KiB/s · peers {}/{}    ",
            s.pieces_done(),
            s.have.len(),
            s.rate / 1024.0,
            s.peers_tried,
            s.peers_total
        );
        let finished = s.finished;
        drop(s);
        if finished {
            eprintln!();
            break;
        }
    }
    worker.join().ok();

    let s = state.lock().unwrap();
    if s.pieces_done() == s.have.len() {
        println!("saved to {outpath}");
        Ok(())
    } else {
        Err(format!("incomplete: {}/{} pieces", s.pieces_done(), s.have.len()))
    }
}

fn cmd_demo(pieces: usize, peers: usize, delay: u64) -> Result<(), String> {
    let piece_size = 256 * 1024u64;
    let sizes = vec![piece_size; pieces];
    let total = piece_size * pieces as u64;
    let state = Arc::new(Mutex::new(download::State::blank(pieces, total)));

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b1);
    let s2 = state.clone();
    std::thread::spawn(move || sim::run(s2, sizes, peers, seed, delay));

    tui::run("demo swarm (simulated)".into(), state).map_err(|e| e.to_string())
}
