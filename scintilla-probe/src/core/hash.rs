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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
    fn streaming_matches_one_shot() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        for a in ALGOS {
            assert_eq!(a.hash_reader(&data[..]).unwrap(), a.hash_bytes(&data), "{a:?}");
        }
    }
}
