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

/// `GET /api/{type}/{id}` — Spotify wraps the object under a per-type key.
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
    `/api/search?q=${encodeURIComponent(query)}&limit=20`,
    signal,
  );
}

export function detail(
  type: DetailType,
  id: string,
  signal?: AbortSignal,
): Promise<DetailPayload> {
  return getJson<DetailPayload>(
    `/api/${type}/${encodeURIComponent(id)}`,
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
    `/api/charts/${encodeURIComponent(country)}?${query}`,
    signal,
  );
}

/// Lyrics and ISRC are best-effort; a failure leaves the page usable.
export async function extras(
  id: string,
  signal?: AbortSignal,
): Promise<Extras | null> {
  try {
    return await getJson<Extras>(
      `/api/lyrics/${encodeURIComponent(id)}`,
      signal,
    );
  } catch {
    return null;
  }
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
    }>(`/api/track/${encodeURIComponent(id)}/credits`, signal);

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
