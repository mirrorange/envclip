//! envclip envelope v1 — see docs/PROTOCOL.md.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const ID_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
const SALT: &[u8] = b"envclip/v1";
const VERSION: u8 = 1;

pub type Vars = Vec<(String, String)>;

#[derive(Debug, PartialEq)]
pub enum Error {
    BadKey,
    BadEnvelope,
    Decrypt,
}

#[derive(Serialize, Deserialize)]
struct Payload1 {
    v: u8,
    vars: Vars,
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("OS random source unavailable");
    b
}

pub fn new_id() -> String {
    // 32 divides 256, so masking keeps the distribution uniform
    random::<12>()
        .iter()
        .map(|b| ID_ALPHABET[(b & 31) as usize] as char)
        .collect()
}

pub fn is_id(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| ID_ALPHABET.contains(&b))
}

pub fn is_name(s: &str) -> bool {
    let mut b = s.bytes();
    matches!(b.next(), Some(c) if c == b'_' || c.is_ascii_alphabetic())
        && b.all(|c| c == b'_' || c.is_ascii_alphanumeric())
}

/// A 32-byte clip key. Zeroed on drop.
pub struct Key(Zeroizing<[u8; 32]>);

impl Key {
    pub fn generate() -> Self {
        Key(Zeroizing::new(random::<32>()))
    }

    pub fn parse(s: &str) -> Result<Self, Error> {
        let raw = Zeroizing::new(B64.decode(s.trim()).map_err(|_| Error::BadKey)?);
        let arr: [u8; 32] = raw.as_slice().try_into().map_err(|_| Error::BadKey)?;
        Ok(Key(Zeroizing::new(arr)))
    }

    pub fn encode(&self) -> String {
        B64.encode(*self.0)
    }

    fn derive(&self, info: &str) -> Zeroizing<[u8; 32]> {
        let mut out = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(SALT), &*self.0)
            .expand(info.as_bytes(), &mut *out)
            .expect("32 bytes is a valid HKDF length");
        out
    }

    /// Proves knowledge of the key to the server without revealing it.
    pub fn auth_token(&self) -> String {
        B64.encode(*self.derive("auth"))
    }

    pub fn auth_hash(&self) -> String {
        Sha256::digest(*self.derive("auth"))
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(&*self.derive("enc")).expect("32-byte key")
    }
}

fn aad(id: &str) -> Vec<u8> {
    format!("envclip:v1:{id}").into_bytes()
}

pub fn seal(id: &str, key: &Key, vars: &Vars) -> String {
    seal_with_nonce(id, key, vars, random::<12>())
}

fn seal_with_nonce(id: &str, key: &Key, vars: &Vars, nonce: [u8; 12]) -> String {
    let plain = Zeroizing::new(
        serde_json::to_vec(&Payload1 {
            v: 1,
            vars: vars.clone(),
        })
        .expect("serializable"),
    );
    let ct = key
        .cipher()
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: &aad(id),
            },
        )
        .expect("AES-GCM encryption");
    let mut out = Vec::with_capacity(1 + 12 + ct.len());
    out.push(VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    B64.encode(out)
}

pub fn unseal(id: &str, key: &Key, blob: &str) -> Result<Vars, Error> {
    let raw = B64.decode(blob.trim()).map_err(|_| Error::BadEnvelope)?;
    if raw.len() < 1 + 12 + 16 || raw[0] != VERSION {
        return Err(Error::BadEnvelope);
    }
    let plain = Zeroizing::new(
        key.cipher()
            .decrypt(
                Nonce::from_slice(&raw[1..13]),
                Payload {
                    msg: &raw[13..],
                    aad: &aad(id),
                },
            )
            .map_err(|_| Error::Decrypt)?,
    );
    let doc: Payload1 = serde_json::from_slice(&plain).map_err(|_| Error::BadEnvelope)?;
    if doc.v != 1
        || doc
            .vars
            .iter()
            .any(|(n, v)| !is_name(n) || v.contains('\0'))
    {
        return Err(Error::BadEnvelope);
    }
    Ok(doc.vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Vector {
        id: String,
        key: String,
        nonce: String,
        vars: Vars,
        auth: String,
        auth_hash: String,
        blob: String,
    }

    fn vector() -> Vector {
        serde_json::from_str(include_str!("../../testdata/v1.json")).unwrap()
    }

    #[test]
    fn v1_vector_matches_web_implementation() {
        let v = vector();
        let key = Key::parse(&v.key).unwrap();
        let nonce: [u8; 12] = B64.decode(&v.nonce).unwrap().try_into().unwrap();
        assert_eq!(key.auth_token(), v.auth);
        assert_eq!(key.auth_hash(), v.auth_hash);
        assert_eq!(seal_with_nonce(&v.id, &key, &v.vars, nonce), v.blob);
        assert_eq!(unseal(&v.id, &key, &v.blob).unwrap(), v.vars);
    }

    #[test]
    fn bound_to_id_and_key() {
        let v = vector();
        let key = Key::parse(&v.key).unwrap();
        assert_eq!(unseal("zzzzzzzzzzzz", &key, &v.blob), Err(Error::Decrypt));
        assert_eq!(
            unseal(&v.id, &Key::generate(), &v.blob),
            Err(Error::Decrypt)
        );
    }

    #[test]
    fn round_trip() {
        let id = new_id();
        assert!(is_id(&id));
        let key = Key::generate();
        let again = Key::parse(&key.encode()).unwrap();
        let vars = vec![("A".into(), "".into()), ("B_2".into(), "x'y\"z\n".into())];
        assert_eq!(unseal(&id, &again, &seal(&id, &key, &vars)).unwrap(), vars);
    }

    #[test]
    fn names_and_keys() {
        assert!(is_name("_A1") && is_name("api_token"));
        assert!(!is_name("1A") && !is_name("A-B") && !is_name(""));
        assert_eq!(Key::parse("short").err(), Some(Error::BadKey));
        assert!(!is_id("0123456789ai"));
    }
}
