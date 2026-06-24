mod bencode;
mod download;
mod metainfo;
mod peer;
mod sha1;
mod tracker;

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: swarm <file.torrent>");
        return ExitCode::FAILURE;
    };
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read error: {e}");
            return ExitCode::FAILURE;
        }
    };
    match metainfo::parse(&data) {
        Ok(m) => {
            println!("name:      {}", m.name);
            println!("info hash: {}", sha1::hex(&m.info_hash));
            println!("size:      {} bytes", m.total_len);
            println!("pieces:    {} x {} bytes", m.piece_count(), m.piece_len);
            println!("trackers:  {}", m.trackers.join(", "));
        }
        Err(e) => {
            eprintln!("parse error: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
