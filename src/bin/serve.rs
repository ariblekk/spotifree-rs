//! Tiny HTTP server: serves the search page, the API page, and a live search
//! endpoint backed by a shared anonymous session.
//!
//! Security knobs, all optional, read from the environment:
//!
//! - `ALLOWED_ORIGIN`: comma-separated origins allowed for cross-site calls; when
//!   unset, the origin check is bypassed entirely
//! - `BIND`: listen address (default `0.0.0.0`)
//!
//! Same-origin requests are always allowed when `ALLOWED_ORIGIN` is set. Leave it
//! unset to accept any origin, which is what a native app or `curl` needs anyway.
//!
//! The API is public: it reads Spotify's catalogue and holds no user data. That
//! is also why there is no API key — a key shipped to a browser bundle is
//! readable by anyone, so it would only pretend to protect something. Put a CDN
//! (Cloudflare rate limiting, WAF) or a private network in front if you need more.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use spotifree_rs::{SearchResults, Session, UserAuth};
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};

/// Built frontend (Vite output). Override with `FRONTEND_DIR` when the assets
/// live outside the working directory, as in the container image.
fn frontend_dir() -> String {
    std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "web/dist".to_string())
}

const MAX_QUERY_LEN: usize = 120;
const MAX_LIMIT: usize = 50;
const MAX_ID_LEN: usize = 64;
const MAX_OFFSET: usize = 10_000;

/// Re-discovery cadence for the web-player build, so a rotated deployment does
/// not break every request until the process restarts.
const REFRESH_INTERVAL: Duration = Duration::from_secs(1800);

const TTL_SEARCH: Duration = Duration::from_secs(30);
const TTL_SUGGEST: Duration = Duration::from_secs(60);
const TTL_TRACK: Duration = Duration::from_secs(120);
const TTL_COLLECTION: Duration = Duration::from_secs(300);
const TTL_RECOMMENDATIONS: Duration = Duration::from_secs(300);
const TTL_CREDITS: Duration = Duration::from_secs(1800);
/// Charts update a few times a day at most, so they can be cached harder.
const TTL_CHARTS: Duration = Duration::from_secs(900);

/// Rows kept per chart. Spotify's editorial charts are `Top 50`, so asking for
/// more never returns more.
const CHART_ROWS: usize = 50;

/// Chart markets the API serves. Spotify's `monthly` chart playlists are not
/// listed on any country hub (and their artwork 404s), so only the two daily
/// charts with a stable id set are exposed.
const CHART_COUNTRIES: [&str; 2] = ["ID", "GLOBAL"];

/// `countryHubsPage` needs a real ISO code; "global" is served from Spotify's
/// own worldwide market.
const HUB_COUNTRY: &str = spotifree_rs::GLOBAL_HUB;

/// Upper bound on cached responses, so an unlimited endpoint cannot grow the
/// process without bound.
const MAX_CACHE_ENTRIES: usize = 512;

#[derive(Clone)]
struct Config {
    /// Empty means "accept any origin" (origin check bypassed).
    allowed_origins: Vec<String>,
    /// QRIS payment gateway (`QRIS_BASE_URL` + `QRIS_API_KEY`). `None` hides the
    /// donate QRIS card: without a key the gateway answers `401` anyway.
    qris: Option<Qris>,
}

/// Server-side QRIS gateway credentials. The `gopay_live_...` key stays here —
/// it must never reach the browser bundle.
#[derive(Clone)]
struct Qris {
    base: String,
    key: String,
}

impl Config {
    fn from_env() -> Self {
        let allowed_origins = std::env::var("ALLOWED_ORIGIN")
            .map(|value| {
                value
                    .split(',')
                    .map(|origin| origin.trim().trim_end_matches('/'))
                    .filter(|origin| !origin.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let qris = match (
            std::env::var("QRIS_BASE_URL"),
            std::env::var("QRIS_API_KEY"),
        ) {
            (Ok(base), Ok(key)) if !base.trim().is_empty() && !key.trim().is_empty() => {
                Some(Qris {
                    base: base.trim().trim_end_matches('/').to_string(),
                    key: key.trim().to_string(),
                })
            }
            _ => None,
        };

        Self {
            allowed_origins,
            qris,
        }
    }
}

/// A JSON answer kept for `ttl`. The body is pre-serialized so a hit costs one
/// hash-map lookup and no serialization.
struct CacheEntry {
    expires_at: Instant,
    body: Bytes,
}

/// Tiny TTL cache for upstream answers.
///
/// Spotify's catalogue changes slowly, while the same album, artist or playlist
/// is fetched over and over by the web UI. Entries are dropped on read once
/// expired, and the whole map is cleared when it grows past `MAX_CACHE_ENTRIES`
/// — no background task, no dependency.
#[derive(Default)]
struct Cache {
    entries: Mutex<HashMap<String, CacheEntry>>,
    /// Keys with a fetch in progress. A caller that finds its key here waits for
    /// the leader's answer instead of asking Spotify again.
    inflight: Mutex<HashMap<String, watch::Sender<Option<Bytes>>>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl Cache {
    /// Entry lookup without touching the hit/miss counters.
    fn peek(&self, key: &str) -> Option<Bytes> {
        let Ok(mut entries) = self.entries.lock() else {
            return None;
        };

        let now = Instant::now();
        let body = entries
            .get(key)
            .filter(|entry| entry.expires_at > now)
            .map(|entry| entry.body.clone());

        if body.is_none() {
            entries.remove(key);
        }

        body
    }

    fn get(&self, key: &str) -> Option<Bytes> {
        let body = self.peek(key);
        let counter = if body.is_some() { &self.hits } else { &self.misses };
        counter.fetch_add(1, Ordering::Relaxed);
        body
    }

    /// Serve `key` from the cache, or run `fetch` once and share its answer with
    /// every caller that arrives while it runs. Concurrent misses for the same
    /// key therefore cost one upstream call instead of one per request.
    async fn get_or_fetch(
        &self,
        key: String,
        ttl: Duration,
        fetch: impl Future<Output = Option<Bytes>>,
    ) -> Option<Bytes> {
        if let Some(body) = self.get(&key) {
            return Some(body);
        }

        let ticket = {
            let Ok(mut inflight) = self.inflight.lock() else {
                return fetch.await;
            };

            match inflight.get(&key) {
                Some(sender) => Ticket::Follower(sender.subscribe()),
                None => {
                    // Another caller may have filled the entry between our
                    // `get` and this lock; re-check without counting a miss.
                    if let Some(body) = self.peek(&key) {
                        return Some(body);
                    }

                    let (sender, _receiver) = watch::channel::<Option<Bytes>>(None);
                    inflight.insert(key.clone(), sender.clone());
                    Ticket::Leader(sender)
                }
            }
        };

        match ticket {
            Ticket::Leader(sender) => {
                let guard = InFlightGuard { cache: self, key };
                let body = fetch.await;

                if let Some(body) = &body {
                    self.insert(guard.key.clone(), body.clone(), ttl);
                    // Wake the followers; a failed fetch sends nothing, so the
                    // dropped sender tells them to try on their own.
                    let _ = sender.send(Some(body.clone()));
                }

                drop(guard);
                body
            }
            Ticket::Follower(mut receiver) => loop {
                if let Some(body) = (*receiver.borrow()).clone() {
                    return Some(body);
                }
                if receiver.changed().await.is_err() {
                    return None;
                }
            },
        }
    }

    fn entry_count(&self) -> usize {
        self.entries.lock().map(|entries| entries.len()).unwrap_or(0)
    }

    fn bytes_count(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.values().map(|entry| entry.body.len()).sum())
            .unwrap_or(0)
    }

    /// `(hits, misses)`, so `/healthz` can show whether the TTLs are useful.
    fn stats(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }

    fn insert(&self, key: String, body: Bytes, ttl: Duration) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };

        if entries.len() >= MAX_CACHE_ENTRIES {
            let now = Instant::now();
            entries.retain(|_, entry| entry.expires_at > now);

            if entries.len() >= MAX_CACHE_ENTRIES {
                entries.clear();
            }
        }

        entries.insert(
            key,
            CacheEntry {
                expires_at: Instant::now() + ttl,
                body,
            },
        );
    }
}

/// Who is responsible for a single-flight key: the leader runs the fetch, a
/// follower waits on the leader's `watch` channel.
enum Ticket {
    Leader(watch::Sender<Option<Bytes>>),
    Follower(watch::Receiver<Option<Bytes>>),
}

/// Removes the in-flight marker when the leader finishes, including if its task
/// is cancelled mid-fetch, so waiters never block on a leader that is gone.
struct InFlightGuard<'a> {
    cache: &'a Cache,
    key: String,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut inflight) = self.cache.inflight.lock() {
            inflight.remove(&self.key);
        }
    }
}

struct App {
    /// Shared and internally synchronized: queries run concurrently.
    session: Session,
    cache: Cache,
    config: Config,
    /// Bills the gateway already settled (`qris_id` → transaction id), so the
    /// browser sees `paid` the moment the webhook lands instead of waiting for
    /// the next poll round trip. Kept in memory on purpose: a donation is
    /// confirmed by the gateway, not by this process remembering it.
    paid: Mutex<HashMap<String, String>>,
    started_at: Instant,
}

/// Outbound client for the QRIS gateway (the Spotify session keeps its own).
static PAYMENTS: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("spotifree-rs")
        .build()
        .unwrap_or_default()
});

/// Donation presets, in rupiah. The UI offers these as buttons.
const DONATE_AMOUNTS: [u64; 4] = [10_000, 25_000, 50_000, 100_000];
/// Only a sanity ceiling: the gateway rejects `amount <= 0` on its own, so no
/// minimum is imposed here.
const DONATE_MAX: u64 = 5_000_000;

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<usize>,
    offset: Option<usize>,
    #[serde(rename = "type")]
    types: Option<String>,
}

#[derive(Default, Deserialize)]
struct SuggestParams {
    #[serde(default)]
    q: String,
}

#[derive(Default, Deserialize)]
struct DiscographyParams {
    offset: Option<usize>,
    limit: Option<usize>,
    #[serde(rename = "type")]
    category: Option<String>,
}

#[derive(Deserialize)]
struct ChartParams {
    /// `id` (Indonesia) or `global`; also accepts `ID`/`GLOBAL`.
    country: Option<String>,
    /// Which chart to rank: `daily` (default) or `alltime`.
    #[serde(rename = "top")]
    top: Option<String>,
    /// Accepted for compatibility; rows are always capped at 50.
    #[allow(dead_code)]
    limit: Option<usize>,
}

#[derive(Default, Deserialize)]
struct MeAuthParams {
    sp_dc: Option<String>,
    token: Option<String>,
}

#[derive(Default, Deserialize)]
struct MePageParams {
    limit: Option<usize>,
    offset: Option<usize>,
    sp_dc: Option<String>,
    token: Option<String>,
}

#[derive(Default, Deserialize)]
struct MeTopParams {
    range: Option<String>,
    time_range: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    sp_dc: Option<String>,
    token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env();

    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080);
    let addr = format!("{bind}:{port}");

    let session = Session::new().await?;
    let state = Arc::new(App {
        session,
        cache: Cache::default(),
        config,
        paid: Mutex::new(HashMap::new()),
        started_at: Instant::now(),
    });

    // Pay the discovery and token cost now instead of on the first user
    // request. Failures are logged; the next request retries on demand.
    if let Err(error) = state.session.warm().await {
        eprintln!("initial warm failed (retried on demand): {error}");
    } else if let Err(error) = state.session.authorize().await {
        eprintln!("initial authorize failed (retried on demand): {error}");
    }

    // Keep the discovered hashes and TOTP secret fresh across deployments.
    let refresher = state.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(REFRESH_INTERVAL);
        ticker.tick().await; // the first tick is immediate; skip it
        loop {
            ticker.tick().await;
            if let Err(error) = refresher.session.refresh().await {
                eprintln!("session refresh failed: {error}");
            }
        }
    });

    let dist = frontend_dir();
    // Middleware only needs the origin list, not the whole app state.
    let cors = Cors::from(&state.config);
    let app = Router::new()
        .route("/v1/search", get(search))
        .route("/v1/search/suggest", get(search_suggest))
        .route("/v1/charts", get(charts))
        .route("/v1/charts/{country}", get(charts_country))
        .route("/v1/browse", get(browse_categories))
        .route("/v1/genre/{id}", get(genre_detail))
        .route("/v1/track/{id}", get(track_detail))
        .route("/v1/track/{id}/credits", get(track_credits))
        .route("/v1/track/{id}/recommendations", get(track_recommendations))
        .route("/v1/track/{id}/related", get(track_recommendations))
        .route("/v1/album/{id}", get(album_detail))
        .route("/v1/artist/{id}", get(artist_detail))
        .route("/v1/artist/{id}/discography", get(artist_discography))
        .route("/v1/playlist/{id}", get(playlist_detail))
        .route("/v1/me", get(me_profile))
        .route("/v1/me/playlists", get(me_playlists))
        .route("/v1/me/tracks", get(me_tracks))
        .route("/v1/me/top", get(me_top))
        .route("/v1/me/top/tracks", get(me_top_tracks))
        .route("/v1/me/top/artists", get(me_top_artists))
        .route("/v1/donate", get(donate_info))
        .route("/v1/donate/qris", post(donate_qris))
        .route("/v1/donate/qris/{id}", get(donate_status))
        .route("/v1/donate/webhook", post(donate_webhook))
        .route("/healthz", get(health))
        // Pre-Vite URLs, still linked from bookmarks and notes.
        .route("/index.html", get(legacy_page))
        .route("/detail.html", get(legacy_page))
        .route("/api.html", get(legacy_page))
        .route("/docs.html", get(legacy_page))
        .route("/donate.html", get(legacy_page))
        .nest_service("/assets", ServeDir::new(format!("{dist}/assets")))
        // Client-side routes (`/`, `/api`, `/donate`, `/detail`) all render from
        // the same SPA entry point.
        .fallback_service(
            ServeDir::new(&dist).fallback(ServeFile::new(format!("{dist}/index.html"))),
        )
        .layer(middleware::from_fn(reject_missing_files))
        .layer(middleware::from_fn_with_state(
            state.config.clone(),
            reject_origin,
        ))
        // Outermost of the guards: a 403 still has to carry CORS headers,
        // otherwise the browser reports a network error instead of the reason.
        .layer(middleware::from_fn_with_state(cors, cors_preflight))
        .layer(middleware::from_fn(add_security_headers))
        // Outermost: compresses JSON and static assets when the client asks.
        .layer(CompressionLayer::new())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("spotifree-rs listening on http://{addr}");
    println!(
        "allowed_origins: {}",
        if state.config.allowed_origins.is_empty() {
            "any".to_string()
        } else {
            state.config.allowed_origins.join(",")
        },
    );

    // Let in-flight requests finish instead of cutting them on a restart.
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("spotifree-rs stopped");

    Ok(())
}

/// Resolves on Ctrl-C or SIGTERM (what Docker sends on `stop`/redeploy).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            // Without a handler, fall back to waiting forever on Ctrl-C.
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

/// Liveness plus the state that explains a `502`. `ok` tracks whether a usable
/// build has been discovered, which is the condition a request cannot work
/// around; the token is informational because it renews on demand.
async fn health(State(state): State<Arc<App>>) -> Response {
    let token = state.session.token().await;
    let warm = state.session.is_warm().await;
    let (hits, misses) = state.cache.stats();
    let uptime_secs = state.started_at.elapsed().as_secs();

    let body = serde_json::json!({
        "ok": warm,
        "warm": warm,
        "uptime_secs": uptime_secs,
        "hashes": state.session.hash_count().await,
        "token_valid": token.as_ref().is_some_and(|token| !token.is_expired()),
        "token_expires_in_ms": token.as_ref().map(|token| token.expires_in_ms()),
        "cache_entries": state.cache.entry_count(),
        "cache_bytes": state.cache.bytes_count(),
        "cache_hits": hits,
        "cache_misses": misses,
    });

    let status = if warm {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(body)).into_response()
}

/// Pages used to be real files (`/detail.html`). Keep those links working by
/// redirecting to the SPA route, query string included.
async fn legacy_page(uri: axum::http::Uri) -> axum::response::Redirect {
    let route = match uri.path() {
        "/index.html" => "/",
        "/detail.html" => "/detail",
        "/api.html" | "/docs.html" => "/docs",
        _ => "/donate",
    };
    let query = uri
        .query()
        .map(|query| format!("?{query}"))
        .unwrap_or_default();
    axum::response::Redirect::permanent(&format!("{route}{query}"))
}

async fn search(
    State(state): State<Arc<App>>,
    Query(params): Query<SearchParams>,
) -> Response {
    let query = params.q.trim();
    if query.is_empty() {
        return deny(StatusCode::BAD_REQUEST, "q must not be empty");
    }
    if query.chars().count() > MAX_QUERY_LEN {
        return deny(StatusCode::BAD_REQUEST, "q is too long");
    }

    let limit = params.limit.unwrap_or(10).clamp(1, MAX_LIMIT);
    let offset = params.offset.unwrap_or(0).min(MAX_OFFSET);
    let types = params.types.as_deref().map(parse_types).unwrap_or_default();
    let key = format!("search:{limit}:{offset}:{}:{query}", types.join(","));

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_SEARCH, async {
            let results = match state.session.search_page(query, limit, offset).await {
                Ok(results) => results,
                Err(error) => {
                    eprintln!("search failed: {error}");
                    return None;
                }
            };

            Some(Bytes::from(
                serde_json::to_vec(&keep_types(results, &types)).unwrap_or_default(),
            ))
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream search failed"),
    }
}

async fn search_suggest(
    State(state): State<Arc<App>>,
    Query(params): Query<SuggestParams>,
) -> Response {
    let query = params.q.trim();
    if query.is_empty() {
        return json_body(Bytes::from_static(
            b"{\"searchV2\":{\"query\":\"\",\"topResultsV2\":{\"itemsV2\":[]}}}",
        ));
    }
    if query.chars().count() > MAX_QUERY_LEN {
        return deny(StatusCode::BAD_REQUEST, "q is too long");
    }

    let key = format!("suggest:{query}");

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_SUGGEST, async {
            match state.session.search_suggestions(query).await {
                Ok(data) => Some(Bytes::from(
                    serde_json::to_vec(&data).unwrap_or_default(),
                )),
                Err(error) => {
                    eprintln!("search_suggestions failed: {error}");
                    None
                }
            }
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream search failed"),
    }
}

async fn track_detail(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    lookup(&state, Detail::Track, &id).await
}

async fn track_credits(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    if !valid_id(&id) {
        return deny(StatusCode::BAD_REQUEST, "invalid track id");
    }

    let key = format!("credits:{id}");

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_CREDITS, async {
            match state.session.track_credits(&id).await {
                Ok(data) => Some(Bytes::from(
                    serde_json::to_vec(&data).unwrap_or_default(),
                )),
                Err(error) => {
                    eprintln!("track_credits failed: {error}");
                    None
                }
            }
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream request failed"),
    }
}

async fn track_recommendations(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    if !valid_id(&id) {
        return deny(StatusCode::BAD_REQUEST, "invalid track id");
    }

    let key = format!("recommendations:{id}");

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_RECOMMENDATIONS, async {
            match state.session.track_recommendations(&id).await {
                Ok(data) => Some(Bytes::from(
                    serde_json::to_vec(&data).unwrap_or_default(),
                )),
                Err(error) => {
                    eprintln!("track_recommendations failed: {error}");
                    None
                }
            }
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream request failed"),
    }
}

/// Charts for Indonesia (`id`) or the world (`global`).
/// `GET /api/charts` uses the configured market; `GET /api/charts/{country}`
/// takes `id` or `global`.
async fn charts(
    State(state): State<Arc<App>>,
    Query(params): Query<ChartParams>,
) -> Response {
    charts_for(state, params).await
}

async fn charts_country(
    State(state): State<Arc<App>>,
    Path(country): Path<String>,
    Query(params): Query<ChartParams>,
) -> Response {
    charts_for(
        state,
        ChartParams {
            country: Some(country),
            ..params
        },
    )
    .await
}

async fn charts_for(state: Arc<App>, params: ChartParams) -> Response {
    // Default to the configured market when it is one we serve, else Indonesia.
    let wanted = params
        .country
        .or_else(|| std::env::var("MARKET").ok())
        .unwrap_or_else(|| "ID".to_string())
        .trim()
        .to_uppercase();

    if !CHART_COUNTRIES.contains(&wanted.as_str()) {
        return deny(
            StatusCode::BAD_REQUEST,
            "country must be one of: id, global",
        );
    }

    let country = if wanted == "GLOBAL" { HUB_COUNTRY } else { "ID" };
    let label = wanted.to_lowercase();

    let top = match params
        .top
        .as_deref()
        .map(str::trim)
        .filter(|top| !top.is_empty())
        .map(str::to_lowercase)
        .as_deref()
    {
        None | Some("daily") => "daily",
        Some("alltime") => "alltime",
        Some(_) => {
            return deny(StatusCode::BAD_REQUEST, "top must be one of: daily, alltime");
        }
    };

    let key = format!("charts:{label}:{top}");

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_CHARTS, async {
            let mut charts = match state
                .session
                .charts(country, Some(top), CHART_ROWS)
                .await
            {
                Ok(charts) => charts,
                Err(error) => {
                    eprintln!("charts failed: {error}");
                    return None;
                }
            };

            // Report the market name the caller asked for, not the hub used to
            // reach it.
            charts.country = label.clone();

            Some(Bytes::from(
                serde_json::to_vec(&charts).unwrap_or_default(),
            ))
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream charts failed"),
    }
}

async fn browse_categories(
    State(state): State<Arc<App>>,
) -> Response {
    let key = "browse:categories".to_string();

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_COLLECTION, async {
            match state.session.browse_categories().await {
                Ok(data) => Some(Bytes::from(
                    serde_json::to_vec(&data).unwrap_or_default(),
                )),
                Err(error) => {
                    eprintln!("browse_categories failed: {error}");
                    None
                }
            }
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream browse failed"),
    }
}

/// What the donate page needs: whether QRIS is configured and the presets.
async fn donate_info(State(state): State<Arc<App>>) -> Response {
    Json(serde_json::json!({
        "qris": state.config.qris.is_some(),
        "amounts": DONATE_AMOUNTS,
        "max": DONATE_MAX,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct DonateBody {
    amount: Option<u64>,
}

/// Create a dynamic QRIS bill through the gateway. The API key stays on this
/// side; the browser only ever sees the QR string and its status.
async fn donate_qris(
    State(state): State<Arc<App>>,
    body: Option<Json<DonateBody>>,
) -> Response {
    let Some(gateway) = state.config.qris.as_ref() else {
        return deny(StatusCode::SERVICE_UNAVAILABLE, "QRIS is not configured");
    };

    let amount = body.and_then(|Json(body)| body.amount).unwrap_or(0);
    if amount == 0 || amount > DONATE_MAX {
        return deny(
            StatusCode::BAD_REQUEST,
            "amount must be between 1 and 5000000",
        );
    }

    let payload = serde_json::json!({
        "amount": amount,
        "unique": true,
        "trx_id": format!("DONATE-{}", now_millis()),
    });

    let answer = PAYMENTS
        .post(format!("{}/create-qris", gateway.base))
        .header("x-api-key", &gateway.key)
        .json(&payload)
        .send()
        .await;

    let Ok(answer) = answer else {
        return deny(StatusCode::BAD_GATEWAY, "payment gateway unreachable");
    };

    if !answer.status().is_success() {
        let status = answer.status();
        eprintln!("create-qris failed: {status}");
        return deny(StatusCode::BAD_GATEWAY, "payment gateway rejected the bill");
    }

    match answer.json::<serde_json::Value>().await {
        Ok(data) if data.get("success").and_then(serde_json::Value::as_bool) == Some(false) => {
            deny(StatusCode::BAD_GATEWAY, "payment gateway rejected the bill")
        }
        Ok(data) => {
            let qr = data.get("data").cloned().unwrap_or(serde_json::Value::Null);
            let pick = |field: &str| qr.get(field).cloned().unwrap_or(serde_json::Value::Null);

            Json(serde_json::json!({
                "qris_id": pick("qris_id"),
                "trx_id": pick("trx_id"),
                "amount": pick("amount"),
                "base_amount": pick("base_amount"),
                "unique_code": pick("unique_code"),
                // EMVCo payload: rendered into a QR by the browser.
                "qris_code": pick("qris_code"),
                "expires_at": pick("expires_at"),
            }))
            .into_response()
        }
        Err(error) => {
            eprintln!("create-qris answer was not JSON: {error}");
            deny(StatusCode::BAD_GATEWAY, "payment gateway sent an odd answer")
        }
    }
}

/// Poll one bill. Public on the gateway side, so no key is needed here.
async fn donate_status(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    let Some(gateway) = state.config.qris.as_ref() else {
        return deny(StatusCode::SERVICE_UNAVAILABLE, "QRIS is not configured");
    };

    // Gateway ids look like `1731751234567_abc123`.
    if id.is_empty() || id.len() > MAX_ID_LEN || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return deny(StatusCode::BAD_REQUEST, "invalid payment id");
    }

    // Settled by the webhook already: answer without a round trip, which is what
    // makes the dialog flip to "paid" the moment the transfer lands.
    if let Ok(paid) = state.paid.lock()
        && let Some(transaction) = paid.get(&id)
    {
        return Json(serde_json::json!({
            "paid": true,
            "status": "PAID",
            "transaction_id": transaction,
        }))
        .into_response();
    }

    let answer = PAYMENTS
        .get(format!("{}/api/qr-status/{id}", gateway.base))
        .send()
        .await;

    let Ok(answer) = answer else {
        return deny(StatusCode::BAD_GATEWAY, "payment gateway unreachable");
    };

    if !answer.status().is_success() {
        return deny(StatusCode::BAD_GATEWAY, "payment gateway rejected the check");
    }

    match answer.json::<serde_json::Value>().await {
        Ok(data) => {
            let status = data.get("status").cloned().unwrap_or(serde_json::Value::Null);
            Json(serde_json::json!({
                "paid": data.get("paid").and_then(serde_json::Value::as_bool).unwrap_or(false),
                "status": status,
            }))
            .into_response()
        }
        Err(error) => {
            eprintln!("qr-status answer was not JSON: {error}");
            deny(StatusCode::BAD_GATEWAY, "payment gateway sent an odd answer")
        }
    }
}

/// Milliseconds since the epoch, used only to tag donation invoices.
fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// Webhooks older than this are refused, so a captured request cannot be
/// replayed tomorrow.
const WEBHOOK_TOLERANCE_SECS: u64 = 300;

/// `payment.settled` receiver. The gateway signs `{timestamp}.{raw body}` with
/// the application key (HMAC-SHA256, Stripe style), so the body is verified
/// before anything is written. Register this URL in the gateway dashboard as
/// `https://<host>/v1/donate/webhook`.
async fn donate_webhook(
    State(state): State<Arc<App>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(gateway) = state.config.qris.as_ref() else {
        return deny(StatusCode::SERVICE_UNAVAILABLE, "QRIS is not configured");
    };

    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };

    let (Some(timestamp), Some(signature)) = (
        header("x-gateway-timestamp"),
        header("x-gateway-signature"),
    ) else {
        return deny(StatusCode::BAD_REQUEST, "missing signature headers");
    };

    // Reject stale deliveries: the timestamp is part of the signed payload.
    let Ok(sent_at) = timestamp.parse::<i64>() else {
        return deny(StatusCode::BAD_REQUEST, "bad timestamp");
    };
    let now = (now_millis() / 1000) as i64;
    if (now - sent_at).abs() > WEBHOOK_TOLERANCE_SECS as i64 {
        return deny(StatusCode::BAD_REQUEST, "timestamp expired");
    }

    // `t=1731751234,v1=<hex>`; only the v1 HMAC is checked here.
    let Some(signature) = signature.split(',').find_map(|part| part.strip_prefix("v1=")) else {
        return deny(StatusCode::BAD_REQUEST, "missing v1 signature");
    };

    if !verify_webhook(&gateway.key, &timestamp, &body, signature) {
        return deny(StatusCode::UNAUTHORIZED, "invalid signature");
    }

    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return deny(StatusCode::BAD_REQUEST, "bad payload");
    };

    if payload.get("event").and_then(serde_json::Value::as_str) != Some("payment.settled") {
        // Acknowledged so the gateway stops retrying an event we do not handle.
        return Json(serde_json::json!({ "ok": true, "received": true })).into_response();
    }

    let qris_id = payload
        .get("qris_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if qris_id.is_empty() {
        return deny(StatusCode::BAD_REQUEST, "missing qris_id");
    }

    let transaction_id = payload
        .get("transaction")
        .and_then(|transaction| transaction.get("transaction_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    if let Ok(mut paid) = state.paid.lock() {
        // Bounded: a long-running process should not grow this forever.
        if paid.len() >= MAX_CACHE_ENTRIES {
            paid.clear();
        }
        paid.insert(qris_id.to_string(), transaction_id.to_string());
    }

    println!("donation settled: {qris_id} ({transaction_id})");
    Json(serde_json::json!({
        "ok": true,
        "received": true,
        "qris_id": qris_id,
    }))
    .into_response()
}

/// `HMAC_SHA256(key, "{timestamp}.{body}")` compared in constant time.
fn verify_webhook(key: &str, timestamp: &str, body: &[u8], expected_hex: &str) -> bool {
    use hmac::{Hmac, KeyInit, Mac};

    let Ok(mut mac) = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key.as_bytes()) else {
        return false;
    };
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);

    let Ok(expected) = decode_hex(expected_hex) else {
        return false;
    };

    mac.verify_slice(&expected).is_ok()
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ()> {
    if !value.len().is_multiple_of(2) {
        return Err(());
    }

    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|_| ())?;
            u8::from_str_radix(text, 16).map_err(|_| ())
        })
        .collect()
}

async fn genre_detail(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    if !valid_id(&id) {
        return deny(StatusCode::BAD_REQUEST, "invalid genre id");
    }

    let key = format!("genre:{id}");

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_COLLECTION, async {
            match state.session.genre(&id).await {
                Ok(data) => Some(Bytes::from(
                    serde_json::to_vec(&data).unwrap_or_default(),
                )),
                Err(error) => {
                    eprintln!("genre_detail failed: {error}");
                    None
                }
            }
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream genre failed"),
    }
}

async fn album_detail(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    lookup(&state, Detail::Album, &id).await
}

async fn artist_detail(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    lookup(&state, Detail::Artist, &id).await
}

async fn artist_discography(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
    Query(params): Query<DiscographyParams>,
) -> Response {
    if !valid_id(&id) {
        return deny(StatusCode::BAD_REQUEST, "invalid artist id");
    }

    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let category = params.category.as_deref();

    let key = format!(
        "discography:{id}:{}:{offset}:{limit}",
        category.unwrap_or("all")
    );

    let fetched = state
        .cache
        .get_or_fetch(key, TTL_COLLECTION, async {
            match state
                .session
                .artist_discography(&id, category, offset, limit)
                .await
            {
                Ok(data) => Some(Bytes::from(
                    serde_json::to_vec(&data).unwrap_or_default(),
                )),
                Err(error) => {
                    eprintln!("artist_discography failed: {error}");
                    None
                }
            }
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream request failed"),
    }
}

async fn playlist_detail(
    State(state): State<Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    lookup(&state, Detail::Playlist, &id).await
}

/// Extract caller-supplied Spotify user credentials (`Authorization: Bearer …`,
/// `X-Sp-Dc`, `X-Spotify-Token`, `Cookie: sp_dc=…`, query param, or server env).
fn extract_user_auth(
    headers: &HeaderMap,
    query_token: Option<&str>,
    query_sp_dc: Option<&str>,
) -> Result<UserAuth, Response> {
    let header_str = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };

    if let Some(auth) = header_str("authorization") {
        return UserAuth::parse(auth)
            .map_err(|err| deny(StatusCode::UNAUTHORIZED, &err.to_string()));
    }

    if let Some(sp_dc) = header_str("x-sp-dc") {
        return UserAuth::from_sp_dc(sp_dc)
            .map_err(|err| deny(StatusCode::UNAUTHORIZED, &err.to_string()));
    }

    if let Some(token) = header_str("x-spotify-token") {
        return UserAuth::parse(token)
            .map_err(|err| deny(StatusCode::UNAUTHORIZED, &err.to_string()));
    }

    if let Some(cookie) = header_str("cookie")
        && cookie.contains("sp_dc=")
    {
        return UserAuth::from_cookie(cookie)
            .map_err(|err| deny(StatusCode::UNAUTHORIZED, &err.to_string()));
    }

    if let Some(sp_dc) = query_sp_dc.map(str::trim).filter(|value| !value.is_empty()) {
        return UserAuth::from_sp_dc(sp_dc)
            .map_err(|err| deny(StatusCode::BAD_REQUEST, &err.to_string()));
    }

    if let Some(token) = query_token.map(str::trim).filter(|value| !value.is_empty()) {
        return UserAuth::parse(token)
            .map_err(|err| deny(StatusCode::BAD_REQUEST, &err.to_string()));
    }

    if let Ok(sp_dc) = std::env::var("SP_DC").or_else(|_| std::env::var("SPOTIFY_SP_DC"))
        && !sp_dc.trim().is_empty()
    {
        return UserAuth::from_sp_dc(&sp_dc)
            .map_err(|err| deny(StatusCode::UNAUTHORIZED, &err.to_string()));
    }

    if let Ok(token) = std::env::var("SPOTIFY_ACCESS_TOKEN")
        && !token.trim().is_empty()
    {
        return UserAuth::parse(&token)
            .map_err(|err| deny(StatusCode::UNAUTHORIZED, &err.to_string()));
    }

    Err(deny(
        StatusCode::UNAUTHORIZED,
        "missing Spotify credentials; send Authorization: Bearer <token|sp_dc>, X-Sp-Dc: <sp_dc>, or Cookie: sp_dc=<cookie>",
    ))
}

fn me_error_response(op: &str, error: spotifree_rs::Error) -> Response {
    match &error {
        spotifree_rs::Error::Status {
            status: 401 | 403, ..
        } => deny(
            StatusCode::UNAUTHORIZED,
            "invalid or expired Spotify credentials",
        ),
        spotifree_rs::Error::Protocol(message) => deny(StatusCode::BAD_REQUEST, message),
        _ => {
            eprintln!("{op} failed: {error}");
            deny(StatusCode::BAD_GATEWAY, "upstream request failed")
        }
    }
}

async fn me_profile(
    State(state): State<Arc<App>>,
    headers: HeaderMap,
    Query(params): Query<MeAuthParams>,
) -> Response {
    let auth = match extract_user_auth(&headers, params.token.as_deref(), params.sp_dc.as_deref()) {
        Ok(auth) => auth,
        Err(response) => return response,
    };

    match state.session.me(&auth).await {
        Ok(profile) => Json(profile).into_response(),
        Err(error) => me_error_response("me", error),
    }
}

async fn me_playlists(
    State(state): State<Arc<App>>,
    headers: HeaderMap,
    Query(params): Query<MePageParams>,
) -> Response {
    let auth = match extract_user_auth(&headers, params.token.as_deref(), params.sp_dc.as_deref()) {
        Ok(auth) => auth,
        Err(response) => return response,
    };

    let limit = params.limit.unwrap_or(20).clamp(1, MAX_LIMIT);
    let offset = params.offset.unwrap_or(0).min(MAX_OFFSET);

    match state.session.me_playlists(&auth, limit, offset).await {
        Ok(playlists) => Json(playlists).into_response(),
        Err(error) => me_error_response("me_playlists", error),
    }
}

async fn me_tracks(
    State(state): State<Arc<App>>,
    headers: HeaderMap,
    Query(params): Query<MePageParams>,
) -> Response {
    let auth = match extract_user_auth(&headers, params.token.as_deref(), params.sp_dc.as_deref()) {
        Ok(auth) => auth,
        Err(response) => return response,
    };

    let limit = params.limit.unwrap_or(20).clamp(1, MAX_LIMIT);
    let offset = params.offset.unwrap_or(0).min(MAX_OFFSET);

    match state.session.me_tracks(&auth, limit, offset).await {
        Ok(tracks) => Json(tracks).into_response(),
        Err(error) => me_error_response("me_tracks", error),
    }
}

async fn me_top(
    State(state): State<Arc<App>>,
    headers: HeaderMap,
    Query(params): Query<MeTopParams>,
) -> Response {
    let (include_tracks, include_artists) = match params
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        None | Some("all") => (true, true),
        Some("track") | Some("tracks") => (true, false),
        Some("artist") | Some("artists") => (false, true),
        Some(other) => {
            let tracks = other.split(',').any(|p| matches!(p.trim(), "track" | "tracks"));
            let artists = other.split(',').any(|p| matches!(p.trim(), "artist" | "artists"));
            if !tracks && !artists {
                return deny(
                    StatusCode::BAD_REQUEST,
                    "type must be one of: tracks, artists",
                );
            }
            (tracks, artists)
        }
    };

    me_top_for(state, headers, params, include_tracks, include_artists).await
}

async fn me_top_tracks(
    State(state): State<Arc<App>>,
    headers: HeaderMap,
    Query(params): Query<MeTopParams>,
) -> Response {
    me_top_for(state, headers, params, true, false).await
}

async fn me_top_artists(
    State(state): State<Arc<App>>,
    headers: HeaderMap,
    Query(params): Query<MeTopParams>,
) -> Response {
    me_top_for(state, headers, params, false, true).await
}

async fn me_top_for(
    state: Arc<App>,
    headers: HeaderMap,
    params: MeTopParams,
    include_tracks: bool,
    include_artists: bool,
) -> Response {
    let auth = match extract_user_auth(&headers, params.token.as_deref(), params.sp_dc.as_deref()) {
        Ok(auth) => auth,
        Err(response) => return response,
    };

    let limit = params.limit.unwrap_or(20).clamp(1, MAX_LIMIT);
    let offset = params.offset.unwrap_or(0).min(MAX_OFFSET);
    let range = params.range.as_deref().or(params.time_range.as_deref());

    match state
        .session
        .me_top(&auth, range, limit, offset, include_tracks, include_artists)
        .await
    {
        Ok(top) => Json(top).into_response(),
        Err(error) => me_error_response("me_top", error),
    }
}

#[derive(Clone, Copy)]
enum Detail {
    Track,
    Album,
    Artist,
    Playlist,
}

impl Detail {
    fn operation(self) -> &'static str {
        match self {
            Detail::Track => "getTrack",
            Detail::Album => "getAlbum",
            Detail::Artist => "queryArtistOverview",
            Detail::Playlist => "fetchPlaylist",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Detail::Track => "track",
            Detail::Album => "album",
            Detail::Artist => "artist",
            Detail::Playlist => "playlist",
        }
    }

    /// Tracks change more often (play counts aside, edits are common), so they
    /// get the shorter TTL; collections are effectively static.
    fn ttl(self) -> Duration {
        match self {
            Detail::Track => TTL_TRACK,
            _ => TTL_COLLECTION,
        }
    }
}

/// Validate the id, then pass the upstream `data` object through as JSON.
async fn lookup(state: &App, detail: Detail, id: &str) -> Response {
    if !valid_id(id) {
        return deny(
            StatusCode::BAD_REQUEST,
            &format!("invalid {} id", detail.label()),
        );
    }

    let key = format!("{}:{id}", detail.label());

    let fetched = state
        .cache
        .get_or_fetch(key, detail.ttl(), async {
            let result = match detail {
                Detail::Track => state.session.track(id).await,
                Detail::Album => state.session.album(id).await,
                Detail::Artist => state.session.artist(id).await,
                Detail::Playlist => state.session.playlist(id).await,
            };

            match result {
                Ok(data) => Some(Bytes::from(
                    serde_json::to_vec(&data).unwrap_or_default(),
                )),
                Err(error) => {
                    eprintln!("{} failed: {error}", detail.operation());
                    None
                }
            }
        })
        .await;

    match fetched {
        Some(body) => json_body(body),
        None => deny(StatusCode::BAD_GATEWAY, "upstream request failed"),
    }
}

/// Serve pre-serialized JSON. `Content-Type` is set here because the response no
/// longer goes through `Json`'s serializer.
fn json_body(body: Bytes) -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))],
        body,
    )
        .into_response()
}

/// Cross-origin guard, applied as middleware to `/api/...`. When `ALLOWED_ORIGIN`
/// is unset the check is bypassed; when set, the request's `Origin` must match
/// one of the listed origins or be same-origin. Browsers send `Origin` on
/// cross-site fetches. Non-browser clients (`curl`, Android) send no `Origin` and
/// are only subject to the API key.
async fn reject_origin(State(config): State<Config>, request: Request, next: Next) -> Response {
    if config.allowed_origins.is_empty() {
        return next.run(request).await;
    }

    let Some(origin) = request.headers().get(header::ORIGIN) else {
        return next.run(request).await;
    };

    let listed = origin
        .to_str()
        .map(|value| {
            let value = value.trim_end_matches('/');
            config
                .allowed_origins
                .iter()
                .any(|allowed| allowed == value)
        })
        .unwrap_or(false);

    if listed || same_origin(origin, request.headers()) {
        return next.run(request).await;
    }

    deny(StatusCode::FORBIDDEN, "origin is not allowed")
}

/// Originst the API answers to, for the CORS middleware.
#[derive(Clone)]
struct Cors {
    allowed: Arc<Vec<String>>,
}

impl From<&Config> for Cors {
    fn from(config: &Config) -> Self {
        Self {
            allowed: Arc::new(config.allowed_origins.clone()),
        }
    }
}

/// Headers that let a separately hosted UI (`VITE_API_BASE` pointing here) call
/// the API. Only origins listed in `ALLOWED_ORIGIN` are echoed back; a wildcard
/// would let any page on the internet read the API through a visitor's browser.
fn add_cors(cors: &Cors, request: &Request, mut response: Response) -> Response {
    if cors.allowed.is_empty() {
        return response;
    }

    // One cached answer per origin, so a CDN cannot hand `example.com` a body
    // carrying someone else's `Access-Control-Allow-Origin`.
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Origin"));

    let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
    else {
        return response;
    };

    let origin = origin.trim_end_matches('/');
    if !cors.allowed.iter().any(|allowed| allowed == origin) {
        return response;
    }

    if let Ok(value) = HeaderValue::from_str(origin) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }

    response
}

/// Answer the CORS preflight without touching the routes: `OPTIONS` is not
/// routed, so without this a cross-origin `GET` with a custom header fails
/// before it is ever sent.
async fn cors_preflight(
    State(cors): State<Cors>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() == axum::http::Method::OPTIONS
        && request.headers().contains_key(header::ORIGIN)
        && !cors.allowed.is_empty()
    {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|origin| origin.to_str().ok())
            .map(|origin| origin.trim_end_matches('/'))
            .unwrap_or_default();

        // A preflight from an origin we do not serve gets nothing back, so the
        // browser refuses to send the real request.
        if !cors.allowed.iter().any(|allowed| allowed == origin) {
            let mut response = deny(StatusCode::FORBIDDEN, "origin is not allowed");
            response
                .headers_mut()
                .append(header::VARY, HeaderValue::from_static("Origin"));
            return response;
        }

        let mut response = Response::new(axum::body::Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;

        let headers = response.headers_mut();
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, OPTIONS"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "accept, authorization, content-type, x-sp-dc, x-spotify-token",
            ),
        );
        headers.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("86400"),
        );

        return add_cors(&cors, &request, response);
    }

    let request_headers = request.headers().clone();
    let response = next.run(request).await;

    // `next.run` consumes the request; rebuild a headers-only view for CORS.
    let mut probe = Request::new(axum::body::Body::empty());
    *probe.headers_mut() = request_headers;
    add_cors(&cors, &probe, response)
}

/// Spotify identifiers are base62; anything else is rejected before it reaches
/// the upstream API.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_LEN && id.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// Parse `type=track,album` into normalized section names.
fn parse_types(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|kind| match kind.trim().to_ascii_lowercase().as_str() {
            "track" | "tracks" | "song" | "songs" => Some("track"),
            "artist" | "artists" => Some("artist"),
            "album" | "albums" => Some("album"),
            "playlist" | "playlists" => Some("playlist"),
            _ => None,
        })
        .map(str::to_string)
        .collect()
}

/// Drop the sections the caller did not ask for. An empty set keeps everything.
fn keep_types(results: SearchResults, types: &[String]) -> SearchResults {
    let keep = |kind: &str| types.is_empty() || types.iter().any(|wanted| wanted == kind);

    SearchResults {
        query: results.query,
        tracks: if keep("track") {
            results.tracks
        } else {
            Vec::new()
        },
        artists: if keep("artist") {
            results.artists
        } else {
            Vec::new()
        },
        albums: if keep("album") {
            results.albums
        } else {
            Vec::new()
        },
        playlists: if keep("playlist") {
            results.playlists
        } else {
            Vec::new()
        },
    }
}

fn same_origin(origin: &HeaderValue, headers: &HeaderMap) -> bool {
    let (Ok(origin), Some(host)) = (
        origin.to_str(),
        headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok()),
    ) else {
        return false;
    };

    let origin = origin.trim_end_matches('/');
    origin == format!("http://{host}") || origin == format!("https://{host}")
}

fn deny(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// Cache policy, shared by the browser and any CDN in front. The API answers are
/// public catalogue data, so they are safe to cache; the TTLs mirror the
/// in-process cache in `App`.
fn cache_control(path: &str) -> &'static str {
    if path.starts_with("/assets/") || path == "/favicon.svg" {
        "public, max-age=31536000, immutable"
    } else if path == "/healthz" {
        "no-store"
    } else if path == "/v1/donate" || path.starts_with("/v1/donate/") {
        // Payment answers are per-visitor and short-lived: never cache them.
        "no-store"
    } else if path == "/v1/me" || path.starts_with("/v1/me/") {
        // User-scoped data: never cache in browser or shared CDN.
        "private, no-store"
    } else if path == "/v1/search" || path == "/v1/search/suggest" {
        "public, max-age=30, stale-while-revalidate=60"
    } else if path.starts_with("/v1/charts")
        || path == "/v1/browse"
        || path.starts_with("/v1/genre/")
    {
        "public, max-age=900, stale-while-revalidate=600"
    } else if path.ends_with("/credits") {
        "public, max-age=1800, stale-while-revalidate=300"
    } else if path.ends_with("/recommendations") || path.ends_with("/related") {
        "public, max-age=300, stale-while-revalidate=120"
    } else if path.starts_with("/v1/track/") {
        "public, max-age=120, stale-while-revalidate=60"
    } else if path.starts_with("/v1/album/")
        || path.starts_with("/v1/artist/")
        || path.starts_with("/v1/playlist/")
    {
        "public, max-age=300, stale-while-revalidate=120"
    } else if path.starts_with("/v1/") || path.starts_with("/api/") {
        // Unknown API path: never let a shared cache keep a bad answer.
        "no-store"
    } else {
        // SPA shell: revalidate so a new deployment is visible immediately.
        "no-cache"
    }
}

/// Stamps hardening headers onto every response, plus the cache policy above.
async fn add_security_headers(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;

    let cache = cache_control(&path);

    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("SAMEORIGIN"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' https: data:; \
             style-src 'self'; font-src 'self'; \
             script-src 'self'; \
             frame-src https://www.youtube-nocookie.com; \
             connect-src 'self' https://lrc.red https://lrclib.net; base-uri 'none'; form-action 'self'",
        ),
    );
    response
}

/// `/v1/...` that no route matches must be a JSON 404, not the SPA shell.
fn reject_unknown_api(path: &str) -> Option<Response> {
    if let Some(rest) = path.strip_prefix("/v1/") {
        let known = rest == "search"
            || rest == "search/suggest"
            || rest == "charts"
            || rest == "browse"
            || rest == "me"
            || rest == "me/playlists"
            || rest == "me/tracks"
            || rest == "me/top"
            || rest == "me/top/tracks"
            || rest == "me/top/artists"
            || rest == "donate"
            || rest == "donate/qris"
            || rest.starts_with("donate/qris/")
            || rest == "donate/webhook"
            || [
                "track/",
                "album/",
                "artist/",
                "playlist/",
                "charts/",
                "genre/",
            ]
            .iter()
            .any(|prefix| rest.starts_with(prefix));

        if known {
            None
        } else {
            Some(deny(StatusCode::NOT_FOUND, "unknown API path"))
        }
    } else if path.starts_with("/api/") {
        Some(deny(StatusCode::NOT_FOUND, "API moved to /v1/"))
    } else {
        None
    }
}

/// A client-side route never carries a file extension, so anything that looks
/// like a file and is not on disk is a genuine 404 rather than the SPA shell.
/// Without this, a missing bundle or `/style.css` would answer `200` with HTML.
async fn reject_missing_files(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();

    if let Some(response) = reject_unknown_api(&path) {
        return response;
    }

    let last = path.rsplit('/').next().unwrap_or_default();

    // The legacy `.html` routes above redirect before the SPA ever sees them.
    let legacy = [
        "/index.html",
        "/detail.html",
        "/api.html",
        "/docs.html",
        "/donate.html",
    ];

    if path.starts_with("/v1/")
        || path.starts_with("/api/")
        || legacy.contains(&path.as_str())
        || !last.contains('.')
    {
        return next.run(request).await;
    }

    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    if tokio::fs::metadata(format!("{}{path}", frontend_dir()))
        .await
        .is_err()
    {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_sections() -> SearchResults {
        SearchResults {
            query: "lofi".into(),
            tracks: vec![Default::default()],
            artists: vec![Default::default()],
            albums: vec![Default::default()],
            playlists: vec![Default::default()],
        }
    }

    #[test]
    fn type_filter_drops_unrequested_sections() {
        let kept = keep_types(all_sections(), &parse_types("tracks,album"));

        assert_eq!(kept.query, "lofi");
        assert_eq!(kept.tracks.len(), 1);
        assert_eq!(kept.albums.len(), 1);
        assert!(kept.artists.is_empty());
        assert!(kept.playlists.is_empty());
    }

    #[test]
    fn unknown_or_empty_type_keeps_everything() {
        for raw in ["", "nonsense", ",,"] {
            let kept = keep_types(all_sections(), &parse_types(raw));
            assert_eq!(kept.tracks.len(), 1, "type={raw:?}");
            assert_eq!(kept.playlists.len(), 1, "type={raw:?}");
        }
    }

    #[test]
    fn ids_must_be_base62() {
        assert!(valid_id("23NgAzXBCwZdtwBn5hQzD5"));
        assert!(!valid_id(""));
        assert!(!valid_id("../etc/passwd"));
        assert!(!valid_id("id with spaces"));
        assert!(!valid_id(&"a".repeat(MAX_ID_LEN + 1)));
    }

    #[test]
    fn unknown_api_paths_are_json_404s() {
        assert!(reject_unknown_api("/v1/nonsense").is_some());
        assert!(reject_unknown_api("/v1/track").is_some());
        assert!(reject_unknown_api("/v1/lyrics/abc").is_some());
        assert!(reject_unknown_api("/api/track").is_some());

        assert!(reject_unknown_api("/docs").is_none());
        assert!(reject_unknown_api("/v1/search").is_none());
        assert!(reject_unknown_api("/v1/search/suggest").is_none());
        assert!(reject_unknown_api("/v1/browse").is_none());
        assert!(reject_unknown_api("/v1/genre/abc").is_none());
        assert!(reject_unknown_api("/v1/donate").is_none());
        assert!(reject_unknown_api("/v1/donate/qris").is_none());
        assert!(reject_unknown_api("/v1/donate/qris/123_abc").is_none());
        assert!(reject_unknown_api("/v1/donate/webhook").is_none());
        assert!(reject_unknown_api("/v1/donate/other").is_some());
        assert!(reject_unknown_api("/v1/me").is_none());
        assert!(reject_unknown_api("/v1/me/playlists").is_none());
        assert!(reject_unknown_api("/v1/me/tracks").is_none());
        assert!(reject_unknown_api("/v1/me/top").is_none());
        assert!(reject_unknown_api("/v1/me/top/tracks").is_none());
        assert!(reject_unknown_api("/v1/me/top/artists").is_none());
        assert!(reject_unknown_api("/v1/me/other").is_some());
        assert!(reject_unknown_api("/v1/track/abc/credits").is_none());
        assert!(reject_unknown_api("/v1/track/abc/recommendations").is_none());
        assert!(reject_unknown_api("/v1/artist/abc/discography").is_none());
        assert!(reject_unknown_api("/v1/album/abc").is_none());
        assert!(reject_unknown_api("/detail").is_none());
    }

    #[test]
    fn webhook_signature_must_match() {
        use hmac::{Hmac, KeyInit, Mac};

        let key = "gopay_live_test";
        let body = br#"{"event":"payment.settled","qris_id":"1_abc"}"#;
        let timestamp = "1731751234";

        let mut mac = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key.as_bytes()).unwrap();
        mac.update(format!("{timestamp}.").as_bytes());
        mac.update(body);
        let signature: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();

        assert!(verify_webhook(key, timestamp, body, &signature));
        // Wrong key, tampered body, tampered timestamp, malformed hex.
        assert!(!verify_webhook("other", timestamp, body, &signature));
        assert!(!verify_webhook(key, timestamp, b"{}", &signature));
        assert!(!verify_webhook(key, "1731751235", body, &signature));
        assert!(!verify_webhook(key, timestamp, body, "nothex"));
        assert!(!verify_webhook(key, timestamp, body, "abc"));
    }

    #[test]
    fn cache_stats_count_hits_and_misses() {
        let cache = Cache::default();
        assert!(cache.get("none").is_none());
        assert_eq!(cache.bytes_count(), 0);
        cache.insert("k".into(), Bytes::from_static(b"v"), Duration::from_secs(60));
        assert!(cache.get("k").is_some());
        assert_eq!(cache.bytes_count(), 1);

        assert_eq!(cache.stats(), (1, 1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_misses_fetch_once() {
        let cache = Arc::new(Cache::default());
        let calls = Arc::new(AtomicU64::new(0));

        let mut tasks = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            let calls = calls.clone();

            tasks.push(tokio::spawn(async move {
                cache
                    .get_or_fetch("k".into(), Duration::from_secs(60), async {
                        calls.fetch_add(1, Ordering::SeqCst);
                        // Hold the leader open so the others pile up behind it.
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Some(Bytes::from_static(b"v"))
                    })
                    .await
            }));
        }

        for task in tasks {
            assert_eq!(task.await.unwrap().as_deref(), Some(b"v".as_slice()));
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_fetch_leaves_no_inflight_marker() {
        let cache = Cache::default();

        let first = cache
            .get_or_fetch("k".into(), Duration::from_secs(60), async { None })
            .await;
        assert!(first.is_none());
        assert!(cache.inflight.lock().unwrap().is_empty());

        let second = cache
            .get_or_fetch("k".into(), Duration::from_secs(60), async {
                Some(Bytes::from_static(b"v"))
            })
            .await;
        assert_eq!(second.as_deref(), Some(b"v".as_slice()));
    }

    #[test]
    fn cors_only_echoes_listed_origins() {
        let cors = Cors {
            allowed: Arc::new(vec!["https://ui.example.com".to_string()]),
        };

        // Listed origin: echoed back, with `Vary` so a CDN keeps per-origin copies.
        let mut request = Request::new(axum::body::Body::empty());
        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://ui.example.com"),
        );
        let response = add_cors(&cors, &request, Response::new(axum::body::Body::empty()));
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            Some("https://ui.example.com")
        );
        assert!(response.headers().contains_key(header::VARY));

        // Stranger: no CORS header, so the browser refuses the response.
        let mut request = Request::new(axum::body::Body::empty());
        request
            .headers_mut()
            .insert(header::ORIGIN, HeaderValue::from_static("https://evil.test"));
        let response = add_cors(&cors, &request, Response::new(axum::body::Body::empty()));
        assert!(!response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));

        // Empty allow-list means "any origin": no CORS headers at all.
        let open = Cors {
            allowed: Arc::new(Vec::new()),
        };
        let mut request = Request::new(axum::body::Body::empty());
        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://ui.example.com"),
        );
        let response = add_cors(&open, &request, Response::new(axum::body::Body::empty()));
        assert!(!response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    }

    #[test]
    fn cache_control_matches_the_data_lifetime() {
        assert!(cache_control("/assets/index-abc.js").contains("immutable"));
        assert!(cache_control("/favicon.svg").contains("immutable"));
        assert_eq!(cache_control("/healthz"), "no-store");
        assert_eq!(cache_control("/v1/typo"), "no-store");
        assert_eq!(cache_control("/v1/donate"), "no-store");
        assert_eq!(cache_control("/v1/donate/qris"), "no-store");
        assert_eq!(cache_control("/v1/donate/qris/123_abc"), "no-store");
        assert_eq!(cache_control("/v1/me"), "private, no-store");
        assert_eq!(cache_control("/v1/me/playlists"), "private, no-store");
        assert_eq!(cache_control("/v1/me/tracks"), "private, no-store");
        assert_eq!(cache_control("/v1/me/top"), "private, no-store");
        assert_eq!(cache_control("/api/typo"), "no-store");
        assert_eq!(cache_control("/"), "no-cache");
        assert!(cache_control("/v1/search").starts_with("public, max-age=30"));
        assert!(cache_control("/v1/search/suggest").starts_with("public, max-age=30"));
        assert!(cache_control("/v1/charts").starts_with("public, max-age=900"));
        assert!(cache_control("/v1/charts/ID").starts_with("public, max-age=900"));
        assert!(cache_control("/v1/browse").starts_with("public, max-age=900"));
        assert!(cache_control("/v1/genre/x").starts_with("public, max-age=900"));
        assert!(cache_control("/v1/album/x").starts_with("public, max-age=300"));
        assert!(cache_control("/v1/track/x/credits").starts_with("public, max-age=1800"));
        assert_eq!(cache_control("/v1/track/x/recommendations").starts_with("public, max-age=300"), true);
        assert_eq!(cache_control("/v1/track/x/related").starts_with("public, max-age=300"), true);
    }

    #[test]
    fn extract_user_auth_accepts_headers_cookies_and_query_params() {
        let token = format!("BQ{}", "x".repeat(60));

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        assert_eq!(
            extract_user_auth(&headers, None, None).unwrap(),
            UserAuth::AccessToken(token)
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-sp-dc",
            HeaderValue::from_static("AQB_cookie_from_x_header"),
        );
        assert_eq!(
            extract_user_auth(&headers, None, None).unwrap(),
            UserAuth::SpDc("AQB_cookie_from_x_header".into())
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("sp_t=abc; sp_dc=AQB_from_cookie; other=1"),
        );
        assert_eq!(
            extract_user_auth(&headers, None, None).unwrap(),
            UserAuth::SpDc("AQB_from_cookie".into())
        );

        let empty = HeaderMap::new();
        assert_eq!(
            extract_user_auth(&empty, None, Some("AQB_from_query")).unwrap(),
            UserAuth::SpDc("AQB_from_query".into())
        );
    }

    #[test]
    fn cache_hits_expires_and_stays_bounded() {
        let cache = Cache::default();

        cache.insert("hit".into(), Bytes::from_static(b"ok"), Duration::from_secs(60));
        assert_eq!(cache.get("hit").as_deref(), Some(b"ok".as_slice()));

        cache.insert("gone".into(), Bytes::from_static(b"x"), Duration::ZERO);
        assert!(cache.get("gone").is_none());

        for index in 0..MAX_CACHE_ENTRIES + 5 {
            cache.insert(
                format!("k{index}"),
                Bytes::from_static(b"x"),
                Duration::from_secs(60),
            );
        }

        assert!(cache.entry_count() <= MAX_CACHE_ENTRIES);
    }
}
