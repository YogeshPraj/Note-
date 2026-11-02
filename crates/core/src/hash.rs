//! Message digests for Tools > MD5 / SHA-1 / SHA-256 / SHA-512.

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::io::Read;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algo {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

pub const ALGOS: [Algo; 4] = [Algo::Md5, Algo::Sha1, Algo::Sha256, Algo::Sha512];

impl Algo {
    pub fn name(self) -> &'static str {
        match self {
            Algo::Md5 => "MD5",
            Algo::Sha1 => "SHA-1",
            Algo::Sha256 => "SHA-256",
            Algo::Sha512 => "SHA-512",
        }
    }

    pub fn hash_bytes(self, data: &[u8]) -> String {
        match self {
            Algo::Md5 => hex(&Md5::digest(data)),
            Algo::Sha1 => hex(&Sha1::digest(data)),
            Algo::Sha256 => hex(&Sha256::digest(data)),
            Algo::Sha512 => hex(&Sha512::digest(data)),
        }
    }

    /// Hash a stream without holding it all in memory (for big files).
    pub fn hash_reader<R: Read>(self, r: R) -> std::io::Result<String> {
        Ok(match self {
            Algo::Md5 => hex(&stream::<Md5, R>(r)?),
            Algo::Sha1 => hex(&stream::<Sha1, R>(r)?),
            Algo::Sha256 => hex(&stream::<Sha256, R>(r)?),
            Algo::Sha512 => hex(&stream::<Sha512, R>(r)?),
        })
    }
}

fn stream<D: Digest, R: Read>(mut r: R) -> std::io::Result<Vec<u8>> {
    let mut d = D::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            return Ok(d.finalize().to_vec());
        }
        d.update(&buf[..n]);
    }
}

/// A reader that reports the bytes read through it and stops when told to (a cancelled hash of a big file).
struct Watched<'a, R> {
    inner: R,
    seen: &'a mut u64,
    total: u64,
    progress: &'a mut dyn FnMut(u64, u64) -> bool,
}

impl<R: Read> Read for Watched<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        *self.seen += crate::num::to_u64(n);
        if !(self.progress)(*self.seen, self.total) {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        Ok(n)
    }
}

/// Hash each of `files`: one line per file, its digest and path (or why it could not be read). `progress(done, total)`
/// gets the bytes read so far and in all; returning false stops, and then None is returned.
pub fn hash_files(algo: Algo, files: &[std::path::PathBuf], progress: &mut dyn FnMut(u64, u64) -> bool) -> Option<Vec<String>> {
    let total = files.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
    let mut seen = 0u64;
    let mut lines = Vec::with_capacity(files.len());
    for p in files {
        let hashed = std::fs::File::open(p).and_then(|f| algo.hash_reader(Watched { inner: f, seen: &mut seen, total, progress: &mut *progress }));
        lines.push(match hashed {
            Ok(h) => format!("{h}  {}", p.display()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => return None,
            Err(e) => format!("(error)  {}: {e}", p.display()),
        });
    }
    Some(lines)
}

/// Lower-case hexadecimal text of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(Algo::Md5.hash_bytes(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(Algo::Sha1.hash_bytes(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(Algo::Sha256.hash_bytes(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(Algo::Sha512.hash_bytes(b"abc").starts_with("ddaf35a193617aba"));
        assert_eq!(Algo::Md5.hash_bytes(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn hashing_files_reports_progress_and_can_be_stopped() {
        let d = std::env::temp_dir().join(format!("notepp_hash_files_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a.bin"), d.join("b.bin"));
        std::fs::write(&a, vec![1u8; 300_000]).unwrap();
        std::fs::write(&b, b"abc").unwrap();
        let mut last = (0, 0);
        let lines = hash_files(Algo::Md5, &[a.clone(), b.clone(), d.join("missing")], &mut |done, total| {
            last = (done, total);
            true
        })
        .unwrap();
        assert_eq!(last, (300_003, 300_003));
        assert!(lines[1].starts_with("900150983cd24fb0d6963f7d28e17f72  "));
        assert!(lines[2].starts_with("(error)"));
        assert_eq!(hash_files(Algo::Md5, &[a, b], &mut |_, _| false), None, "returning false stops");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn streaming_matches_one_shot() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        for a in ALGOS {
            assert_eq!(a.hash_reader(&data[..]).unwrap(), a.hash_bytes(&data), "{a:?}");
        }
    }
}
