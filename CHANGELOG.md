# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions use [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `VITE_API_BASE`: build-time env var for the UI. Unset keeps requests relative
  (same-origin, used by the all-in-one image and the Vite dev proxy); set it to
  the API URL when the UI is hosted on its own, e.g. Vercel or Cloudflare Pages.
- CORS for separately hosted UIs: origins listed in `ALLOWED_ORIGIN` get
  `Access-Control-Allow-Origin` plus `Vary: Origin`, and `OPTIONS` preflights are
  answered (or refused, for unlisted origins) before routing.
- `Dockerfile.full` keeps the all-in-one image; `Dockerfile` now builds the API
  only and copies nothing from `web/`.
- In-process TTL cache for upstream answers (search 30s, suggest 60s, track 2m,
  credits 1h, recommendations 5m, album/artist/playlist 5m, charts 15m).
- Singleflight: concurrent misses for one cache key share a single upstream call.
- Server-side collection paging — albums through to the end, playlists up to 1000
  items — merged into the normal response.
- Retry on a rotated web-player deployment: a missed persisted hash, `401`, or
  `PersistedQueryNotFound` triggers one rediscovery and one retry.
- `/healthz` now reports `warm`, `hashes`, `token_valid`, `token_expires_in_ms`,
  `cache_entries`, `cache_hits`, `cache_misses`, `uptime_secs`, `cache_bytes`,
  and answers `503` until a build is discovered.
- Graceful shutdown on Ctrl-C / `SIGTERM`, with `stop_grace_period: 15s` in the
  production compose file.
- Cacheable `Cache-Control` per endpoint so browsers and Cloudflare can cache API
  answers, plus a "Behind Cloudflare" cache-rules guide in the README.
- Unknown `/v1/*` paths answer `404 {"error":"unknown API path"}` instead of the
  SPA shell.
- Versioned `/v1` API endpoints:
  - `GET /v1/search/suggest?q=<term>`: search suggestion chips.
  - `GET /v1/track/{id}/credits`: track contributors and sources.
  - `GET /v1/track/{id}/recommendations`: track recommendations.
  - `GET /v1/artist/{id}/discography`: artist discography filtered by release type.
  - `GET /v1/browse`: curated browse categories.
  - `GET /v1/genre/{id}`: genre pages with subgenres, playlists, and releases.
- Client-side lyrics retrieval via `lrc.red` with fallback to `lrclib.net`.
- Donate page: dynamic QRIS through a payment gateway (`qris.blek.my.id`), with
  `POST /v1/donate/qris`, `GET /v1/donate/qris/{id}` polling and a `GET /v1/donate`
  capability probe. The gateway key stays on the server (`QRIS_BASE_URL` /
  `QRIS_API_KEY`); the card hides when it is unset. Saweria stays as a fallback.
- "Play on YouTube" direct action on track details.
- Search suggestion chips on the search page.
- `GET /v1/charts/{id|global}` (and `/v1/charts`): ranked chart plus trending
  songs, popular albums and artists. Markets are limited to `id` and `global`;
  `top=daily|alltime` picks the expanded chart and rows are capped at 50. Cached
  for 15 minutes.

### Changed

- `Session` is shared and serves requests concurrently (token and discovery are
  synchronized internally; no global mutex).
- Web-player build is discovered at startup and refreshed every 30 minutes in the
  background; the anonymous token is pre-fetched at boot.
- Bundle downloads and chunk fetches run in parallel.
- Responses are brotli/gzip-compressed.
- Server CSP header updated to allow direct browser lyrics fetches from `https://lrc.red` and `https://lrclib.net`.
- README trimmed to the essentials.
- UI typography is self-hosted Geist (body) and Geist Mono (code), replacing the
  TeX Gyre Heros webfont.
- Documentation page moved to `/docs` with curl/JS/Python tabs.
- Donate page keeps only Saweria, with a simpler responsive layout (QRIS removed).
- A plain primary-colour square favicon (`/favicon.svg`) is served and cached
  immutably.
- Dark theme: near-black background, bone-white text, lime `#ccf05b` accents.
- The primary accent is lime (`#ccf05b`) with dark text on top; `--ring`,
  scrollbars and the favicon follow the same colour.
- Detail pages are more responsive: artist headers use a compact social-media
  layout (cover on left, title on right on mobile), desktop sidebar proportions
  adjusted, and lyrics render at 48px on desktop.
- New `/charts` page: a 50-row chart table with rank movement and play counts,
  Indonesia/Global and Daily/All-time toggles, movers, trending, popular albums
  and artists.

### Removed

- Backend lyrics route (`GET /v1/lyrics/{id}`) removed to eliminate upstream blocking and reduce server load; lyrics are now fetched directly by the browser.
- Raw LRC toggle view in frontend; replaced with clean plain text presentation.
- Rate limiting and the concurrency cap (the reverse proxy / CDN owns that now).
- Podcasts from search results, models, CLI, server filters, and frontend.
- The UI footer.

### Fixed

- Leading empty lines and gap at the top of lyrics display caused by stripped LRC headers.
- Spacing above Artist Pick card on artist detail view (`mt-8`).
- Charts: `monthly` always returned the wrong chart (Spotify publishes no
  monthly chart playlist, so the selector fell through to the first entry) and
  `global` returned the local charts renamed with global artwork. The chart is
  now picked by chart family and market, and the label follows the market asked
  for.
- Entries whose play count was missing reported `plays: 0` from the fallback
  attribute instead of falling back to the per-chart count.
- Large albums and playlists were silently truncated at 50 / 100 items; they are
  now paged and merged.
- `/healthz` reported `ok: true` even when the build was not discovered.

## [0.1.0] - 2026-11-14

Initial commit (863d4fd): anonymous authorization, persisted-query execution, and
typed search over tracks, artists, albums, and playlists, with a Vite + React SPA
served by `src/bin/serve.rs`.
