mod bencode;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: swarm <file>");
        return;
    };
    match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|d| bencode::decode(&d)) {
        Ok(v) => println!("{v:?}"),
        Err(e) => eprintln!("{e}"),
    }
}
