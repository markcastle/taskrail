//! Signing in to remote servers that use the MCP authorization flow.
//!
//! The SDK does the OAuth work: metadata discovery, dynamic client
//! registration, PKCE, the token exchange, refresh. This module adds the two
//! things a desktop app needs around it: a loopback listener that catches the
//! browser's redirect, and a credential store that the shell backs with the
//! operating system's keychain. Nothing here opens the browser; the shell
//! does that, so the connector crate stays free of UI concerns.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rmcp::transport::auth::{
    AuthError, AuthorizationManager, AuthorizationRequest, CredentialStore, OAuthState,
    StoredCredentials,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Where serialized credentials go. The shell backs this with the OS
/// keychain; tests use memory.
pub trait CredentialPersistence: Send + Sync {
    fn load(&self) -> Result<Option<String>, String>;
    fn save(&self, json: &str) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

/// Everything a session may need to be let in.
#[derive(Clone, Default)]
pub struct SessionAuth {
    /// A static token, sent as `Authorization: Bearer`.
    pub bearer: Option<String>,
    /// Stored OAuth credentials from an earlier sign-in.
    pub oauth: Option<Arc<dyn CredentialPersistence>>,
}

impl SessionAuth {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self {
            bearer: Some(token.into()),
            oauth: None,
        }
    }
}

struct PersistedStore(Arc<dyn CredentialPersistence>);

#[async_trait]
impl CredentialStore for PersistedStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let Some(json) = self.0.load().map_err(AuthError::CredentialStoreError)? else {
            return Ok(None);
        };
        serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let json = serde_json::to_string(&credentials)
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))?;
        self.0.save(&json).map_err(AuthError::CredentialStoreError)
    }

    async fn clear(&self) -> Result<(), AuthError> {
        self.0.clear().map_err(AuthError::CredentialStoreError)
    }
}

/// An authorized manager rebuilt from stored credentials, or `None` when
/// nobody has signed in to this server yet.
pub async fn restore(
    url: &str,
    store: Arc<dyn CredentialPersistence>,
) -> Result<Option<AuthorizationManager>, AuthError> {
    let mut manager = AuthorizationManager::new(url).await?;
    manager.set_credential_store(PersistedStore(store));
    if manager.initialize_from_store().await? {
        Ok(Some(manager))
    } else {
        Ok(None)
    }
}

/// A sign-in in progress: the browser has been pointed at `auth_url` and
/// `listener` is waiting for it to come back.
pub struct SignInStart {
    pub auth_url: String,
    listener: CallbackListener,
    flow: OAuthState,
}

/// Discover the server's authorization setup, register the client, and
/// prepare the URL the person must visit.
pub async fn begin(
    url: &str,
    store: Arc<dyn CredentialPersistence>,
) -> Result<SignInStart, AuthError> {
    let listener = CallbackListener::bind()
        .await
        .map_err(|e| AuthError::InternalError(format!("could not open a local port: {e}")))?;
    let mut manager = AuthorizationManager::new(url).await?;
    manager.set_credential_store(PersistedStore(store));
    let mut flow = OAuthState::Unauthorized(manager);
    flow.start_authorization(
        AuthorizationRequest::new(listener.redirect_uri()).with_client_name("Task Rail"),
    )
    .await?;
    let auth_url = flow.get_authorization_url().await?;
    Ok(SignInStart {
        auth_url,
        listener,
        flow,
    })
}

impl SignInStart {
    /// Wait for the redirect, exchange the code, and leave the credentials
    /// in the store. `timeout` bounds how long the browser may take.
    pub async fn finish(self, timeout: Duration) -> Result<(), AuthError> {
        let callback_url = self
            .listener
            .wait(timeout)
            .await
            .map_err(AuthError::InternalError)?;
        let mut flow = self.flow;
        // Handling the callback exchanges the code, saves the credentials
        // through the store, and moves the state to Authorized itself.
        flow.handle_callback_url(&callback_url).await
    }
}

/// One-shot HTTP listener on a loopback port for the OAuth redirect.
pub struct CallbackListener {
    listener: TcpListener,
    port: u16,
}

impl CallbackListener {
    pub async fn bind() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        Ok(Self { listener, port })
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    /// Accept connections until one asks for `/callback`, answer it with a
    /// page the person can close, and return the full redirect URL.
    pub async fn wait(self, timeout: Duration) -> Result<String, String> {
        tokio::time::timeout(timeout, self.serve())
            .await
            .map_err(|_| "the sign-in was not completed in time".to_owned())?
    }

    async fn serve(self) -> Result<String, String> {
        loop {
            let (mut stream, _) = self.listener.accept().await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 16 * 1024];
            let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let target = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_owned();
            if target.starts_with("/callback") {
                let _ = stream
                    .write_all(response(200, SIGNED_IN_PAGE).as_bytes())
                    .await;
                let _ = stream.shutdown().await;
                return Ok(format!("http://127.0.0.1:{}{}", self.port, target));
            }
            let _ = stream
                .write_all(response(404, "Not here.").as_bytes())
                .await;
            let _ = stream.shutdown().await;
        }
    }
}

fn response(status: u16, body: &str) -> String {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

const SIGNED_IN_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Task Rail</title>
<style>body{font-family:system-ui,sans-serif;background:#1b1d22;color:#e8eaef;display:grid;place-items:center;height:100vh;margin:0}
main{text-align:center}h1{font-size:22px;margin:0 0 8px}p{color:#9aa0ab;margin:0}</style></head>
<body><main><h1>Signed in to Task Rail</h1><p>You can close this tab and go back to the rail.</p></main></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Memory(Mutex<Option<String>>);

    impl CredentialPersistence for Memory {
        fn load(&self) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, json: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(json.to_owned());
            Ok(())
        }
        fn clear(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    #[tokio::test]
    async fn the_credential_store_round_trips_through_json() {
        let memory: Arc<dyn CredentialPersistence> = Arc::new(Memory::default());
        let store = PersistedStore(memory.clone());
        assert!(store.load().await.unwrap().is_none());
        store
            .save(StoredCredentials::new(
                "client-1".into(),
                None,
                vec!["read".into()],
                None,
            ))
            .await
            .unwrap();
        let back = store.load().await.unwrap().unwrap();
        assert_eq!(back.client_id, "client-1");
        assert_eq!(back.granted_scopes, ["read"]);
        store.clear().await.unwrap();
        assert!(memory.load().unwrap().is_none());
    }

    #[tokio::test]
    async fn the_listener_answers_the_callback_and_ignores_other_paths() {
        let listener = CallbackListener::bind().await.unwrap();
        let port = listener.port;
        assert_eq!(
            listener.redirect_uri(),
            format!("http://127.0.0.1:{port}/callback")
        );
        let waiter = tokio::spawn(listener.wait(Duration::from_secs(5)));

        let get = |path: &str| {
            let path = path.to_owned();
            async move {
                let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .unwrap();
                s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                    .await
                    .unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).await.unwrap();
                out
            }
        };
        assert!(get("/favicon.ico").await.starts_with("HTTP/1.1 404"));
        let ok = get("/callback?code=abc&state=xyz").await;
        assert!(ok.starts_with("HTTP/1.1 200"));
        assert!(ok.contains("Signed in"));
        let url = waiter.await.unwrap().unwrap();
        assert_eq!(
            url,
            format!("http://127.0.0.1:{port}/callback?code=abc&state=xyz")
        );
    }

    #[tokio::test]
    async fn the_listener_times_out_when_nobody_comes_back() {
        let listener = CallbackListener::bind().await.unwrap();
        let err = listener.wait(Duration::from_millis(50)).await.unwrap_err();
        assert!(err.contains("not completed"));
    }
}
