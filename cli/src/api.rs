//! HTTP client for the envclip edge API.

use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use ureq::tls::{Certificate, RootCerts, TlsConfig};

pub struct Client {
    agent: ureq::Agent,
    pub base: String,
}

#[derive(Deserialize)]
pub struct Created {
    pub id: String,
    pub expires_at: u64,
}

#[derive(Deserialize)]
pub struct Status {
    pub expires_at: u64,
}

#[derive(Deserialize)]
struct Claimed {
    ciphertext: String,
}

#[derive(Deserialize)]
struct ApiError {
    error: String,
}

pub enum Error {
    /// Wrong key, already claimed, revoked or expired. Deliberately indistinguishable.
    NotFound,
    Server(u16, String),
    Net(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound => f.write_str(crate::i18n::tr(
                "drop not found — wrong key, already claimed, revoked, or expired",
                "未找到该投递 —— 密钥错误、已被领取、已撤销或已过期",
            )),
            Error::Server(code, msg) => {
                write!(f, "{} {code}: {msg}", crate::i18n::tr("server error", "服务端错误"))
            }
            Error::Net(e) => write!(f, "{}: {e}", crate::i18n::tr("network error", "网络错误")),
        }
    }
}

/// Mozilla roots plus whatever the OS (or `SSL_CERT_FILE`) trusts, so envclip
/// works both on bare containers and behind TLS-inspecting sandbox proxies.
fn roots() -> RootCerts {
    let mut certs: Vec<Certificate<'static>> = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|c| Certificate::from_der(c.as_ref()).to_owned())
        .collect();
    for c in rustls_native_certs::load_native_certs().certs {
        certs.push(Certificate::from_der(c.as_ref()).to_owned());
    }
    RootCerts::Specific(Arc::new(certs))
}

pub fn default_server() -> Option<String> {
    std::env::var("ENVCLIP_SERVER")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| option_env!("ENVCLIP_DEFAULT_SERVER").filter(|s| !s.is_empty()).map(str::to_string))
}

impl Client {
    pub fn new(base: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(concat!("envclip/", env!("CARGO_PKG_VERSION")))
            .tls_config(TlsConfig::builder().root_certs(roots()).build())
            .build();
        Client { agent: config.into(), base: base.trim_end_matches('/').to_string() }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base)
    }

    fn finish(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<(u16, String), Error> {
        let resp = resp.map_err(|e| Error::Net(e.to_string()))?;
        let code = resp.status().as_u16();
        let body = resp.into_body().read_to_string().map_err(|e| Error::Net(e.to_string()))?;
        match code {
            200..=299 => Ok((code, body)),
            404 => Err(Error::NotFound),
            _ => {
                // Non-JSON bodies (proxies, HTML error pages) are noise; keep the status only.
                let msg = serde_json::from_str::<ApiError>(&body)
                    .map(|e| e.error)
                    .unwrap_or_else(|_| crate::i18n::tr("unexpected response", "意外的响应").to_string());
                Err(Error::Server(code, msg))
            }
        }
    }

    pub fn create(&self, ciphertext: &str, claim_hash: &str, ttl: Duration) -> Result<Created, Error> {
        let body = serde_json::json!({
            "ciphertext": ciphertext,
            "claim_hash": claim_hash,
            "ttl": ttl.as_secs(),
        });
        let r = self
            .agent
            .post(self.url("/drops"))
            .header("content-type", "application/json")
            .send(body.to_string());
        let (_, body) = Self::finish(r)?;
        serde_json::from_str(&body).map_err(|e| Error::Net(e.to_string()))
    }

    pub fn claim(&self, id: &str, token: &str) -> Result<String, Error> {
        let r = self
            .agent
            .post(self.url(&format!("/drops/{id}/claim")))
            .header("authorization", format!("Bearer {token}"))
            .send_empty();
        let (_, body) = Self::finish(r)?;
        serde_json::from_str::<Claimed>(&body).map(|c| c.ciphertext).map_err(|e| Error::Net(e.to_string()))
    }

    pub fn status(&self, id: &str, token: &str) -> Result<Status, Error> {
        let r = self
            .agent
            .get(self.url(&format!("/drops/{id}")))
            .header("authorization", format!("Bearer {token}"))
            .call();
        let (_, body) = Self::finish(r)?;
        serde_json::from_str(&body).map_err(|e| Error::Net(e.to_string()))
    }

    pub fn revoke(&self, id: &str, token: &str) -> Result<(), Error> {
        let r = self
            .agent
            .delete(self.url(&format!("/drops/{id}")))
            .header("authorization", format!("Bearer {token}"))
            .call();
        Self::finish(r).map(|_| ())
    }

    pub fn drop_url(&self, id: &str) -> String {
        format!("{}/c/{id}", self.base)
    }
}

/// Splits `https://host/c/ID` into (`https://host`, `ID`). A bare ID uses the
/// default server.
pub fn parse_ref(r: &str) -> Option<(String, String)> {
    let r = r.trim().trim_end_matches('/');
    let valid = |id: &str| !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric());
    if let Some(i) = r.rfind("/c/") {
        let (base, id) = (&r[..i], &r[i + 3..]);
        let id = id.split(['?', '#']).next().unwrap_or("");
        return (valid(id) && (base.starts_with("https://") || base.starts_with("http://")))
            .then(|| (base.to_string(), id.to_string()));
    }
    if valid(r) {
        return default_server().map(|s| (s, r.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_ref;

    #[test]
    fn refs() {
        assert_eq!(
            parse_ref("https://envclip.example/c/abc123/"),
            Some(("https://envclip.example".into(), "abc123".into()))
        );
        assert_eq!(
            parse_ref("http://localhost:8787/c/x9?x=1"),
            Some(("http://localhost:8787".into(), "x9".into()))
        );
        assert_eq!(parse_ref("ftp://h/c/abc"), None);
        assert_eq!(parse_ref("https://h/c/a-b"), None);
    }
}
