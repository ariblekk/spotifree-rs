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
- In-process TTL cache for upstream answers (search 30s, track 2m,
  album/artist/playlist 5m, lyrics 30m).
- Singleflight: concurrent misses for one cache key share a single upstream call.
- Server-side collection paging — albums through to the end, playlists up to 1000
  items — merged into the normal response.
- Retry on a rotated web-player deployment: a missed persisted hash, `401`, or
  `PersistedQueryNotFound` triggers one rediscovery and one retry.
- `/healthz` now reports `warm`, `hashes`, `token_valid`, `token_expires_in_ms`,
  `cache_entries`, `cache_hits`, `cache_misses`, and answers `503` until a build
  is discovered.
- Graceful shutdown on Ctrl-C / `SIGTERM`, with `stop_grace_period: 15s` in the
  production compose file.
- Cacheable `Cache-Control` per endpoint so browsers and Cloudflare can cache API
  answers, plus a "Behind Cloudflare" cache-rules guide in the README.
- Unknown `/api/*` paths answer `404 {"error":"unknown API path"}` instead of the
  SPA shell.
- `GET /api/charts/{id|global}` (and `/api/charts`): ranked chart plus trending
  songs, popular albums and artists. Markets are limited to `id` and `global`;
  `top=daily|alltime` picks the expanded chart and rows are capped at 50. Cached
  for 15 minutes.

### Changed

- `Session` is shared and serves requests concurrently (token and discovery are
  synchronized internally; no global mutex).
- Web-player build is discovered at startup and refreshed every 30 minutes in the
  background; the anonymous token is pre-fetched at boot.
- Lyrics providers, bundle downloads, and chunk fetches run in parallel.
- Responses are brotli/gzip-compressed.
- README trimmed to the essentials.
- UI typography is self-hosted Geist (body) and Geist Mono (code), replacing the
  TeX Gyre Heros webfont.
- API page trimmed to the essentials with curl/JS/Python tabs.
- Donate page keeps only Saweria, with a simpler responsive layout (QRIS removed).
- A plain primary-colour square favicon (`/favicon.svg`) is served and cached
  immutably.
- Dark theme: near-black background, bone-white text, lime `#ccf05b` accents.
- The primary accent is lime (`#ccf05b`) with dark text on top; `--ring`,
  scrollbars and the favicon follow the same colour.
- Detail pages are more responsive; artist headers use a compact social-media
  layout and lyrics render at a readable size instead of 3rem.
- New `/charts` page: a 50-row chart table with rank movement and play counts,
  Indonesia/Global and Daily/All-time toggles, movers, trending, popular albums
  and artists.

### Removed

- Rate limiting and the concurrency cap (the reverse proxy / CDN owns that now).
- Podcasts from search results, models, CLI, server filters, and frontend.
- The UI footer.

### Fixed

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
