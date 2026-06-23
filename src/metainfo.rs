use crate::bencode;
use crate::sha1;

pub struct Metainfo {
    pub trackers: Vec<String>,
    pub info_hash: [u8; 20],
    pub name: String,
    pub piece_len: u64,
    pub total_len: u64,
    pub pieces: Vec<[u8; 20]>,
}

pub fn parse(data: &[u8]) -> Result<Metainfo, String> {
    let root = bencode::decode(data)?;
    let info = root.get("info").ok_or("no info dict")?;

    let (s, e) = bencode::member_span(data, b"info").ok_or("couldn't locate info dict")?;
    let info_hash = sha1::hash(&data[s..e]);

    let name = info.get("name").and_then(|v| v.as_str()).unwrap_or_default();
    let piece_len = info
        .get("piece length")
        .and_then(|v| v.as_int())
        .ok_or("no piece length")? as u64;

    let blob = info.get("pieces").and_then(|v| v.as_bytes()).ok_or("no pieces")?;
    if blob.len() % 20 != 0 {
        return Err("pieces blob isn't a multiple of 20".into());
    }
    let pieces: Vec<[u8; 20]> = blob
        .chunks_exact(20)
        .map(|c| {
            let mut a = [0u8; 20];
            a.copy_from_slice(c);
            a
        })
        .collect();

    // single-file torrents have info.length; multi-file sum the files
    let total_len = if let Some(l) = info.get("length").and_then(|v| v.as_int()) {
        l as u64
    } else if let Some(files) = info.get("files").and_then(|v| v.as_list()) {
        files
            .iter()
            .filter_map(|f| f.get("length").and_then(|v| v.as_int()))
            .sum::<i64>() as u64
    } else {
        return Err("torrent has neither length nor files".into());
    };

    let mut trackers = Vec::new();
    if let Some(a) = root.get("announce").and_then(|v| v.as_str()) {
        trackers.push(a);
    }
    if let Some(list) = root.get("announce-list").and_then(|v| v.as_list()) {
        for tier in list {
            for t in tier.as_list().unwrap_or(&[]) {
                if let Some(url) = t.as_str() {
                    if !trackers.contains(&url) {
                        trackers.push(url);
                    }
                }
            }
        }
    }

    Ok(Metainfo { trackers, info_hash, name, piece_len, total_len, pieces })
}

impl Metainfo {
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    pub fn piece_size(&self, i: usize) -> u64 {
        let last = self.pieces.len() - 1;
        if i < last {
            self.piece_len
        } else {
            self.total_len - self.piece_len * last as u64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_file_torrent() {
        let info = b"d6:lengthi12e4:name2:hi12:piece lengthi16e6:pieces20:AAAAAAAAAAAAAAAAAAAAe";
        let mut t = b"d8:announce13:http://x/anno4:info".to_vec();
        t.extend_from_slice(info);
        t.push(b'e');

        let m = parse(&t).unwrap();
        assert_eq!(m.name, "hi");
        assert_eq!(m.total_len, 12);
        assert_eq!(m.piece_len, 16);
        assert_eq!(m.pieces.len(), 1);
        assert_eq!(m.trackers, vec!["http://x/anno".to_string()]);
        // info hash is sha1 over the exact info-dict bytes
        assert_eq!(m.info_hash, sha1::hash(info));
        assert_eq!(m.piece_size(0), 12); // last (only) piece is the leftover
    }
}
