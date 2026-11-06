//! envclip wire crypto, v1. See `docs/PROTOCOL.md`.
//!
//! ```text
//! secret      16 random bytes, shown to humans as 26 Crockford base32 chars
//! enc_key     HKDF-SHA256(ikm=secret, salt="envclip/v1", info="enc")   32 bytes
//! claim_token HKDF-SHA256(ikm=secret, salt="envclip/v1", info="claim") 32 bytes
//! claim_hash  hex(SHA-256(claim_token))         stored by the server
//! sealed      base64url(0x01 || nonce[12] || AES-256-GCM(enc_key, nonce, aad="envclip/v1", pt))
//! ```

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const SALT: &[u8] = b"envclip/v1";
const AAD: &[u8] = b"envclip/v1";
const VERSION: u8 = 1;
const PAD_BLOCK: usize = 256;
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

pub struct Secret([u8; 16]);

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub struct Keys {
    enc: Zeroizing<[u8; 32]>,
    claim: Zeroizing<[u8; 32]>,
}

impl Secret {
    pub fn generate() -> Self {
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).expect("system RNG unavailable");
        Secret(b)
    }

    /// Lenient parse: case-insensitive, ignores `-`, spaces and an `ek_` prefix,
    /// maps the Crockford look-alikes `o→0`, `i/l→1`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix("ek_").unwrap_or(s);
        let mut acc: u32 = 0;
        let mut bits = 0u32;
        let mut out = Vec::with_capacity(16);
        let mut n = 0;
        for c in s.chars() {
            let c = match c.to_ascii_lowercase() {
                '-' | ' ' => continue,
                'o' => '0',
                'i' | 'l' => '1',
                c => c,
            };
            let v = ALPHABET.iter().position(|&a| a as char == c)? as u32;
            n += 1;
            acc = (acc << 5) | v;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
                acc &= (1 << bits) - 1;
            }
        }
        if n != 26 || out.len() != 16 || acc != 0 {
            out.zeroize();
            return None;
        }
        let mut b = [0u8; 16];
        b.copy_from_slice(&out);
        out.zeroize();
        Some(Secret(b))
    }

    pub fn encode(&self) -> String {
        let mut s = String::with_capacity(26);
        let (mut acc, mut bits) = (0u32, 0u32);
        for &byte in &self.0 {
            acc = (acc << 8) | byte as u32;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                s.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
            }
            acc &= (1 << bits) - 1;
        }
        if bits > 0 {
            s.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
        }
        s
    }

    pub fn keys(&self) -> Keys {
        let hk = Hkdf::<Sha256>::new(Some(SALT), &self.0);
        let mut enc = Zeroizing::new([0u8; 32]);
        let mut claim = Zeroizing::new([0u8; 32]);
        hk.expand(b"enc", enc.as_mut()).expect("hkdf length");
        hk.expand(b"claim", claim.as_mut()).expect("hkdf length");
        Keys { enc, claim }
    }
}

impl Keys {
    /// Bearer token proving knowledge of the secret. Never stored server-side.
    pub fn claim_token(&self) -> String {
        B64.encode(self.claim.as_ref())
    }

    pub fn claim_hash(&self) -> String {
        let d = Sha256::digest(self.claim.as_ref());
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Pads the plaintext with spaces to a multiple of 256 bytes so the
    /// ciphertext length only leaks a coarse size class.
    pub fn seal(&self, plaintext: &[u8]) -> String {
        let mut buf = Zeroizing::new(plaintext.to_vec());
        let padded = plaintext.len().div_ceil(PAD_BLOCK).max(1) * PAD_BLOCK;
        buf.resize(padded, b' ');
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).expect("system RNG unavailable");
        let cipher = Aes256Gcm::new_from_slice(self.enc.as_ref()).expect("key length");
        let ct =
            cipher.encrypt(&Nonce::from(nonce), Payload { msg: &buf, aad: AAD }).expect("encryption failure");
        let mut out = Vec::with_capacity(1 + 12 + ct.len());
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        B64.encode(out)
    }

    pub fn open(&self, sealed: &str) -> Option<Zeroizing<Vec<u8>>> {
        let raw = B64.decode(sealed.trim()).ok()?;
        if raw.len() < 1 + 12 + 16 || raw[0] != VERSION {
            return None;
        }
        let nonce: [u8; 12] = raw[1..13].try_into().ok()?;
        let cipher = Aes256Gcm::new_from_slice(self.enc.as_ref()).ok()?;
        cipher.decrypt(&Nonce::from(nonce), Payload { msg: &raw[13..], aad: AAD }).ok().map(Zeroizing::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_roundtrip() {
        for _ in 0..64 {
            let s = Secret::generate();
            let enc = s.encode();
            assert_eq!(enc.len(), 26);
            let back = Secret::parse(&enc).unwrap();
            assert_eq!(back.0, s.0);
            let dashed: String = enc
                .to_uppercase()
                .chars()
                .enumerate()
                .flat_map(|(i, c)| (i > 0 && i % 4 == 0).then_some('-').into_iter().chain([c]))
                .collect();
            assert_eq!(Secret::parse(&dashed).unwrap().0, s.0);
        }
        assert!(Secret::parse("too-short").is_none());
        assert!(Secret::parse(&"u".repeat(26)).is_none());
    }

    #[test]
    fn seal_open() {
        let k = Secret::generate().keys();
        let sealed = k.seal(b"{\"v\":1}");
        let pt = k.open(&sealed).unwrap();
        assert_eq!(pt.len(), PAD_BLOCK);
        assert!(pt.starts_with(b"{\"v\":1}"));
        assert!(Secret::generate().keys().open(&sealed).is_none());
    }

    /// Fixed vector shared with `worker/test/crypto.test.mjs`.
    #[test]
    fn known_vector() {
        let s = Secret::parse("00000000000000000000000000").unwrap();
        assert_eq!(s.0, [0u8; 16]);
        let s = Secret([0x42; 16]);
        assert_eq!(s.encode(), "89144gj289144gj289144gj288");
        let k = s.keys();
        assert_eq!(k.claim_token(), "5afZeGlOYEASJ0SfNwMNRCBmGES_1QpG7JIxmyAPvS8");
        assert_eq!(k.claim_hash(), "7d6867f5035879db744c92ef7ddf44a1d87f68ed2be2d14a73dc6a3037585534");
    }
}
