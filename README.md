# swarm

A small BitTorrent client written from scratch in Rust — bencode, the tracker
protocol, the peer wire protocol and piece verification, no torrent libraries.

There's a browser demo (the piece-picking, simulated, plus a `.torrent`
inspector): **https://alexandergese.github.io/swarm/**

## What it does

- **Reads `.torrent` files** — own bencode parser, info-hash taken over the
  exact info-dict bytes, single- and multi-file layouts.
- **Talks to trackers** — HTTP announce, parses the compact peer list.
- **Speaks the peer protocol** — handshake, bitfield, request/piece, the lot,
  over plain TCP.
- **Downloads and verifies** — pulls pieces block by block, checks each one
  against its SHA-1 before writing it to the right offset.
- **Shows the swarm** — a TUI with the piece grid filling in, per-peer
  have-bars, speed and progress.

The SHA-1 is hand-rolled (it's what the spec uses for integrity), so the core
has no dependencies beyond the TUI/CLI bits.

## Use it

```
swarm info  some.torrent           # name, info hash, size, piece layout, trackers
swarm peers some.torrent           # announce and list the peers
swarm get   some.torrent -o out    # download (plain progress line)
swarm get   some.torrent --tui     # download with the live swarm view
swarm demo                         # animated simulated swarm (no network)
```

## Build

```
cargo build --release
cargo test
```

## Install

```
cargo install swarm-bt        # installs the `swarm` binary
```

(The crate is published as `swarm-bt` — the name `swarm` was taken — but the
command it installs is `swarm`.)

## License

MIT
