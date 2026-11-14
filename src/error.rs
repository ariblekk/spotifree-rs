use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("spotify returned status {status}: {body}")]
    Status { status: u16, body: String },

    #[error("missing persisted query hash for {0}; Spotify may have renamed the operation")]
    MissingQuery(String),

    #[error("spotify response has no data: {0}")]
    NoData(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// True when a retry after rediscovery may succeed: the persisted hash or
    /// token went stale because Spotify deployed a new web-player build.
    pub fn is_discovery_stale(&self) -> bool {
        match self {
            Error::MissingQuery(_) => true,
            Error::Status { status, .. } => *status == 401 || *status == 403,
            Error::NoData(body) => {
                let body = body.to_ascii_lowercase();
                body.contains("persistedquery")
                    || body.contains("persisted query")
                    || body.contains("sha256hash")
            }
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_rotation_failures_are_retryable() {
        assert!(Error::MissingQuery("getTrack".into()).is_discovery_stale());
        assert!(
            Error::Status {
                status: 401,
                body: String::new()
            }
            .is_discovery_stale()
        );
        assert!(
            Error::NoData(r#"[{"message":"PersistedQueryNotFound"}]"#.into())
                .is_discovery_stale()
        );

        assert!(
            !Error::Status {
                status: 500,
                body: String::new()
            }
            .is_discovery_stale()
        );
        assert!(!Error::NoData("429 Too Many Requests".into()).is_discovery_stale());
        assert!(!Error::Protocol("invalid id".into()).is_discovery_stale());
    }
}
