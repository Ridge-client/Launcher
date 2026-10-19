//! Concurrent website health checks.

use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::{stream, StreamExt};
use reqwest::{redirect, Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::Entry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Status {
    /// Responded with a 2xx/3xx status.
    Active { code: u16 },
    /// Reachable but refused an automated client (401/403/429/999).
    Guarded { code: u16 },
    /// Unreachable, timed out, or returned a not-found/server error.
    Down { reason: String },
}

impl Status {
    pub fn is_down(&self) -> bool {
        matches!(self, Status::Down { .. })
    }
}

#[derive(Debug, Serialize)]
pub struct CheckResult<'a> {
    #[serde(flatten)]
    pub entry: Entry<'a>,
    pub status: Status,
    pub elapsed_ms: u128,
}

pub fn client(timeout: Duration) -> Result<Client> {
    Ok(Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout.min(Duration::from_secs(5)))
        .redirect(redirect::Policy::limited(10))
        .user_agent(concat!(
            "Ridge/",
            env!("CARGO_PKG_VERSION"),
            " (OSINT catalog health check)"
        ))
        .build()?)
}

fn classify(code: StatusCode) -> Status {
    let c = code.as_u16();
    if code.is_success() || code.is_redirection() {
        Status::Active { code: c }
    } else if matches!(c, 401 | 403 | 429 | 999) {
        Status::Guarded { code: c }
    } else {
        Status::Down {
            reason: format!("HTTP {c}"),
        }
    }
}

fn describe(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timeout".into()
    } else if err.is_connect() {
        "connection failed".into()
    } else if err.is_redirect() {
        "redirect loop".into()
    } else {
        "request error".into()
    }
}

/// Checks one URL with HEAD, falling back to GET because many servers mishandle HEAD.
pub async fn check_url(client: &Client, url: &str) -> Status {
    if let Ok(resp) = client.head(url).send().await {
        let status = classify(resp.status());
        if matches!(status, Status::Active { .. }) {
            return status;
        }
    }
    match client.get(url).send().await {
        Ok(resp) => classify(resp.status()),
        Err(err) => Status::Down {
            reason: describe(&err),
        },
    }
}

/// Checks all entries with bounded concurrency, calling `on_done` after each one.
pub async fn check_all<'a>(
    entries: Vec<Entry<'a>>,
    concurrency: usize,
    timeout: Duration,
    on_done: impl Fn(&CheckResult<'a>),
) -> Result<Vec<CheckResult<'a>>> {
    let client = client(timeout)?;
    let client = &client;
    let on_done = &on_done;
    let results = stream::iter(entries)
        .map(|entry| async move {
            let start = Instant::now();
            let status = check_url(client, &entry.tool.url).await;
            let result = CheckResult {
                entry,
                status,
                elapsed_ms: start.elapsed().as_millis(),
            };
            on_done(&result);
            result
        })
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serves `status_line` to every request on a random local port.
    async fn serve(status_line: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    let resp = format!(
                        "HTTP/1.1 {status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn active_site() {
        let url = serve("200 OK").await;
        let c = client(Duration::from_secs(5)).unwrap();
        assert_eq!(check_url(&c, &url).await, Status::Active { code: 200 });
    }

    #[tokio::test]
    async fn guarded_site() {
        let url = serve("403 Forbidden").await;
        let c = client(Duration::from_secs(5)).unwrap();
        assert_eq!(check_url(&c, &url).await, Status::Guarded { code: 403 });
    }

    #[tokio::test]
    async fn not_found_is_down() {
        let url = serve("404 Not Found").await;
        let c = client(Duration::from_secs(5)).unwrap();
        assert!(check_url(&c, &url).await.is_down());
    }

    #[tokio::test]
    async fn closed_port_is_down() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let c = client(Duration::from_secs(2)).unwrap();
        assert!(check_url(&c, &format!("http://{addr}/")).await.is_down());
    }
}
