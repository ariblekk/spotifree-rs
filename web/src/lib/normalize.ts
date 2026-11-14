/// Normalizers: Spotify's detail payloads differ per resource, so each page
/// gets one small shape instead of reaching into nested unions in JSX.
///
/// Everything here takes `any` on purpose — these are upstream payloads with no
/// published schema, and every field is treated as optional.

/* eslint-disable @typescript-eslint/no-explicit-any */

import { idFromUri } from "./api";

export interface Person {
  name: string;
  uri?: string;
}

export interface LocalTrack {
  name: string;
  uri?: string;
  number?: number;
  duration?: number;
  explicit?: boolean;
  artists: string[];
}

export interface Release {
  id: string | null;
  name: string;
  image: string | null;
  year?: number;
  type?: string;
}

export interface AlbumDetail {
  name: string;
  uri: string;
  type?: string;
  image: string | null;
  year: string;
  label?: string;
  total: number;
  artists: Person[];
  tracks: LocalTrack[];
}

export interface ArtistDetail {
  name: string;
  uri: string;
  image: string | null;
  listeners?: number;
  followers?: number;
  bio: string;
  topTracks: LocalTrack[];
  albums: Release[];
  singles: Release[];
}

export interface PlaylistDetail {
  name: string;
  uri: string;
  image: string | null;
  description: string;
  owner: string;
  total: number;
  tracks: LocalTrack[];
}

export interface TrackDetail {
  name: string;
  uri: string;
  image: string | null;
  duration?: number;
  playcount: number;
  artists: Person[];
  album: { id: string | null; name: string; year?: number; type?: string } | null;
}

export function pickImage(art: any): string | null {
  const sources: any[] = art?.sources ?? [];
  if (sources.length === 0) return null;

  return [...sources].sort((a, b) => (b.width || 0) - (a.width || 0))[0].url;
}

function artistNames(items: any[]): string[] {
  return items.map((artist) => artist.profile?.name).filter(Boolean);
}

export function normalizeAlbum(union: any): AlbumDetail {
  const tracks: any[] = (union.tracksV2?.items || []).map(
    (entry: any) => entry.track || entry,
  );

  return {
    name: union.name,
    uri: union.uri,
    type: union.type,
    image: pickImage(union.coverArt),
    year: (union.date?.isoString || "").slice(0, 4),
    label: union.label,
    total: union.tracksV2?.totalCount ?? tracks.length,
    artists: (union.artists?.items || []).map((artist: any) => ({
      name: artist.profile?.name || "",
      uri: artist.uri,
    })),
    tracks: tracks.map((track: any) => ({
      name: track.name,
      uri: track.uri,
      number: track.trackNumber,
      duration: track.duration?.totalMilliseconds,
      explicit: track.contentRating?.label === "EXPLICIT",
      artists: artistNames(track.artists?.items || []),
    })),
  };
}

function releases(section: any): Release[] {
  return (section?.items || [])
    .flatMap((item: any) => item.releases?.items || [])
    .map((release: any) => ({
      id: release.id || idFromUri(release.uri),
      name: release.name,
      image: pickImage(release.coverArt),
      year: release.date?.year,
      type: release.type,
    }));
}

export function normalizeArtist(union: any): ArtistDetail {
  const discography = union.discography || {};
  const topTracks = (discography.topTracks?.items || [])
    .map((entry: any) => entry.track)
    .filter(Boolean);

  return {
    name: union.profile?.name,
    uri: union.uri,
    image: pickImage(union.visuals?.avatarImage),
    listeners: union.stats?.monthlyListeners,
    followers: union.stats?.followers,
    bio: union.profile?.biography?.text || "",
    topTracks: topTracks.map((track: any) => ({
      name: track.name,
      uri: track.uri,
      duration: track.duration?.totalMilliseconds,
      explicit: track.contentRating?.label === "EXPLICIT",
      artists: artistNames(track.artists?.items || []),
    })),
    albums: releases(discography.albums),
    singles: releases(discography.singles),
  };
}

export function normalizePlaylist(union: any): PlaylistDetail {
  const items: any[] = (union.content?.items || [])
    .map((entry: any) => entry.itemV2?.data)
    .filter(Boolean);

  return {
    name: union.name,
    uri: union.uri,
    image: pickImage(union.images?.items?.[0]),
    description: union.description || "",
    owner: union.ownerV2?.data?.name || "",
    total: union.content?.totalCount ?? items.length,
    tracks: items.map((track: any) => ({
      name: track.name,
      uri: track.uri,
      duration:
        track.trackDuration?.totalMilliseconds ??
        track.duration?.totalMilliseconds,
      explicit: track.contentRating?.label === "EXPLICIT",
      artists: artistNames(track.artists?.items || []),
    })),
  };
}

export function normalizeTrack(union: any): TrackDetail {
  const album = union.albumOfTrack;
  const artists = [
    ...(union.firstArtist?.items || []),
    ...(union.otherArtists?.items || []),
  ].map((artist: any) => ({
    name: artist.profile?.name || "",
    uri: artist.uri,
  }));

  return {
    name: union.name,
    uri: union.uri,
    image: pickImage(album?.coverArt),
    duration: union.duration?.totalMilliseconds,
    playcount: Number(union.playcount) || 0,
    artists,
    album: album
      ? {
          id: album.id || idFromUri(album.uri),
          name: album.name,
          year: album.date?.year,
          type: album.type,
        }
      : null,
  };
}
