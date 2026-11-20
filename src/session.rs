//! Anonymous Spotify web-player session.
//!
//! The session discovers the current web-player build, generates the TOTP that
//! Spotify requires for its token endpoint, and executes persisted GraphQL
//! queries against the same backend the web player uses.
//!
//! Session state is shared and synchronized internally, so the same `Session`
//! can serve concurrent requests: every query borrows `&self`, the token is
//! guarded by an async mutex, and the discovered bundles/hashes by an async
//! read-write lock.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::models::{
    BrowseCategory, Charts, GenrePage, SearchResults, UserPlaylists, UserProfile, UserTop,
    UserTracks, market_of, parse_browse_categories, parse_chart_entries, parse_charts,
    parse_genre_page, parse_search, parse_user_playlists, parse_user_profile, parse_user_top,
    parse_user_tracks, relabel,
};
use crate::protocol::bundles::{
    AppServerConfig, chunk_urls, extract_js_links, extract_query_hashes, extract_script_base64,
    extract_totp_secret, parse_app_server_config,
};
use crate::protocol::routes;
use crate::protocol::totp::{decode_secret, generate_totp};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const SSR_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

const TEXT_ACCEPT: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

const TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;

/// Collection paging. Spotify caps each page, so a collection larger than the
/// page size would otherwise be truncated silently. `*_MAX` keeps one request
/// from fanning out without bound.
const ALBUM_PAGE: usize = 50;
const ALBUM_MAX: usize = 500;
const PLAYLIST_PAGE: usize = 100;
const PLAYLIST_MAX: usize = 1000;

/// Country hub that carries Spotify's worldwide charts.
pub const GLOBAL_HUB: &str = "US";

/// Web-pack chunks that carry the persisted hashes of the route operations this
/// client calls. Hashes are not in the main bundle, so each route chunk has to
/// be fetched separately.
const OPERATION_CHUNKS: &[&str] = &[
    "xpui-routes-search",
    "xpui-routes-album",
    "xpui-routes-artist",
    "xpui-routes-playlist",
    "xpui-routes-track",
    "xpui-routes-country-hub",
    "xpui-routes-profile",
    "xpui-routes-collection",
];

const MAX_USER_TOKEN_CACHE: usize = 128;

/// User-supplied credentials for `/me` operations.
///
/// A client can pass either a ready-made Spotify bearer `access_token` (`BQ…`)
/// or an `sp_dc` browser cookie, which the session exchanges for a user access
/// token through Spotify's TOTP token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserAuth {
    AccessToken(String),
    SpDc(String),
}

impl UserAuth {
    /// Parse a raw credential string (`Bearer <token>`, `sp_dc=<cookie>`, full
    /// cookie header, raw `BQ…` access token, or raw `sp_dc` value).
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("bearer") {
            return Err(Error::Protocol(
                "missing Spotify token or sp_dc cookie".into(),
            ));
        }

        let stripped = trimmed
            .strip_prefix("Bearer ")
            .or_else(|| trimmed.strip_prefix("bearer "))
            .map(str::trim)
            .unwrap_or(trimmed);

        if stripped.is_empty() {
            return Err(Error::Protocol(
                "missing Spotify token or sp_dc cookie".into(),
            ));
        }

        if let Some(sp_dc) = extract_sp_dc_pair(stripped) {
            return Self::from_sp_dc(sp_dc);
        }

        // Spotify access tokens (Web Player and Web API OAuth) start with `BQ`.
        if stripped.starts_with("BQ") && stripped.len() >= 40 {
            Self::from_access_token(stripped)
        } else {
            Self::from_sp_dc(stripped)
        }
    }

    /// Parse an explicit cookie header or raw `sp_dc` value.
    pub fn from_cookie(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if let Some(sp_dc) = extract_sp_dc_pair(trimmed) {
            return Self::from_sp_dc(sp_dc);
        }
        Self::from_sp_dc(trimmed)
    }

    /// Construct a validated bearer `access_token`.
    pub fn from_access_token(token: &str) -> Result<Self> {
        let token = token.trim();
        if token.is_empty()
            || token.len() > 2048
            || !token
                .chars()
                .all(|c| c.is_ascii_graphic() && c != ';' && c != '"')
        {
            return Err(Error::Protocol("invalid Spotify access token".into()));
        }
        Ok(Self::AccessToken(token.to_string()))
    }

    /// Construct a validated `sp_dc` cookie value.
    pub fn from_sp_dc(sp_dc: &str) -> Result<Self> {
        let sp_dc = sp_dc.trim().trim_matches('"');
        if sp_dc.is_empty()
            || sp_dc.len() > 1024
            || !sp_dc
                .chars()
                .all(|c| c.is_ascii_graphic() && c != ';' && c != '"')
        {
            return Err(Error::Protocol("invalid sp_dc cookie".into()));
        }
        Ok(Self::SpDc(sp_dc.to_string()))
    }
}

fn extract_sp_dc_pair(raw: &str) -> Option<&str> {
    raw.split(';')
        .find_map(|part| part.trim().strip_prefix("sp_dc="))
        .map(|val| val.trim().trim_matches('"'))
        .filter(|val| !val.is_empty())
}

#[derive(Debug, Clone)]
pub struct AccessToken {
    pub client_id: String,
    pub access_token: String,
    pub client_token: String,
    pub expires_at_ms: i64,
    pub is_anonymous: bool,
}

impl AccessToken {
    /// Milliseconds until the token expires.
    pub fn expires_in_ms(&self) -> i64 {
        self.expires_at_ms - now_ms()
    }

    pub fn is_expired(&self) -> bool {
        self.expires_in_ms() <= TOKEN_REFRESH_BUFFER_MS
    }
}

/// Everything discovered from a web-player deployment. Replaced wholesale on a
/// refresh, so readers never observe a half-updated build.
struct Discovery {
    app: AppServerConfig,
    hashes: HashMap<String, String>,
    secret: (String, u32),
}

pub struct Session {
    http: reqwest::Client,
    /// Cookie-less client used for user-scoped (`/me`) requests so a caller's
    /// `sp_dc` cookie never leaks into the shared anonymous cookie jar.
    user_http: reqwest::Client,
    language: String,
    discovery: tokio::sync::RwLock<Option<Discovery>>,
    token: tokio::sync::Mutex<Option<AccessToken>>,
    /// Short-lived cache of `SHA-256(sp_dc)` -> user `AccessToken` so consecutive
    /// `/v1/me/*` calls from the same client do not re-hit `open.spotify.com/api/token`.
    user_tokens: tokio::sync::Mutex<HashMap<[u8; 32], AccessToken>>,
    /// Coalesces concurrent rediscovery, so a rotated deployment triggers one
    /// fetch instead of one per in-flight request.
    refresh_lock: tokio::sync::Mutex<()>,
    /// Bumped on every successful discovery; lets a stale caller tell whether
    /// another request already refreshed.
    generation: AtomicU64,
}

impl Session {
    /// Create a session with the default `en` language.
    pub async fn new() -> Result<Self> {
        Self::with_language("en").await
    }

    /// Create a session with a validated `Accept-Language` value.
    pub async fn with_language(language: &str) -> Result<Self> {
        if !is_valid_language(language) {
            return Err(Error::Protocol(format!(
                "invalid language {language:?}; use a value such as en, en-US or fr-CA"
            )));
        }

        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .timeout(Duration::from_secs(30))
            .build()?;

        let user_http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            user_http,
            language: language.to_string(),
            discovery: tokio::sync::RwLock::new(None),
            token: tokio::sync::Mutex::new(None),
            user_tokens: tokio::sync::Mutex::new(HashMap::new()),
            refresh_lock: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
        })
    }

    /// Snapshot of the current token, when one has been obtained.
    pub async fn token(&self) -> Option<AccessToken> {
        self.token.lock().await.clone()
    }

    /// Download the web-player bundles and extract the persisted query hashes
    /// and the TOTP secret. Safe to call repeatedly; subsequent calls are cheap.
    pub async fn warm(&self) -> Result<()> {
        self.discover(false).await
    }

    /// Number of discovered operation hashes; `0` before the first warm-up.
    pub async fn hash_count(&self) -> usize {
        self.discovery
            .read()
            .await
            .as_ref()
            .map(|discovery| discovery.hashes.len())
            .unwrap_or(0)
    }

    /// Operation names discovered in the current build. Used by probes and
    /// documentation tooling.
    pub async fn operation_names(&self) -> Vec<String> {
        self.discovery
            .read()
            .await
            .as_ref()
            .map(|discovery| discovery.hashes.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Whether a usable build has been discovered.
    pub async fn is_warm(&self) -> bool {
        self.discovery
            .read()
            .await
            .as_ref()
            .is_some_and(|discovery| discovery.hashes.contains_key("searchDesktop"))
    }

    /// Force a rediscovery, replacing the cached build. Cheap enough to run on a
    /// timer so a Spotify deployment does not turn every request into a `502`
    /// until the process restarts.
    pub async fn refresh(&self) -> Result<()> {
        let _guard = self.refresh_lock.lock().await;
        self.discover(true).await
    }

    /// Refresh only if nobody has refreshed since `seen` was read.
    async fn refresh_from(&self, seen: u64) -> Result<()> {
        let _guard = self.refresh_lock.lock().await;

        if self.generation.load(Ordering::Acquire) != seen {
            return Ok(());
        }

        self.discover(true).await
    }

    async fn discover(&self, force: bool) -> Result<()> {
        if !force {
            let ready = {
                let guard = self.discovery.read().await;
                guard
                    .as_ref()
                    .is_some_and(|discovery| discovery.hashes.contains_key("searchDesktop"))
            };

            if ready {
                return Ok(());
            }
        }

        let html = self.fetch_text(routes::OPEN).await?;
        let app = parse_app_server_config(&html)?;

        let main_urls: Vec<String> = extract_js_links(&html)
            .into_iter()
            .filter(|url| url.contains("/web-player."))
            .collect();

        if main_urls.is_empty() {
            return Err(Error::Protocol(
                "Spotify HTML does not reference a web-player bundle".into(),
            ));
        }

        let main_sources = fetch_many(&self.http, &self.language, main_urls).await;
        if main_sources.is_empty() {
            return Err(Error::Protocol(
                "could not download a web-player bundle".into(),
            ));
        }

        let mut hashes = HashMap::new();
        let mut secret: Option<(String, u32)> = None;

        for (_, source) in &main_sources {
            hashes.extend(extract_query_hashes(source));

            if let Some(candidate) = extract_totp_secret(source) {
                let is_newer = secret
                    .as_ref()
                    .map(|(_, version)| candidate.1 > *version)
                    .unwrap_or(true);

                if is_newer {
                    secret = Some(candidate);
                }
            }
        }

        let mut seen = HashSet::new();
        let mut chunk_targets = Vec::new();

        for (_, source) in &main_sources {
            for needle in OPERATION_CHUNKS {
                for url in chunk_urls(source, needle) {
                    if seen.insert(url.clone()) {
                        chunk_targets.push(url);
                    }
                }
            }
        }

        for (_, chunk) in fetch_many(&self.http, &self.language, chunk_targets).await {
            hashes.extend(extract_query_hashes(&chunk));
        }

        let secret = secret.ok_or_else(|| {
            Error::Protocol("could not find a TOTP secret in the web-player bundle".into())
        })?;

        *self.discovery.write().await = Some(Discovery {
            app,
            hashes,
            secret,
        });

        self.generation.fetch_add(1, Ordering::Release);

        Ok(())
    }

    /// Obtain a fresh anonymous token, reusing a valid one when possible. A
    /// single refresh runs at a time; concurrent callers wait then reuse it.
    pub async fn authorize(&self) -> Result<()> {
        if self.token_is_fresh().await {
            return Ok(());
        }

        self.warm().await?;

        let mut slot = self.token.lock().await;

        if let Some(token) = slot.as_ref()
            && !token.is_expired()
        {
            return Ok(());
        }

        let (secret, version, client_version) = {
            let discovery = self.discovery.read().await;
            let discovery = discovery
                .as_ref()
                .ok_or_else(|| Error::Protocol("session is not warmed up".into()))?;

            (
                discovery.secret.0.clone(),
                discovery.secret.1,
                discovery.app.client_version.clone(),
            )
        };

        let totp = generate_totp(&decode_secret(&secret), now_secs());
        let totp_version = version.to_string();

        let response = self
            .http
            .get(routes::TOKEN)
            .query(&[
                ("reason", "init"),
                ("productType", "web-player"),
                ("totp", totp.as_str()),
                ("totpServer", totp.as_str()),
                ("totpVer", totp_version.as_str()),
            ])
            .header("accept", "application/json")
            .send()
            .await?;

        let body: Value = ensure_ok(response).await?.json().await?;

        let client_id = string_field(&body, "clientId")?;
        let access_token = string_field(&body, "accessToken")?;
        let expires_at_ms = body
            .get("accessTokenExpirationTimestampMs")
            .and_then(Value::as_i64)
            .ok_or_else(|| Error::Protocol("token response has no expiry".into()))?;
        let is_anonymous = body
            .get("isAnonymous")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let client_token = self.fetch_client_token(&client_id, &client_version).await?;

        *slot = Some(AccessToken {
            client_id,
            access_token,
            client_token,
            expires_at_ms,
            is_anonymous,
        });

        Ok(())
    }

    async fn token_is_fresh(&self) -> bool {
        self.token
            .lock()
            .await
            .as_ref()
            .is_some_and(|token| !token.is_expired())
    }

    /// Execute any persisted GraphQL operation and return the `data` object.
    ///
    /// When the failure looks like a rotated deployment (stale persisted hash
    /// or token), the build is rediscovered once and the call retried.
    pub async fn query(&self, operation: &str, variables: Value) -> Result<Value> {
        let generation = self.generation.load(Ordering::Acquire);

        match self.query_once(operation, variables.clone()).await {
            Err(error) if error.is_discovery_stale() => {
                if self.refresh_from(generation).await.is_err() {
                    return Err(error);
                }

                self.query_once(operation, variables).await
            }
            result => result,
        }
    }

    async fn query_once(&self, operation: &str, variables: Value) -> Result<Value> {
        self.authorize().await?;

        let (access_token, client_token, app_version, hash) = {
            let token = self.token.lock().await;
            let token = token
                .as_ref()
                .ok_or_else(|| Error::Protocol("session has no token".into()))?;

            let discovery = self.discovery.read().await;
            let discovery = discovery.as_ref();

            let hash = discovery
                .and_then(|discovery| discovery.hashes.get(operation).cloned())
                .ok_or_else(|| Error::MissingQuery(operation.to_string()))?;
            let app_version = discovery
                .map(|discovery| discovery.app.client_version.clone())
                .unwrap_or_default();

            (
                token.access_token.clone(),
                token.client_token.clone(),
                app_version,
                hash,
            )
        };

        let payload = json!({
            "variables": variables,
            "operationName": operation,
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": hash
                }
            }
        });

        let response = self
            .http
            .post(routes::PATHFINDER)
            .header("authorization", format!("Bearer {access_token}"))
            .header("client-token", client_token)
            .header("spotify-app-version", app_version)
            .header("app-platform", "WebPlayer")
            .header("origin", "https://open.spotify.com")
            .header("referer", "https://open.spotify.com/")
            .header("accept", "application/json")
            .header("accept-language", &self.language)
            .json(&payload)
            .send()
            .await?;

        let body: Value = ensure_ok(response).await?.json().await?;

        if let Some(data) = body.get("data") {
            return Ok(data.clone());
        }

        let errors = body
            .get("errors")
            .map(Value::to_string)
            .unwrap_or_else(|| body.to_string());

        Err(Error::NoData(errors))
    }

    /// Execute the first operation that has a discovered hash.
    ///
    /// Spotify renames operations between deployments, so callers pass the
    /// candidate names in preference order.
    pub async fn query_with_fallbacks(
        &self,
        operations: &[&str],
        variables: &Value,
    ) -> Result<Value> {
        self.authorize().await?;

        let generation = self.generation.load(Ordering::Acquire);

        let Some(operation) = self.resolve(operations).await else {
            // Every candidate hash is missing: a deployment shipped new names.
            if self.refresh_from(generation).await.is_err() {
                return Err(Error::MissingQuery(operations.join("|")));
            }

            return match self.resolve(operations).await {
                Some(operation) => self.query(operation, variables.clone()).await,
                None => Err(Error::MissingQuery(operations.join("|"))),
            };
        };

        self.query(operation, variables.clone()).await
    }

    /// First candidate operation that has a discovered hash.
    async fn resolve<'a>(&self, operations: &[&'a str]) -> Option<&'a str> {
        let discovery = self.discovery.read().await;

        operations.iter().copied().find(|name| {
            discovery
                .as_ref()
                .is_some_and(|discovery| discovery.hashes.contains_key(*name))
        })
    }

    /// Search tracks, artists, albums and playlists in one request.
    pub async fn search(&self, query: &str, limit: usize) -> Result<SearchResults> {
        self.search_page(query, limit, 0).await
    }

    /// Search with pagination. Section filtering happens at the HTTP layer so
    /// the upstream request stays identical to what the web player sends.
    pub async fn search_page(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResults> {
        let query = query.trim();

        if query.is_empty() {
            return Err(Error::Protocol("search query must not be empty".into()));
        }

        if !(1..=100).contains(&limit) {
            return Err(Error::Protocol("limit must be between 1 and 100".into()));
        }

        let variables = json!({
            "searchTerm": query,
            "offset": offset,
            "limit": limit,
            "numberOfTopResults": 20,
            "includeAudiobooks": false,
            "includeArtistHasConcertsField": false,
            "includePreReleases": false,
            "includeAlbumPreReleases": false,
            "includeAuthors": false,
            "includeEpisodeContentRatingsV2": false,
            "isPrefix": null,
            "sectionFilters": ["GENERIC"]
        });

        let data = self.query("searchDesktop", variables).await?;

        Ok(parse_search(query, &data))
    }

    /// Track details.
    pub async fn track(&self, id: &str) -> Result<Value> {
        validate_id(id, "track")?;

        let variables = json!({ "uri": format!("spotify:track:{id}") });

        self.query_with_fallbacks(&["getTrack", "track"], &variables)
            .await
    }

    /// Track credits (songwriters, producers, performers).
    pub async fn track_credits(&self, id: &str) -> Result<Value> {
        validate_id(id, "track")?;

        let variables = json!({ "trackUri": format!("spotify:track:{id}") });

        self.query_with_fallbacks(
            &["queryTrackCreditsGroupedModal", "trackCreditsGroupedModal"],
            &variables,
        )
        .await
    }

    /// Related/recommended tracks seeded by track URI.
    pub async fn track_recommendations(&self, id: &str) -> Result<Value> {
        validate_id(id, "track")?;

        let variables = json!({ "uri": format!("spotify:track:{id}") });

        self.query_with_fallbacks(
            &["internalLinkRecommenderTrack", "seoRecommendedTrackPlaylistDesktop"],
            &variables,
        )
        .await
    }

    /// Instant search suggestions as the user types.
    pub async fn search_suggestions(&self, query: &str) -> Result<Value> {
        let variables = json!({ "query": query });

        self.query_with_fallbacks(&["searchSuggestions"], &variables)
            .await
    }

    /// ISRC for a track, read from Spotify's protobuf track metadata.
    ///
    /// The GraphQL `getTrack` never exposes it, but the spclient `Track`
    /// message does, as `external_id`. Requires the same bearer + client token
    /// as any other call; the official Web API is not used.
    pub async fn track_isrc(&self, id: &str) -> Result<Option<String>> {
        validate_id(id, "track")?;
        self.authorize().await?;

        let (access_token, client_token) = {
            let token = self.token.lock().await;
            let token = token
                .as_ref()
                .ok_or_else(|| Error::Protocol("session has no token".into()))?;

            (token.access_token.clone(), token.client_token.clone())
        };

        let gid = base62_gid(id)
            .ok_or_else(|| Error::Protocol(format!("track id {id:?} is not base62")))?;

        let response = self
            .http
            .get(format!("{}/{gid}", routes::TRACK_METADATA))
            .query(&[("market", "from_token")])
            .header("authorization", format!("Bearer {access_token}"))
            .header("client-token", client_token)
            .header("app-platform", "WebPlayer")
            .header("accept", "*/*")
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let bytes = ensure_ok(response).await?.bytes().await?;

        Ok(find_isrc(&bytes))
    }

    /// Album metadata plus its track list, as returned by pathfinder.
    ///
    /// Albums with more tracks than one upstream page are fetched in pages and
    /// merged, so a 300-track album is not silently cut at 50.
    pub async fn album(&self, id: &str) -> Result<Value> {
        validate_id(id, "album")?;

        self.collection(
            &["getAlbum", "album"],
            json!({ "uri": format!("spotify:album:{id}"), "locale": "" }),
            "/albumUnion/tracksV2/items",
            "/albumUnion/tracksV2/totalCount",
            ALBUM_PAGE,
            ALBUM_MAX,
        )
        .await
    }

    /// Artist profile with discography and top tracks, as returned by pathfinder.
    pub async fn artist(&self, id: &str) -> Result<Value> {
        validate_id(id, "artist")?;

        let variables = json!({
            "uri": format!("spotify:artist:{id}"),
            "locale": "",
            "includePrerelease": true
        });

        self.query_with_fallbacks(
            &["queryArtistOverview", "artistOverview", "artist"],
            &variables,
        )
        .await
    }

    /// Artist discography releases (all, albums, singles, or compilations).
    pub async fn artist_discography(
        &self,
        id: &str,
        category: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        validate_id(id, "artist")?;

        let op = match category {
            Some("album") | Some("albums") => "queryArtistDiscographyAlbums",
            Some("single") | Some("singles") => "queryArtistDiscographySingles",
            Some("compilation") | Some("compilations") => "queryArtistDiscographyCompilations",
            _ => "queryArtistDiscographyAll",
        };

        let variables = json!({
            "uri": format!("spotify:artist:{id}"),
            "offset": offset,
            "limit": limit.clamp(1, 100),
        });

        self.query_with_fallbacks(&[op], &variables).await
    }

    /// Playlist metadata plus its tracks, as returned by pathfinder.
    ///
    /// Playlists longer than one upstream page are fetched in pages and merged,
    /// up to `PLAYLIST_MAX`, so the list is not silently cut at 100.
    pub async fn playlist(&self, id: &str) -> Result<Value> {
        validate_id(id, "playlist")?;

        self.collection(
            &["fetchPlaylist", "playlistV2"],
            json!({
                "uri": format!("spotify:playlist:{id}"),
                "enableWatchFeedEntrypoint": false
            }),
            "/playlistV2/content/items",
            "/playlistV2/content/totalCount",
            PLAYLIST_PAGE,
            PLAYLIST_MAX,
        )
        .await
    }

    /// Country charts: the `CHARTS` shelf plus trending songs, popular albums
    /// and popular artists for one market.
    ///
    /// Spotify has no public chart operation; the chart shelf is a set of
    /// editorial playlists (`format: "chart"`), so `top` fetches one of them by
    /// rank to expose per-track `plays`/`rank`/`previous_pos`.
    ///
    /// `top` is `daily` or `alltime`. The chart's frequency cannot be trusted:
    /// for a global market the hub lists the *local* charts, which are shown
    /// using the global artwork names (`Top 50 - <market>`, `Top Songs -
    /// <market>`). The artwork names are stable, so they drive the choice.
    pub async fn charts(
        &self,
        country: &str,
        top: Option<&str>,
        limit: usize,
    ) -> Result<Charts> {
        let country = country.trim().to_uppercase();

        if country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(Error::Protocol(
                "country must be a two-letter ISO code".into(),
            ));
        }

        let data = self
            .query_with_fallbacks(
                &["countryHubsPage", "countryHub"],
                &json!({ "countryCode": country }),
            )
            .await?;

        let mut charts = parse_charts(&country, &data);

        // The hub lists both the market's charts and the global ones; pick the
        // right pair instead of trusting the shelf order.
        let wanted = if country == GLOBAL_HUB { "global" } else { "local" };
        if charts.charts.iter().any(|chart| market_of(chart) == Some("global"))
            && charts.charts.iter().any(|chart| market_of(chart) == Some("local"))
        {
            charts.charts.retain(|chart| market_of(chart) == Some(wanted));
        }

        let daily = top != Some("alltime");

        let Some(mut chart) = charts
            .charts
            .iter()
            .position(|chart| {
                let name = chart.name.to_lowercase();
                // "Top 50 - …" is the daily chart, "Top Songs - …" the
                // all-time one.
                if daily {
                    name.starts_with("top 50")
                } else {
                    name.starts_with("top songs")
                }
            })
            .map(|index| charts.charts.swap_remove(index))
        else {
            return Ok(charts);
        };

        relabel(&mut chart, if country == GLOBAL_HUB { "Global" } else { "Indonesia" });

        let payload = self.playlist(&chart.id).await.unwrap_or(Value::Null);

        // A chart playlist that cannot be read still leaves the chart list
        // intact; only its rows are missing.
        if !payload.is_null() {
            parse_chart_entries(&mut chart, &payload);

            if limit > 0 {
                chart.entries.truncate(limit);
            }

            charts.charts.insert(0, chart);
        }

        Ok(charts)
    }

    /// Browse / explore categories from Spotify's search grid.
    pub async fn browse_categories(&self) -> Result<Vec<BrowseCategory>> {
        let response = self
            .http
            .get(routes::SEARCH_PAGE)
            .header("user-agent", SSR_USER_AGENT)
            .header("accept-language", &self.language)
            .send()
            .await?;
        let html = ensure_ok(response).await?.text().await?;
        let state = extract_script_base64(&html, "initialState")?;
        Ok(parse_browse_categories(&state))
    }

    /// Genre page content by category/genre ID (e.g. `0JQ5DAqbMKFziKOShCi009`).
    pub async fn genre(&self, id: &str) -> Result<GenrePage> {
        validate_id(id, "genre")?;
        let url = format!("{}{id}", routes::GENRE_PAGE);
        let response = self
            .http
            .get(&url)
            .header("user-agent", SSR_USER_AGENT)
            .header("accept-language", &self.language)
            .send()
            .await?;
        let html = ensure_ok(response).await?.text().await?;
        let state = extract_script_base64(&html, "initialState")?;
        parse_genre_page(id, &state)
            .ok_or_else(|| Error::Protocol(format!("genre {id} not found")))
    }

    /// Resolve a caller's `UserAuth` into a bearer `access_token`.
    ///
    /// When the caller supplies an `sp_dc` cookie, it is exchanged against
    /// `open.spotify.com/api/token` using the discovered TOTP secret and cached
    /// in memory (keyed by `SHA-256(sp_dc)`) until it expires.
    pub async fn resolve_user_token(&self, auth: &UserAuth) -> Result<String> {
        match auth {
            UserAuth::AccessToken(token) => Ok(token.clone()),
            UserAuth::SpDc(sp_dc) => {
                let token = self.exchange_sp_dc(sp_dc).await?;
                Ok(token.access_token)
            }
        }
    }

    /// Exchange a user's `sp_dc` cookie for a non-anonymous web-player `AccessToken`.
    pub async fn exchange_sp_dc(&self, sp_dc: &str) -> Result<AccessToken> {
        let digest = sha256_digest(sp_dc);

        if let Some(cached) = self.user_tokens.lock().await.get(&digest).cloned()
            && !cached.is_expired()
        {
            return Ok(cached);
        }

        self.warm().await?;
        let _ = self.authorize().await;

        let (secret, version, client_version) = {
            let discovery = self.discovery.read().await;
            let discovery = discovery
                .as_ref()
                .ok_or_else(|| Error::Protocol("session is not warmed up".into()))?;

            (
                discovery.secret.0.clone(),
                discovery.secret.1,
                discovery.app.client_version.clone(),
            )
        };

        let totp = generate_totp(&decode_secret(&secret), now_secs());
        let totp_version = version.to_string();

        let response = self
            .user_http
            .get(routes::TOKEN)
            .query(&[
                ("reason", "init"),
                ("productType", "web-player"),
                ("totp", totp.as_str()),
                ("totpServer", totp.as_str()),
                ("totpVer", totp_version.as_str()),
            ])
            .header("cookie", format!("sp_dc={sp_dc}"))
            .header("referer", routes::OPEN)
            .header("accept", "application/json")
            .send()
            .await?;

        let body: Value = ensure_ok(response).await?.json().await?;

        let is_anonymous = body
            .get("isAnonymous")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if is_anonymous {
            return Err(Error::Status {
                status: 401,
                body: "sp_dc cookie is invalid or expired".into(),
            });
        }

        let client_id = string_field(&body, "clientId")?;
        let access_token = string_field(&body, "accessToken")?;
        let expires_at_ms = body
            .get("accessTokenExpirationTimestampMs")
            .and_then(Value::as_i64)
            .ok_or_else(|| Error::Protocol("token response has no expiry".into()))?;

        let client_token = match self.token.lock().await.as_ref() {
            Some(anon) if anon.client_id == client_id && !anon.client_token.is_empty() => {
                anon.client_token.clone()
            }
            _ => self
                .fetch_client_token(&client_id, &client_version)
                .await
                .unwrap_or_default(),
        };

        let token = AccessToken {
            client_id,
            access_token,
            client_token,
            expires_at_ms,
            is_anonymous: false,
        };

        let mut cache = self.user_tokens.lock().await;
        if cache.len() >= MAX_USER_TOKEN_CACHE {
            cache.retain(|_, entry| !entry.is_expired());
            if cache.len() >= MAX_USER_TOKEN_CACHE {
                cache.clear();
            }
        }
        cache.insert(digest, token.clone());

        Ok(token)
    }

    /// Execute a persisted GraphQL operation using a caller-supplied bearer
    /// `access_token` instead of the shared anonymous token.
    pub async fn query_as(
        &self,
        access_token: &str,
        operation: &str,
        variables: Value,
    ) -> Result<Value> {
        self.authorize().await?;

        let (client_token, app_version, hash) = {
            let token = self.token.lock().await;
            let client_token = token
                .as_ref()
                .map(|t| t.client_token.clone())
                .unwrap_or_default();

            let discovery = self.discovery.read().await;
            let discovery = discovery.as_ref();

            let hash = discovery
                .and_then(|d| d.hashes.get(operation).cloned())
                .ok_or_else(|| Error::MissingQuery(operation.to_string()))?;
            let app_version = discovery
                .map(|d| d.app.client_version.clone())
                .unwrap_or_default();

            (client_token, app_version, hash)
        };

        let payload = json!({
            "variables": variables,
            "operationName": operation,
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": hash
                }
            }
        });

        let response = self
            .user_http
            .post(routes::PATHFINDER)
            .header("authorization", format!("Bearer {access_token}"))
            .header("client-token", client_token)
            .header("spotify-app-version", app_version)
            .header("app-platform", "WebPlayer")
            .header("origin", "https://open.spotify.com")
            .header("referer", "https://open.spotify.com/")
            .header("accept", "application/json")
            .header("accept-language", &self.language)
            .json(&payload)
            .send()
            .await?;

        let body: Value = ensure_ok(response).await?.json().await?;

        if let Some(data) = body.get("data") {
            return Ok(data.clone());
        }

        let errors = body
            .get("errors")
            .map(Value::to_string)
            .unwrap_or_else(|| body.to_string());

        Err(Error::NoData(errors))
    }

    /// Execute the first discovered operation candidate using a caller-supplied
    /// bearer `access_token`.
    pub async fn query_with_fallbacks_as(
        &self,
        access_token: &str,
        operations: &[&str],
        variables: &Value,
    ) -> Result<Value> {
        self.authorize().await?;

        let generation = self.generation.load(Ordering::Acquire);

        let Some(operation) = self.resolve(operations).await else {
            if self.refresh_from(generation).await.is_err() {
                return Err(Error::MissingQuery(operations.join("|")));
            }

            return match self.resolve(operations).await {
                Some(operation) => self.query_as(access_token, operation, variables.clone()).await,
                None => Err(Error::MissingQuery(operations.join("|"))),
            };
        };

        self.query_as(access_token, operation, variables.clone()).await
    }

    /// Call `https://api.spotify.com/v1/me{subpath}` with the caller's bearer token.
    ///
    /// Used both as a fallback when a pathfinder operation hash is unavailable
    /// and when the caller passes a standard Spotify Web API OAuth token.
    async fn web_api_me(
        &self,
        access_token: &str,
        subpath: &str,
        query: &[(&str, String)],
    ) -> Result<Value> {
        let _ = self.authorize().await;

        let (client_token, app_version) = {
            let token = self.token.lock().await;
            let client_token = token.as_ref().map(|t| t.client_token.clone());
            let discovery = self.discovery.read().await;
            let app_version = discovery.as_ref().map(|d| d.app.client_version.clone());
            (client_token, app_version)
        };

        let mut request = self
            .user_http
            .get(format!("{}{subpath}", routes::WEB_API_ME))
            .query(query)
            .header("authorization", format!("Bearer {access_token}"))
            .header("app-platform", "WebPlayer")
            .header("origin", "https://open.spotify.com")
            .header("referer", "https://open.spotify.com/")
            .header("accept", "application/json")
            .header("accept-language", &self.language);

        if let Some(ct) = client_token.filter(|ct| !ct.is_empty()) {
            request = request.header("client-token", ct);
        }
        if let Some(ver) = app_version.filter(|ver| !ver.is_empty()) {
            request = request.header("spotify-app-version", ver);
        }

        let response = request.send().await?;
        Ok(ensure_ok(response).await?.json().await?)
    }

    /// Current authenticated user's profile and account details (`GET /v1/me`).
    pub async fn me(&self, auth: &UserAuth) -> Result<UserProfile> {
        let access_token = self.resolve_user_token(auth).await?;
        let empty = json!({});

        let (profile_res, account_res) = tokio::join!(
            self.query_with_fallbacks_as(&access_token, &["profileAttributes"], &empty),
            self.query_with_fallbacks_as(&access_token, &["accountAttributes"], &empty),
        );

        if let Ok(profile_data) = &profile_res
            && let Some(mut profile) = parse_user_profile(profile_data, account_res.ok().as_ref())
        {
            if profile.country.is_none() || profile.product.is_none() {
                if let Ok(web_data) = self.web_api_me(&access_token, "", &[]).await
                    && let Some(web_profile) = parse_user_profile(&web_data, None)
                {
                    if profile.country.is_none() {
                        profile.country = web_profile.country;
                    }
                    if profile.product.is_none() {
                        profile.product = web_profile.product;
                    }
                    if profile.explicit_filter.is_none() {
                        profile.explicit_filter = web_profile.explicit_filter;
                    }
                    if profile.image.is_none() {
                        profile.image = web_profile.image;
                    }
                }
            }
            return Ok(profile);
        }

        let web_data = self.web_api_me(&access_token, "", &[]).await?;
        parse_user_profile(&web_data, None)
            .ok_or_else(|| Error::NoData("could not parse user profile".into()))
    }

    /// Current authenticated user's playlists (`GET /v1/me/playlists`).
    pub async fn me_playlists(
        &self,
        auth: &UserAuth,
        limit: usize,
        offset: usize,
    ) -> Result<UserPlaylists> {
        let access_token = self.resolve_user_token(auth).await?;
        let limit = limit.clamp(1, 50);

        let variables = json!({
            "filters": ["Playlists"],
            "order": null,
            "textFilter": "",
            "features": ["LIKED_SONGS", "YOUR_EPISODES_V2"],
            "limit": limit,
            "offset": offset,
            "flatten": false,
            "expandedFolders": [],
            "folderUri": null,
            "includeFoldersWhenFlattening": true
        });

        if let Ok(data) = self
            .query_with_fallbacks_as(&access_token, &["libraryV3"], &variables)
            .await
            && data.pointer("/me/libraryV3").is_some()
        {
            return Ok(parse_user_playlists(&data, limit, offset));
        }

        let data = self
            .web_api_me(
                &access_token,
                "/playlists",
                &[("limit", limit.to_string()), ("offset", offset.to_string())],
            )
            .await?;

        Ok(parse_user_playlists(&data, limit, offset))
    }

    /// Current authenticated user's Liked Songs (`GET /v1/me/tracks`).
    pub async fn me_tracks(
        &self,
        auth: &UserAuth,
        limit: usize,
        offset: usize,
    ) -> Result<UserTracks> {
        let access_token = self.resolve_user_token(auth).await?;
        let limit = limit.clamp(1, 50);

        let variables = json!({
            "offset": offset,
            "limit": limit,
        });

        if let Ok(data) = self
            .query_with_fallbacks_as(&access_token, &["fetchLibraryTracks"], &variables)
            .await
            && data.pointer("/me/library/tracks").is_some()
        {
            return Ok(parse_user_tracks(&data, limit, offset));
        }

        let data = self
            .web_api_me(
                &access_token,
                "/tracks",
                &[("limit", limit.to_string()), ("offset", offset.to_string())],
            )
            .await?;

        Ok(parse_user_tracks(&data, limit, offset))
    }

    /// Current authenticated user's top tracks and/or top artists (`GET /v1/me/top`).
    ///
    /// `range` accepts `short` (`short_term`), `medium` (`medium_term`), or
    /// `long` (`long_term`).
    pub async fn me_top(
        &self,
        auth: &UserAuth,
        range: Option<&str>,
        limit: usize,
        offset: usize,
        include_tracks: bool,
        include_artists: bool,
    ) -> Result<UserTop> {
        let (label, gql_range) = match range
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            None | Some("medium") | Some("medium_term") => ("medium_term", "MEDIUM_TERM"),
            Some("short") | Some("short_term") => ("short_term", "SHORT_TERM"),
            Some("long") | Some("long_term") => ("long_term", "LONG_TERM"),
            Some(_) => {
                return Err(Error::Protocol(
                    "range must be one of: short, medium, long".into(),
                ));
            }
        };

        let access_token = self.resolve_user_token(auth).await?;
        let limit = limit.clamp(1, 50);

        let variables = json!({
            "includeTopArtists": include_artists,
            "topArtistsInput": {
                "offset": offset,
                "limit": limit,
                "sortBy": "AFFINITY",
                "timeRange": gql_range
            },
            "includeTopTracks": include_tracks,
            "topTracksInput": {
                "offset": offset,
                "limit": limit,
                "sortBy": "AFFINITY",
                "timeRange": gql_range
            }
        });

        if let Ok(data) = self
            .query_with_fallbacks_as(&access_token, &["userTopContent"], &variables)
            .await
        {
            let mut top = parse_user_top(&data, label);
            if !include_tracks {
                top.tracks.clear();
            }
            if !include_artists {
                top.artists.clear();
            }
            if !top.tracks.is_empty() || !top.artists.is_empty() {
                return Ok(top);
            }
        }

        let query = [
            ("time_range", label.to_string()),
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
        ];

        let (tracks_res, artists_res) = tokio::join!(
            async {
                if include_tracks {
                    Some(self.web_api_me(&access_token, "/top/tracks", &query).await)
                } else {
                    None
                }
            },
            async {
                if include_artists {
                    Some(self.web_api_me(&access_token, "/top/artists", &query).await)
                } else {
                    None
                }
            }
        );

        let mut combined = json!({});
        if let Some(res) = tracks_res {
            combined["tracks"] = res?;
        }
        if let Some(res) = artists_res {
            combined["artists"] = res?;
        }

        Ok(parse_user_top(&combined, label))
    }

    /// Fetch every page of a paginated collection and merge the pages into the
    /// first payload, which is the shape the API already returns.
    async fn collection(
        &self,
        operations: &[&str],
        mut variables: Value,
        items_path: &str,
        total_path: &str,
        page: usize,
        cap: usize,
    ) -> Result<Value> {
        variables["offset"] = json!(0);
        variables["limit"] = json!(page);

        let mut payload = self.query_with_fallbacks(operations, &variables).await?;

        let total = payload
            .pointer(total_path)
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let mut collected = items_len(&payload, items_path);

        while collected < total && collected < cap {
            variables["offset"] = json!(collected);

            let next = self.query_with_fallbacks(operations, &variables).await?;

            match merge_page(&mut payload, &next, items_path) {
                Some(added) => collected += added,
                // Empty page, or `offset` was ignored and the page repeats:
                // stop rather than loop or duplicate the list.
                None => break,
            }
        }

        Ok(payload)
    }

    async fn fetch_client_token(&self, client_id: &str, client_version: &str) -> Result<String> {
        let payload = json!({
            "client_data": {
                "client_version": client_version,
                "client_id": client_id,
                "js_sdk_data": {
                    "device_brand": "unknown",
                    "device_model": "unknown",
                    "os": "windows",
                    "os_version": "NT 10.0",
                    "device_id": device_id(),
                    "device_type": "computer"
                }
            }
        });

        let response = self
            .http
            .post(routes::CLIENT_TOKEN)
            .header("accept", "application/json")
            .json(&payload)
            .send()
            .await?;

        let body: Value = ensure_ok(response).await?.json().await?;

        body.pointer("/granted_token/token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Error::Protocol("client token response has no granted_token".into()))
    }

    async fn fetch_text(&self, url: &str) -> Result<String> {
        let response = self
            .http
            .get(url)
            .header("accept", TEXT_ACCEPT)
            .header("accept-language", &self.language)
            .send()
            .await?;

        Ok(ensure_ok(response).await?.text().await?)
    }
}

/// Fetch many URLs concurrently, dropping the ones that fail. Order is not
/// preserved; the callers only aggregate hashes.
async fn fetch_many(
    http: &reqwest::Client,
    language: &str,
    urls: Vec<String>,
) -> Vec<(String, String)> {
    let mut tasks = tokio::task::JoinSet::new();

    for url in urls {
        let http = http.clone();
        let language = language.to_string();

        tasks.spawn(async move {
            let response = http
                .get(&url)
                .header("accept", TEXT_ACCEPT)
                .header("accept-language", language)
                .send()
                .await
                .ok()?;

            if !response.status().is_success() {
                return None;
            }

            Some((url, response.text().await.ok()?))
        });
    }

    let mut sources = Vec::new();

    while let Some(result) = tasks.join_next().await {
        if let Ok(Some(source)) = result {
            sources.push(source);
        }
    }

    sources
}

/// Number of items in a paginated collection, at a JSON pointer such as
/// `/albumUnion/tracksV2/items`.
fn items_len(payload: &Value, items_path: &str) -> usize {
    payload
        .pointer(items_path)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

/// Append one page's items to the merged payload. Returns the number added, or
/// `None` when the page has nothing new — empty, or `offset` was ignored and it
/// repeats the previous page.
fn merge_page(payload: &mut Value, next: &Value, items_path: &str) -> Option<usize> {
    let items = next.pointer(items_path)?.as_array()?;

    if items.is_empty() {
        return None;
    }

    let target = payload.pointer_mut(items_path)?.as_array_mut()?;

    // Compare the incoming page with the items already sitting at that offset.
    // If `offset` was ignored the server replays the same page; stop instead of
    // appending duplicates.
    let start = target.len().saturating_sub(items.len());
    if target[start..] == items[..] {
        return None;
    }

    let added = items.len();
    target.extend(items.iter().cloned());

    Some(added)
}

/// Spotify catalog identifiers are base62; anything outside that is a bad
/// request rather than an upstream lookup.
pub fn validate_id(id: &str, kind: &str) -> Result<()> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(Error::Protocol(format!("invalid {kind} id {id:?}")));
    }

    Ok(())
}

/// Spotify base62 id -> 128-bit hex GID. The protobuf metadata endpoints key on
/// the hex form, not on the base62 id used everywhere else.
pub fn base62_gid(id: &str) -> Option<String> {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

    let mut value: u128 = 0;
    for byte in id.bytes() {
        let digit = ALPHABET.iter().position(|&c| c == byte)? as u128;
        value = value.checked_mul(62)?.checked_add(digit)?;
    }

    Some(format!("{value:032x}"))
}

/// Pull the first ISRC (`CCXXXYYNNNNN`) out of a protobuf blob. The Track
/// message carries it as `external_id`; scanning avoids a protobuf dependency.
pub fn find_isrc(bytes: &[u8]) -> Option<String> {
    bytes
        .windows(12)
        .find(|window| is_isrc(window))
        .map(|window| String::from_utf8_lossy(window).into_owned())
}

fn is_isrc(window: &[u8]) -> bool {
    window.len() == 12
        && window[0].is_ascii_uppercase()
        && window[1].is_ascii_uppercase()
        && window[2..5]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && window[5..12].iter().all(u8::is_ascii_digit)
}

async fn ensure_ok(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();

    Err(Error::Status { status, body })
}

fn string_field(body: &Value, name: &str) -> Result<String> {
    body.get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::Protocol(format!("token response has no {name}")))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn device_id() -> String {
    format!("{:016x}{:08x}", now_ms() as u64, std::process::id())
}

fn sha256_digest(value: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher.finalize().into()
}

fn is_valid_language(value: &str) -> bool {
    let value = value.replace('_', "-");

    if value.is_empty() || value.len() > 64 {
        return false;
    }

    value.split('-').all(|part| {
        !part.is_empty() && part.len() <= 8 && part.chars().all(|ch| ch.is_ascii_alphanumeric())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base62_ids_convert_to_hex_gids() {
        // Verified against spclient /metadata/4/track.
        assert_eq!(
            base62_gid("0SGmmZbnfbpqQXZ7BLDSpg").as_deref(),
            Some("1cfab63fb85d49a5bed19b3c6636b54e")
        );
        assert_eq!(
            base62_gid("3AJwUDP919kvQ9QcozQPxg").as_deref(),
            Some("7609b5161c5b40a2a6d14bf6047e7d6a")
        );
    }

    #[test]
    fn base62_rejects_non_base62() {
        assert!(base62_gid("not-an-id!").is_none());
    }

    #[test]
    fn pages_are_merged_and_repeats_are_rejected() {
        let path = "/albumUnion/tracksV2/items";
        let mut payload = json!({ "albumUnion": { "tracksV2": {
            "totalCount": 3,
            "items": [{ "track": { "uri": "a" } }]
        } } });
        let next = json!({ "albumUnion": { "tracksV2": {
            "items": [{ "track": { "uri": "b" } }, { "track": { "uri": "c" } }]
        } } });

        assert_eq!(items_len(&payload, path), 1);
        assert_eq!(merge_page(&mut payload, &next, path), Some(2));
        assert_eq!(items_len(&payload, path), 3);

        // The same page again (offset ignored) adds nothing.
        assert_eq!(merge_page(&mut payload, &next, path), None);

        // An empty page stops the loop instead of appending nothing forever.
        let empty = json!({ "albumUnion": { "tracksV2": { "items": [] } } });
        assert_eq!(merge_page(&mut payload, &empty, path), None);
    }

    #[test]
    fn isrc_is_scanned_out_of_protobuf_bytes() {
        let mut blob = vec![0x00, 0x0c];
        blob.extend_from_slice(b"IDB832600861");
        blob.extend_from_slice(&[0xff, 0x41]);

        assert_eq!(find_isrc(&blob).as_deref(), Some("IDB832600861"));
        // Non-uppercase prefixes and short blobs must not match.
        assert!(find_isrc(b"1B1234567890").is_none());
        assert!(find_isrc(b"AB123456789").is_none());
    }

    #[test]
    fn user_auth_distinguishes_access_tokens_and_sp_dc_cookies() {
        let token = format!("BQ{}", "a".repeat(60));
        assert_eq!(
            UserAuth::parse(&format!("Bearer {token}")).unwrap(),
            UserAuth::AccessToken(token.clone())
        );
        assert_eq!(
            UserAuth::parse(&token).unwrap(),
            UserAuth::AccessToken(token)
        );

        assert_eq!(
            UserAuth::parse("AQB_some_sp_dc_cookie_value").unwrap(),
            UserAuth::SpDc("AQB_some_sp_dc_cookie_value".into())
        );
        assert_eq!(
            UserAuth::parse("Bearer sp_dc=AQB_cookie_val").unwrap(),
            UserAuth::SpDc("AQB_cookie_val".into())
        );
        assert_eq!(
            UserAuth::from_cookie("sp_t=123; sp_dc=AQB_from_header; sp_key=xyz").unwrap(),
            UserAuth::SpDc("AQB_from_header".into())
        );

        assert!(UserAuth::parse("").is_err());
        assert!(UserAuth::parse("Bearer   ").is_err());
        assert!(UserAuth::from_sp_dc("bad cookie with spaces").is_err());
    }
}
