/// Typed shapes returned by the Rust API. Detail payloads are Spotify's own
/// objects, so the raw JSON stays `unknown` until `normalize.ts` narrows it.

export interface Track {
  id: string;
  uri: string;
  name: string;
  artists: string[];
  album: string | null;
  duration_ms: number | null;
  image: string | null;
}

export interface Artist {
  id: string;
  uri: string;
  name: string;
  image: string | null;
}

export interface Album {
  id: string;
  uri: string;
  name: string;
  artists: string[];
  image: string | null;
}

export interface Playlist {
  id: string;
  uri: string;
  name: string;
  owner: string | null;
  image: string | null;
}

export interface SearchResults {
  query: string;
  tracks: Track[];
  artists: Artist[];
  albums: Album[];
  playlists: Playlist[];
}

export interface Charts {
  country: string;
  color: string | null;
  charts: Chart[];
  trending: Track[];
  popular_albums: Album[];
  popular_artists: Artist[];
}

export interface ChartEntry {
  position: number;
  previous: number | null;
  plays: number | null;
  name: string;
  id: string;
  uri: string;
  artists: string[];
  album: string | null;
  duration_ms: number | null;
  image: string | null;
}

export interface Chart {
  id: string;
  uri: string;
  name: string;
  description: string | null;
  image: string | null;
  frequency: string | null;
  updated_at: string | null;
  rank_type: string | null;
  entries: ChartEntry[];
}

export interface Lyrics {
  kind: string;
  source: string;
  instrumental: boolean;
  plain: string | null;
  synced: string | null;
}

export interface Extras {
  isrc: string | null;
  lyrics: Lyrics | null;
}

export interface Contributor {
  name: string;
  role: string;
  roleGroup?: string | null;
  uri?: string | null;
}

export interface TrackCredits {
  contributors: Contributor[];
  sources: string[];
}

export type DetailType = "album" | "artist" | "track" | "playlist";

/// `GET /v1/{type}/{id}` — Spotify wraps the object under a per-type key.
export type DetailPayload = Record<string, unknown>;

/// Where the Rust API lives. Empty (the default) keeps requests relative, so a
/// build served by the Rust binary stays same-origin. Set `VITE_API_BASE` when
/// the UI is hosted on its own (Vercel, Cloudflare Pages, a `file://` copy).
export const API_BASE = (import.meta.env.VITE_API_BASE ?? "")
  .trim()
  .replace(/\/+$/, "");

/// Absolute API URL for `path`, for callers that need the full address.
export function apiUrl(path: string) {
  return `${API_BASE}${path}`;
}

async function getJson<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(apiUrl(path), { signal });
  const body = await response.json().catch(() => null);

  if (!response.ok) {
    const message = (body as { error?: string } | null)?.error;
    throw new Error(message || `HTTP ${response.status}`);
  }

  return body as T;
}

export function search(query: string, signal?: AbortSignal) {
  return getJson<SearchResults>(
    `/v1/search?q=${encodeURIComponent(query)}&limit=20`,
    signal,
  );
}

export function detail(
  type: DetailType,
  id: string,
  signal?: AbortSignal,
): Promise<DetailPayload> {
  return getJson<DetailPayload>(
    `/v1/${type}/${encodeURIComponent(id)}`,
    signal,
  );
}

/// Country charts: chart playlists plus trending and popular lists.
///
/// Only `id` (Indonesia) and `global` are served; `top` is `daily` (Spotify's
/// "Top 50") or `alltime` ("Top Songs"). Rows are capped server-side at 50.
export function charts(
  country: string,
  top?: string,
  limit = 50,
  signal?: AbortSignal,
) {
  const query = new URLSearchParams({ limit: String(limit) });
  if (top) query.set("top", top);

  return getJson<Charts | null>(
    `/v1/charts/${encodeURIComponent(country)}?${query}`,
    signal,
  );
}

export interface BrowseCategory {
  id: string;
  title: string;
  color?: string | null;
  image_url?: string | null;
  href: string;
}

export interface GenreItem {
  title: string;
  description?: string | null;
  href: string;
  image_url?: string | null;
  followers?: number | null;
  artists?: string[] | null;
}

export interface GenreSection {
  title: string;
  items: GenreItem[];
}

export interface GenrePage {
  id: string;
  title: string;
  subtitle?: string | null;
  sections: GenreSection[];
}

export function browse(signal?: AbortSignal) {
  return getJson<BrowseCategory[]>("/v1/browse", signal);
}

export function genre(id: string, signal?: AbortSignal) {
  return getJson<GenrePage>(`/v1/genre/${encodeURIComponent(id)}`, signal);
}

export interface DonateInfo {
  qris: boolean;
  amounts: number[];
  min: number;
  max: number;
}

export interface DonateBill {
  qris_id: string;
  trx_id: string;
  amount: number;
  base_amount: number;
  unique_code: number;
  qris_code: string;
  expires_at: string;
}

/// Whether the server has QRIS credentials and which presets it offers.
export function donateInfo(signal?: AbortSignal) {
  return getJson<DonateInfo>("/v1/donate", signal);
}

/// Create a QRIS bill. The API key stays on the server; only the EMVCo string
/// comes back, ready to be drawn as a QR.
export async function donateQris(
  amount: number,
  signal?: AbortSignal,
): Promise<DonateBill> {
  const response = await fetch(apiUrl("/v1/donate/qris"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ amount }),
    signal,
  });

  const body = await response.json().catch(() => null);
  if (!response.ok) {
    const message = (body as { error?: string } | null)?.error;
    throw new Error(message || `HTTP ${response.status}`);
  }

  return body as DonateBill;
}

/// Poll a bill until it is paid, expires, or `signal` aborts.
export function donateStatus(id: string, signal?: AbortSignal) {
  return getJson<{ paid: boolean; status: string | null }>(
    `/v1/donate/qris/${encodeURIComponent(id)}`,
    signal,
  );
}

export interface LyricsHints {
  title: string;
  artist: string;
  album?: string | null;
  durationMs?: number | null;
}

/// Fetch lyrics directly from public providers (lrc.red, falling back to LRCLIB)
/// client-side without routing through backend.
export async function fetchLyrics(
  hints: LyricsHints,
  signal?: AbortSignal,
): Promise<Extras> {
  const title = hints.title.trim();
  const artist = hints.artist.trim();
  if (!title || !artist) {
    return { isrc: null, lyrics: null };
  }

  const durationSec = hints.durationMs
    ? Math.round(hints.durationMs / 1000)
    : undefined;

  // 1. Primary: lrc.red (client-side)
  try {
    const q = `${title} ${artist}`;
    const searchRes = await fetch(
      `https://lrc.red/search.json?q=${encodeURIComponent(q)}`,
      { signal },
    );
    if (searchRes.ok) {
      const data = (await searchRes.json()) as {
        hits?: Array<{
          isrc: string;
          title: string;
          artist: string;
          duration?: number;
        }>;
      };

      const hits = data?.hits ?? [];
      if (hits.length > 0) {
        let best = hits[0];
        if (durationSec) {
          best = hits.reduce((closest, curr) => {
            const diffCurr = Math.abs((curr.duration ?? 0) - durationSec);
            const diffClosest = Math.abs((closest.duration ?? 0) - durationSec);
            return diffCurr < diffClosest ? curr : closest;
          }, hits[0]);
        }

        if (best.isrc) {
          const lrcRes = await fetch(
            `https://lrc.red/s/${encodeURIComponent(best.isrc)}.lrc`,
            { signal },
          );
          if (lrcRes.ok) {
            const synced = await lrcRes.text();
            if (synced && synced.trim()) {
              return {
                isrc: best.isrc,
                lyrics: {
                  kind: "synced",
                  source: "lrc.red",
                  instrumental: false,
                  plain: null,
                  synced: synced.trim(),
                },
              };
            }
          }
        }
      }
    }
  } catch {
    // fallback to lrclib
  }

  // 2. Fallback: LRCLIB (client-side)
  try {
    const params = new URLSearchParams({
      track_name: title,
      artist_name: artist,
    });
    if (hints.album) params.set("album_name", hints.album);
    if (durationSec) params.set("duration", String(durationSec));

    const res = await fetch(`https://lrclib.net/api/get?${params}`, { signal });
    if (res.ok) {
      const data = await res.json();
      const lyrics = parseLrcLib(data);
      if (lyrics) return { isrc: null, lyrics };
    }
  } catch {
    // fallback to lrclib search
  }

  try {
    const params = new URLSearchParams({
      track_name: title,
      artist_name: artist,
    });
    const res = await fetch(`https://lrclib.net/api/search?${params}`, {
      signal,
    });
    if (res.ok) {
      const records = (await res.json()) as Array<{
        duration?: number;
        instrumental?: boolean;
        plainLyrics?: string;
        syncedLyrics?: string;
      }>;
      if (Array.isArray(records) && records.length > 0) {
        let best = records[0];
        if (durationSec) {
          best = records.reduce((closest, curr) => {
            const diffCurr = Math.abs((curr.duration ?? 0) - durationSec);
            const diffClosest = Math.abs((closest.duration ?? 0) - durationSec);
            return diffCurr < diffClosest ? curr : closest;
          }, records[0]);
        }
        const lyrics = parseLrcLib(best);
        if (lyrics) return { isrc: null, lyrics };
      }
    }
  } catch {
    // best-effort
  }

  return { isrc: null, lyrics: null };
}

function parseLrcLib(data: {
  instrumental?: boolean;
  plainLyrics?: string;
  syncedLyrics?: string;
}): Lyrics | null {
  if (!data) return null;
  if (data.instrumental) {
    return {
      kind: "instrumental",
      source: "lrclib.net",
      instrumental: true,
      plain: null,
      synced: null,
    };
  }
  const synced = data.syncedLyrics?.trim() || null;
  const plain = data.plainLyrics?.trim() || null;
  if (!synced && !plain) return null;

  return {
    kind: synced ? "synced" : "plain",
    source: "lrclib.net",
    instrumental: false,
    plain,
    synced,
  };
}

export async function fetchCredits(
  id: string,
  signal?: AbortSignal,
): Promise<TrackCredits | null> {
  try {
    const raw = await getJson<{
      trackUnion?: {
        creditsTrait?: {
          contributors?: {
            items?: Array<{
              name: string;
              role: string;
              roleGroup?: { name: string } | null;
              uri?: string | null;
            }>;
          };
          sources?: {
            items?: Array<{ name: string }>;
          };
        };
      };
    }>(`/v1/track/${encodeURIComponent(id)}/credits`, signal);

    const trait = raw?.trackUnion?.creditsTrait;
    if (!trait) return null;

    const contributors: Contributor[] = (trait.contributors?.items ?? []).map(
      (c) => ({
        name: c.name,
        role: c.role,
        roleGroup: c.roleGroup?.name ?? null,
        uri: c.uri ?? null,
      }),
    );
    const sources: string[] = (trait.sources?.items ?? [])
      .map((s) => s.name)
      .filter(Boolean);

    return { contributors, sources };
  } catch {
    return null;
  }
}

const numberFormat = new Intl.NumberFormat("en");

export function formatNumber(value: number): string {
  return numberFormat.format(value);
}

export function duration(ms: number | null | undefined): string {
  if (!ms || ms < 0) return "";
  const total = Math.round(ms / 1000);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

export function spotifyUrl(uri: string | null | undefined): string | null {
  const match = /^spotify:([a-z]+):(.+)$/.exec(uri || "");
  return match ? `https://open.spotify.com/${match[1]}/${match[2]}` : null;
}

export function idFromUri(uri: string | null | undefined): string | null {
  const match = /^spotify:[a-z]+:(.+)$/.exec(uri || "");
  return match ? match[1] : null;
}

export function detailHref(type: DetailType, id: string): string {
  return `/detail?type=${type}&id=${encodeURIComponent(id)}`;
}

export function openOnSpotify(uri: string | null | undefined) {
  const url = spotifyUrl(uri);
  if (url) window.open(url, "_blank", "noopener");
}
