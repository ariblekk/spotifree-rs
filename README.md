# spotifree-rs

Unofficial Spotify client in Rust. No API key, developer account, or premium —
it talks to the same backend the Spotify web player uses.

Independent implementation of the observed protocol, inspired by
[spotipyFree](https://github.com/TzurSoffer/spotipyFree) (MIT) /
[SpotAPI](https://github.com/Aran404/SpotAPI) (GPL-3.0), not a port of their code.

## How it works

1. Read the `appServerConfig` payload from `open.spotify.com` for the current
   `clientVersion` and bundle hashes.
2. Download the `web-player` bundle (and route chunks) and extract the **TOTP
   secret** and the **persisted GraphQL query hashes**.
3. Generate the TOTP, get an anonymous token, and run persisted GraphQL
   operations against `api-partner.spotify.com/pathfinder/v2/query`.

Hashes and the TOTP secret rotate with every deployment, so everything is
discovered at runtime.

## Usage

```bash
cargo run -- search "Blinding Lights" --limit 3
cargo run -- search lofi --limit 5 --json
cargo run -- token
cargo run -- query searchDesktop '{"searchTerm":"lofi","offset":0,"limit":5}'
```

## Library

```rust
use spotifree_rs::Session;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = Session::new().await?;
    let results = session.search("lofi", 5).await?;

    for track in &results.tracks {
        println!("{} — {}", track.name, track.artists.join(", "));
    }

    Ok(())
}
```

`Session::query(operation, variables)` runs any discovered operation and returns
its raw `data`, so unmodelled operations are still reachable.

## Web UI and API

The frontend is a Vite + React SPA in `web/`, served by `src/bin/serve.rs` from
`web/dist`.

```bash
cd web && npm install && npm run build   # -> web/dist
cargo run --bin serve                    # http://localhost:8080
curl "http://localhost:8080/v1/search?q=lofi&limit=5"
```

For UI work, run the Rust server and Vite side by side (Vite proxies to
`BACKEND_ORIGIN`, default `http://127.0.0.1:8080`):

```bash
cargo run --bin serve        # terminal 1
cd web && npm run dev        # terminal 2 -> http://localhost:5173
```

With Docker: `docker compose up -d --build` (http://localhost:8080).

### Hosting the UI separately

`Dockerfile` builds the API only; `Dockerfile.full` bundles the UI too. To host
the UI elsewhere (Vercel, Cloudflare Pages):

```bash
cd web
VITE_API_BASE=https://api.example.com npm run build   # -> web/dist
```

Then set `ALLOWED_ORIGIN=https://ui.example.com` on the container. Listed origins
get CORS headers (with `Vary: Origin`) and `OPTIONS` preflights are answered.
With `VITE_API_BASE` unset the UI calls relative URLs, which is what the
all-in-one image and the Vite dev proxy both need.

| Endpoint | Returns |
| --- | --- |
| `GET /v1/search?q=<term>&limit=<1-100>&offset=<n>&type=<kinds>` | `SearchResults` JSON |
| `GET /v1/search/suggest?q=<term>` | search suggestion chips JSON |
| `GET /v1/track/{id}` | pathfinder `trackUnion` JSON |
| `GET /v1/track/{id}/credits` | contributors and sources JSON |
| `GET /v1/track/{id}/recommendations` | recommended tracks JSON |
| `GET /v1/album/{id}` | pathfinder `albumUnion` JSON (full track list) |
| `GET /v1/artist/{id}` | pathfinder `artistUnion` JSON (profile + discography) |
| `GET /v1/artist/{id}/discography` | artist releases (all, album, single, compilation) |
| `GET /v1/playlist/{id}` | pathfinder `playlistV2` JSON (items, up to 1000) |
| `GET /v1/browse` | browse / explore categories list (`id`, `title`, `color`, `image_url`) |
| `GET /v1/genre/{id}` | curated genre page with sections (`playlists`, `releases`, subgenres) |
| `GET /v1/charts/{id\|global}?top=<daily\|alltime>` | 50 ranked rows + `trending` / `popular_albums` / `popular_artists` |
| `GET /healthz` | `warm`, `hashes`, `token_valid`, `uptime_secs`, `cache_bytes`, cache counters; `503` until warm |
| `GET /`, `/docs`, `/charts`, `/donate`, `/detail?...` | SPA pages |

`type` accepts `track,artist,album,playlist` and filters sections server-side.
Detail endpoints return Spotify's object as-is. Unknown or failed upstream calls
answer `502`, bad params `400`, unknown `/v1/*` paths `404` — all as
`{"error": "..."}`.

## Configuration

The API is public: no key, no login, no user data — it only reads Spotify's
catalogue. A key would end up inside the UI bundle anyway, where anyone can read
it, so protection belongs at the edge instead (Cloudflare rate limiting / WAF) or
on a private network.

| Variable | Default | Effect |
| --- | --- | --- |
| `ALLOWED_ORIGIN` | unset = any | Comma-separated UI origins allowed to call the API cross-site (also enables CORS + preflight) |
| `BIND` | `0.0.0.0` | Listen address; use `127.0.0.1` for local-only |
| `FRONTEND_DIR` | `web/dist` | Directory holding the built frontend (unused when it is not present) |

Every response gets `nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy:
no-referrer` and a restrictive CSP; upstream error bodies are never returned.

## Performance

- In-process TTL cache (search 30s, track 2m, album/artist/playlist 5m, charts
  15m, lyrics 30m); concurrent misses for the same key share one upstream call.
- Collections are paged server-side (albums to the end, playlists up to 1000), so
  a large album is not cut off.
- Build discovered at startup and refreshed every 30 min; responses
  brotli/gzip-compressed; session serves requests concurrently.
- On a rotated deployment the session rediscovers once and retries, instead of
  serving a window of `502`s.

## Behind Cloudflare

The origin sends cacheable headers; tell Cloudflare to cache API paths
(**Caching → Cache Rules**, rules are mutually exclusive).

| Rule | Expression | Cache | Edge TTL |
| --- | --- | --- | --- |
| Static assets | `starts_with(http.request.uri.path, "/assets/")` | Eligible | Respect origin (1 year) |
| Public API | `starts_with(http.request.uri.path, "/v1/")` | Eligible | Respect origin (30s–30m) |
| Health | `http.request.uri.path eq "/healthz"` | Bypass | — |
| SPA shell | `http.request.uri.path in {"/" "/detail" "/docs" "/donate"}` | Bypass | — |

API answers are public catalogue data keyed by full URL, so a shared cache never
mixes users. Also enable **Brotli**, **Tiered Cache**, and **Cache Deception
Armor**, and keep the origin port behind a Tunnel or firewall rule.

## Caveats

- Not an official API. Spotify may change or block it at any time; the bundle
  parsing is inherently fragile.
- Browser user agent over standard rustls TLS; if Spotify fingerprints TLS, a
  BoringSSL client (`wreq`) becomes necessary.
- Use at your own risk with regard to Spotify's terms of service.
