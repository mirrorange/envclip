//! HTTP client for the envclip edge API.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

pub enum ApiError {
    NotFound,
    Conflict,
    Status(u16, String),
    Network(String),
}

impl ApiError {
    pub fn describe(&self, server: &str) -> String {
        match self {
            ApiError::NotFound => t!(
                "clip not found: already opened, revoked, expired, or the key is wrong",
                "未找到：已被打开、撤销、过期，或密钥错误"
            ),
            ApiError::Conflict => t!("id collision, try again", "id 冲突，请重试"),
            ApiError::Status(code, err) => t!("server replied {} {}", "服务端返回 {} {}", code, err),
            ApiError::Network(e) => t!("cannot reach {}: {}", "无法连接 {}：{}", server, e),
        }
    }
}

pub struct Client {
    base: String,
    agent: ureq::Agent,
}

/// System roots (so TLS-inspecting sandbox proxies work) plus Mozilla's bundle.
fn tls() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(cfg)
}

#[derive(Deserialize)]
struct Created {
    expires_at: u64,
}

#[derive(Deserialize)]
struct Opened {
    blob: String,
}

#[derive(Deserialize, Default)]
struct ErrBody {
    #[serde(default)]
    error: String,
}

fn map(e: ureq::Error) -> ApiError {
    match e {
        ureq::Error::Status(404, _) => ApiError::NotFound,
        ureq::Error::Status(409, _) => ApiError::Conflict,
        ureq::Error::Status(code, res) => {
            ApiError::Status(code, res.into_json::<ErrBody>().unwrap_or_default().error)
        }
        ureq::Error::Transport(t) => ApiError::Network(t.to_string()),
    }
}

impl Client {
    pub fn new(base: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .tls_config(tls())
            .try_proxy_from_env(true)
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("envclip/", env!("CARGO_PKG_VERSION")))
            .build();
        Client { base: base.trim_end_matches('/').to_string(), agent }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    pub fn create(&self, id: &str, auth_hash: &str, blob: &str, ttl: u64) -> Result<u64, ApiError> {
        let body = json!({ "id": id, "auth_hash": auth_hash, "blob": blob, "ttl": ttl });
        let res = self.agent.post(&self.url("/api/clips")).send_json(body).map_err(map)?;
        let c: Created = res.into_json().map_err(|e| ApiError::Network(e.to_string()))?;
        Ok(c.expires_at)
    }

    pub fn open(&self, id: &str, auth: &str) -> Result<String, ApiError> {
        let res = self
            .agent
            .post(&self.url(&format!("/api/clips/{id}/open")))
            .send_json(json!({ "auth": auth }))
            .map_err(map)?;
        let o: Opened = res.into_json().map_err(|e| ApiError::Network(e.to_string()))?;
        Ok(o.blob)
    }

    pub fn revoke(&self, id: &str, auth: &str) -> Result<(), ApiError> {
        self.agent
            .delete(&self.url(&format!("/api/clips/{id}")))
            .send_json(json!({ "auth": auth }))
            .map_err(map)?;
        Ok(())
    }
}
