import { Fragment, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Link, useSearchParams } from "react-router-dom";
import {
  BadgeCheck,
  ChevronDown,
  ExternalLink,
  Loader2,
  Play,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { Contributor, DetailType, Extras, TrackCredits } from "@/lib/api";
import {
  detail,
  detailHref,
  duration,
  fetchCredits,
  fetchLyrics,
  formatNumber,
  idFromUri,
  openOnSpotify,
  spotifyUrl,
} from "@/lib/api";
import type {
  AlbumDetail,
  ArtistDetail,
  ArtistPick,
  LocalTrack,
  Person,
  PlaylistDetail,
  Release,
  TrackDetail,
} from "@/lib/normalize";
import {
  normalizeAlbum,
  normalizeArtist,
  normalizePlaylist,
  normalizeTrack,
} from "@/lib/normalize";
import { cn } from "@/lib/utils";

type Loaded =
  | { kind: "album"; album: AlbumDetail }
  | { kind: "artist"; artist: ArtistDetail }
  | { kind: "playlist"; playlist: PlaylistDetail }
  | {
      kind: "track";
      track: TrackDetail;
      extras: Extras | null;
      credits: TrackCredits | null;
    };

type View =
  | { status: "loading" }
  | { status: "error"; text: string }
  | { status: "ok"; loaded: Loaded };

const SUPPORTED: DetailType[] = ["album", "artist", "track", "playlist"];

function isDetailType(value: string | null): value is DetailType {
  return SUPPORTED.includes(value as DetailType);
}

const SECTION_TITLE =
  "mt-11 mb-3.5 border-b-2 border-primary pb-2.5 text-[22px] font-bold tracking-[-0.01em]";

export default function DetailPage() {
  const [params] = useSearchParams();
  const type = params.get("type");
  const id = params.get("id");
  const [view, setView] = useState<View>({ status: "loading" });

  useEffect(() => {
    if (!id || !isDetailType(type)) {
      setView({
        status: "error",
        text: "Missing or unsupported detail type.",
      });
      return;
    }

    const controller = new AbortController();
    setView({ status: "loading" });

    (async () => {
      try {
        const body = await detail(type, id, controller.signal);

        if (type === "album") {
          if (!body.albumUnion) throw new Error("album not found");
          const album = normalizeAlbum(body.albumUnion);
          document.title = `${album.name} — spotifree-rs`;
          setView({ status: "ok", loaded: { kind: "album", album } });
          return;
        }

        if (type === "artist") {
          if (!body.artistUnion) throw new Error("artist not found");
          const artist = normalizeArtist(body.artistUnion);
          document.title = `${artist.name} — spotifree-rs`;
          setView({ status: "ok", loaded: { kind: "artist", artist } });
          return;
        }

        if (type === "playlist") {
          if (!body.playlistV2) throw new Error("playlist not found");
          const playlist = normalizePlaylist(body.playlistV2);
          document.title = `${playlist.name} — spotifree-rs`;
          setView({ status: "ok", loaded: { kind: "playlist", playlist } });
          return;
        }

        if (!body.trackUnion) throw new Error("track not found");
        const track = normalizeTrack(body.trackUnion);
        document.title = `${track.name} — spotifree-rs`;
        const [extras, credits] = await Promise.all([
          fetchLyrics(
            {
              title: track.name,
              artist: track.artists[0]?.name || "",
              album: track.album?.name,
              durationMs: track.duration,
            },
            controller.signal,
          ),
          fetchCredits(id, controller.signal),
        ]);
        if (controller.signal.aborted) return;
        setView({
          status: "ok",
          loaded: { kind: "track", track, extras, credits },
        });
      } catch (error) {
        if (controller.signal.aborted) return;

        const hint =
          location.protocol === "file:"
            ? "Open this page through the server (cargo run --bin serve) instead of the file system."
            : error instanceof Error
              ? error.message
              : String(error);
        setView({ status: "error", text: `Failed — ${hint}` });
      }
    })();

    return () => controller.abort();
  }, [type, id]);

  return (
    <main className="mx-auto max-w-[1400px] px-6 pt-7 pb-22 max-sm:px-4 max-sm:pt-5 max-sm:pb-15">
      {view.status === "loading" ? (
        <p className="py-8 text-[15px] text-muted-foreground">Loading…</p>
      ) : view.status === "error" ? (
        <p className="py-8 text-[15px] text-destructive">{view.text}</p>
      ) : (
        <Detail loaded={view.loaded} />
      )}
    </main>
  );
}

function Detail({ loaded }: { loaded: Loaded }) {
  switch (loaded.kind) {
    case "album":
      return <AlbumView album={loaded.album} />;
    case "artist":
      return <ArtistView artist={loaded.artist} />;
    case "playlist":
      return <PlaylistView playlist={loaded.playlist} />;
    case "track":
      return (
        <TrackView
          track={loaded.track}
          extras={loaded.extras}
          credits={loaded.credits}
        />
      );
  }
}

function ArtistLink({ person }: { person: Person }) {
  const id = idFromUri(person.uri);
  return id ? (
    <Link
      to={detailHref("artist", id)}
      className="text-foreground/80 hover:underline"
    >
      {person.name}
    </Link>
  ) : (
    <span>{person.name}</span>
  );
}

function Dot() {
  return <span className="px-1.5">·</span>;
}

function SpotifyLink({ uri, className }: { uri: string; className?: string }) {
  const href = spotifyUrl(uri);
  if (!href) return null;

  return (
    <a
      className={cn(
        "mt-3 inline-block text-sm text-primary underline-offset-2 hover:underline",
        className,
      )}
      href={href}
      target="_blank"
      rel="noopener"
    >
      Open in Spotify →
    </a>
  );
}

function TrackTable({ tracks }: { tracks: LocalTrack[] }) {
  return (
    <Table className="table-fixed">
      <TableHeader>
        <TableRow className="hover:bg-transparent">
          <TableHead className="h-auto w-11 px-2 py-2.5 text-right text-sm font-normal text-muted-foreground max-[640px]:hidden">
            #
          </TableHead>
          <TableHead className="h-auto px-2 py-2.5 text-sm font-normal text-muted-foreground">
            Title
          </TableHead>
          <TableHead className="h-auto w-20 px-2 py-2.5 text-right text-sm font-normal text-muted-foreground max-[640px]:hidden">
            Time
          </TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {tracks.map((track, index) => {
          const url = spotifyUrl(track.uri);
          const open = () => openOnSpotify(track.uri);

          return (
            <TableRow
              key={`${track.uri ?? track.name}-${index}`}
              className={url ? "cursor-pointer" : undefined}
              tabIndex={url ? 0 : undefined}
              role={url ? "link" : undefined}
              onClick={() => {
                if (url) open();
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" && url) open();
              }}
            >
              <TableCell className="px-2 py-3 text-right text-muted-foreground max-[640px]:hidden">
                {track.number || index + 1}
              </TableCell>
              <TableCell className="max-w-0 overflow-hidden px-2 py-3 align-top whitespace-normal">
                <div
                  className="truncate font-bold text-foreground"
                  title={track.name}
                >
                  {track.name || "(untitled)"}
                  {track.explicit ? (
                    <span className="ml-1.5 inline-block bg-muted px-1 align-middle text-[11px] leading-4 font-semibold text-muted-foreground">
                      E
                    </span>
                  ) : null}
                </div>
                {track.artists.length ? (
                  <div
                    className="truncate text-sm text-muted-foreground"
                    title={track.artists.join(", ")}
                  >
                    {track.artists.join(", ")}
                  </div>
                ) : null}
              </TableCell>
              <TableCell className="px-2 py-3 text-right text-muted-foreground max-[640px]:hidden">
                {duration(track.duration)}
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}

function AlbumGrid({ items }: { items: Release[] }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-x-4.5 gap-y-6 max-[960px]:grid-cols-[repeat(auto-fill,minmax(130px,1fr))] max-[400px]:grid-cols-[repeat(auto-fill,minmax(110px,1fr))] max-[400px]:gap-x-3 max-[400px]:gap-y-4">
      {items.map((item, index) => {
        const body = (
          <>
            <img
              className="mb-2 aspect-square w-full bg-secondary object-cover"
              alt=""
              loading="lazy"
              src={item.image || undefined}
            />
            <div className="truncate text-[15px] font-semibold text-foreground group-hover:underline">
              {item.name}
            </div>
            <div className="text-[13px] text-muted-foreground">
              {[item.year, item.type].filter(Boolean).join(" · ")}
            </div>
          </>
        );

        return item.id ? (
          <Link
            className="group block min-w-0 no-underline hover:no-underline"
            key={item.id}
            to={detailHref("album", item.id)}
          >
            {body}
          </Link>
        ) : (
          <div className="group" key={`${item.name}-${index}`}>
            {body}
          </div>
        );
      })}
    </div>
  );
}

function AlbumView({ album }: { album: AlbumDetail }) {
  return (
    <>
      <section className="flex items-end gap-7 max-sm:flex-col max-sm:items-start max-sm:gap-4">
        {album.image ? (
          <img
            className="size-40 flex-none bg-secondary object-cover max-sm:size-28"
            src={album.image}
            alt=""
          />
        ) : null}
        <div className="min-w-0">
          <p className="mb-1.5 text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
            {album.type || "Album"}
          </p>
          <h1 className="mb-2 text-[clamp(1.6rem,3.6vw,2.4rem)] leading-[1.1] font-bold tracking-[-0.03em]">
            {album.name}
          </h1>
          <p className="m-0 text-[15px] text-muted-foreground">
            {album.artists.map((artist, index) => (
              <Fragment key={`${artist.uri}-${index}`}>
                {index > 0 ? <Dot /> : null}
                <ArtistLink person={artist} />
              </Fragment>
            ))}
            {album.year ? (
              <>
                <Dot />
                {album.year}
              </>
            ) : null}
            {album.total ? (
              <>
                <Dot />
                {album.total} track{album.total === 1 ? "" : "s"}
              </>
            ) : null}
            {album.label ? (
              <>
                <Dot />
                {album.label}
              </>
            ) : null}
          </p>
          <SpotifyLink uri={album.uri} />
        </div>
      </section>

      {album.tracks.length === 0 ? (
        <p className="py-8 text-[15px] text-muted-foreground">
          No tracks returned for this album.
        </p>
      ) : (
        <>
          <h2 className={SECTION_TITLE}>Tracks</h2>
          <TrackTable tracks={album.tracks} />
        </>
      )}
    </>
  );
}

function parseUri(uri: string): { type: DetailType; id: string } | null {
  const parts = uri.split(":");
  if (parts.length >= 3) {
    const rawType = parts[parts.length - 2];
    const id = parts[parts.length - 1];
    if (
      rawType === "album" ||
      rawType === "artist" ||
      rawType === "track" ||
      rawType === "playlist"
    ) {
      return { type: rawType, id };
    }
  }
  return null;
}

function formatSocialName(name: string): string {
  const map: Record<string, string> = {
    TWITTER: "Twitter / X",
    INSTAGRAM: "Instagram",
    FACEBOOK: "Facebook",
    WIKIPEDIA: "Wikipedia",
  };
  return map[name.toUpperCase()] || name;
}

function ArtistPickCard({
  pick,
  artist,
}: {
  pick: ArtistPick;
  artist: ArtistDetail;
}) {
  const target = parseUri(pick.uri);

  const cardContent = (
    <div className="flex items-center gap-3.5 rounded-xl border border-border bg-card p-3 transition-colors group-hover:border-primary/50 group-hover:bg-secondary/40">
      {pick.image ? (
        <img
          src={pick.image}
          alt=""
          className="size-16 flex-none rounded-md bg-secondary object-cover"
        />
      ) : null}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-semibold text-foreground group-hover:underline">
          {pick.title}
        </p>
        {pick.subtitle ? (
          <p className="truncate text-xs text-muted-foreground">
            {pick.subtitle}
          </p>
        ) : null}
      </div>
    </div>
  );

  return (
    <div className="mt-8 max-w-md">
      <div className="mb-2 flex items-center gap-2">
        {artist.image ? (
          <img
            src={artist.image}
            alt=""
            className="size-5 rounded-full object-cover"
          />
        ) : null}
        <span className="text-[11px] font-bold tracking-wider text-muted-foreground uppercase">
          Artist Pick
        </span>
      </div>
      {pick.comment ? (
        <p className="mb-2 text-sm italic text-foreground/80">
          “{pick.comment}”
        </p>
      ) : null}
      {target ? (
        <Link
          to={detailHref(target.type, target.id)}
          className="group block no-underline hover:no-underline"
        >
          {cardContent}
        </Link>
      ) : (
        <div className="group">{cardContent}</div>
      )}
    </div>
  );
}

function ArtistView({ artist }: { artist: ArtistDetail }) {
  const stats: string[] = [];
  if (artist.listeners)
    stats.push(`${formatNumber(artist.listeners)} monthly listeners`);
  if (artist.followers)
    stats.push(`${formatNumber(artist.followers)} followers`);

  return (
    <>
      <section className="relative overflow-hidden rounded-2xl border border-border bg-card">
        {artist.headerImage ? (
          <div className="relative h-44 w-full sm:h-56">
            <img
              src={artist.headerImage}
              alt=""
              className="h-full w-full object-cover"
            />
            <div className="absolute inset-0 bg-gradient-to-t from-card via-card/50 to-transparent" />
          </div>
        ) : null}
        <div
          className={`flex flex-col gap-4 p-6 sm:p-8 ${
            artist.headerImage ? "relative -mt-16 sm:-mt-20" : ""
          }`}
        >
          <div className="flex items-center gap-5 max-[400px]:gap-3.5">
            {artist.image ? (
              <img
                className="size-20 flex-none rounded-full border-2 border-card bg-secondary object-cover shadow-md sm:size-28"
                src={artist.image}
                alt=""
              />
            ) : null}
            <div className="min-w-0">
              <div className="mb-1 flex items-center gap-2">
                <p className="text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
                  Artist
                </p>
                {artist.verified ? (
                  <span className="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                    <BadgeCheck className="size-3.5 fill-primary text-background" />
                    Verified
                  </span>
                ) : null}
              </div>
              <h1 className="mb-1.5 text-[clamp(1.5rem,3.4vw,2.25rem)] leading-[1.1] font-bold tracking-[-0.03em]">
                {artist.name}
              </h1>
              {stats.length ? (
                <p className="m-0 text-[15px] text-muted-foreground">
                  {stats.join(" · ")}
                </p>
              ) : null}
            </div>
          </div>
          {artist.bio ? (
            <p className="m-0 max-w-[70ch] text-[15px] whitespace-pre-line text-foreground/80">
              {artist.bio}
            </p>
          ) : null}
          {artist.externalLinks && artist.externalLinks.length > 0 ? (
            <div className="flex flex-wrap items-center gap-2">
              {artist.externalLinks.map((link) => (
                <a
                  key={link.url}
                  href={link.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1.5 rounded-full border border-border bg-secondary/60 px-3 py-1 text-xs font-medium text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  <span>{formatSocialName(link.name)}</span>
                  <ExternalLink className="size-3 opacity-60" />
                </a>
              ))}
            </div>
          ) : null}
          <SpotifyLink uri={artist.uri} />
        </div>
      </section>

      {artist.artistPick ? (
        <ArtistPickCard pick={artist.artistPick} artist={artist} />
      ) : null}

      {artist.topTracks.length ? (
        <>
          <h2 className={SECTION_TITLE}>Popular</h2>
          <TrackTable tracks={artist.topTracks} />
        </>
      ) : null}

      {artist.albums.length ? (
        <>
          <h2 className={SECTION_TITLE}>Albums</h2>
          <AlbumGrid items={artist.albums} />
        </>
      ) : null}

      {artist.singles.length ? (
        <>
          <h2 className={SECTION_TITLE}>Singles and EPs</h2>
          <AlbumGrid items={artist.singles} />
        </>
      ) : null}
    </>
  );
}

function PlaylistView({ playlist }: { playlist: PlaylistDetail }) {
  const stats: string[] = [];
  if (playlist.owner) stats.push(`by ${playlist.owner}`);
  if (playlist.total)
    stats.push(`${playlist.total} track${playlist.total === 1 ? "" : "s"}`);

  return (
    <>
      <section className="flex items-end gap-7 max-sm:flex-col max-sm:items-start max-sm:gap-4">
        {playlist.image ? (
          <img
            className="size-40 flex-none bg-secondary object-cover max-sm:size-28"
            src={playlist.image}
            alt=""
          />
        ) : null}
        <div className="min-w-0">
          <p className="mb-1.5 text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
            Playlist
          </p>
          <h1 className="mb-2 text-[clamp(1.6rem,3.6vw,2.4rem)] leading-[1.1] font-bold tracking-[-0.03em]">
            {playlist.name}
          </h1>
          {stats.length ? (
            <p className="m-0 text-[15px] text-muted-foreground">
              {stats.join(" · ")}
            </p>
          ) : null}
          {playlist.description ? (
            <p className="mt-2.5 max-w-[70ch] text-[15px] whitespace-pre-line text-foreground/80">
              {playlist.description}
            </p>
          ) : null}
          <SpotifyLink uri={playlist.uri} />
        </div>
      </section>

      {playlist.tracks.length === 0 ? (
        <p className="py-8 text-[15px] text-muted-foreground">
          No tracks returned for this playlist.
        </p>
      ) : (
        <>
          <h2 className={SECTION_TITLE}>Tracks</h2>
          <TrackTable tracks={playlist.tracks} />
        </>
      )}
    </>
  );
}

function lyricsMissing(extras: Extras | null): boolean {
  const lyrics = extras?.lyrics;
  return !lyrics || lyrics.instrumental || (!lyrics.plain && !lyrics.synced);
}

function renderContributors(list: Contributor[]) {
  const seen = new Set<string>();
  const unique = list.filter((c) => {
    if (!c.name || seen.has(c.name)) return false;
    seen.add(c.name);
    return true;
  });

  return (
    <span>
      {unique.map((c, i) => {
        const artistId = c.uri ? idFromUri(c.uri) : null;
        return (
          <Fragment key={`${c.name}-${i}`}>
            {i > 0 ? ", " : ""}
            {artistId ? (
              <Link
                to={detailHref("artist", artistId)}
                className="underline-offset-2 hover:underline"
              >
                {c.name}
              </Link>
            ) : (
              c.name
            )}
          </Fragment>
        );
      })}
    </span>
  );
}

function TrackFacts({
  track,
  credits,
  extras,
}: {
  track: TrackDetail;
  credits: TrackCredits | null;
  extras: Extras | null;
}) {
  const rows: Array<[string, ReactNode]> = [];

  if (track.album?.name) {
    rows.push([
      "Album",
      track.album.id ? (
        <Link
          to={detailHref("album", track.album.id)}
          className="underline-offset-2 hover:underline"
        >
          {track.album.name}
        </Link>
      ) : (
        track.album.name
      ),
    ]);
  }
  if (track.album?.year) rows.push(["Released", String(track.album.year)]);

  if (credits) {
    const writers: Contributor[] = [];
    const producers: Contributor[] = [];

    for (const c of credits.contributors) {
      const group = c.roleGroup || "";
      const role = c.role || "";
      if (
        group.includes("Composition") ||
        group.includes("Lyric") ||
        /writer|composer|lyricist|author/i.test(role)
      ) {
        writers.push(c);
      } else if (
        group.includes("Production") ||
        group.includes("Producer") ||
        /producer/i.test(role)
      ) {
        producers.push(c);
      }
    }

    if (writers.length > 0) {
      rows.push(["Writers", renderContributors(writers)]);
    }
    if (producers.length > 0) {
      rows.push(["Producers", renderContributors(producers)]);
    }
    if (credits.sources.length > 0) {
      rows.push(["Label", credits.sources.join(", ")]);
    }
  }

  const hasCredits = rows.some(
    ([k]) => k === "Writers" || k === "Producers" || k === "Label",
  );
  if (!hasCredits) {
    if (track.duration) rows.push(["Length", duration(track.duration)]);
    if (track.playcount) rows.push(["Plays", formatNumber(track.playcount)]);
  }

  if (lyricsMissing(extras) && extras?.isrc) rows.push(["ISRC", extras.isrc]);

  return (
    <Table className="mt-5 table-fixed">
      <TableBody>
        {rows.map(([label, value]) => (
          <TableRow key={label} className="hover:bg-transparent">
            <TableHead className="h-auto w-[38%] px-0 py-[7px] align-top text-sm font-normal text-muted-foreground">
              {label}
            </TableHead>
            <TableCell className="px-0 py-[7px] align-top text-sm break-words whitespace-normal text-foreground/80">
              {value}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function stripTimestamps(text: string): string {
  const lines = text.split(/\r?\n/).map((line) => {
    let rest = line.trimStart();
    while (rest.startsWith("[")) {
      const closeIdx = rest.indexOf("]");
      if (closeIdx === -1) break;
      rest = rest.slice(closeIdx + 1).trimStart();
    }
    return rest.replace(/<[0-9:.]+>/g, "").trim();
  });

  return lines.join("\n").trim();
}

function lyricsLines(text: string) {
  const rawLines = text.trim().split(/\r?\n/);
  const lines: string[] = [];
  let prevEmpty = true; // true di awal untuk membuang baris kosong paling atas

  for (const raw of rawLines) {
    const trimmed = raw.trim();
    if (!trimmed) {
      if (!prevEmpty) {
        lines.push("");
        prevEmpty = true;
      }
    } else {
      lines.push(trimmed);
      prevEmpty = false;
    }
  }

  return lines.map((line, index) => {
    if (!line) return <div className="h-6" key={index} aria-hidden="true" />;
    if (/^\[[^\]]+\]$/.test(line))
      return (
        <p
          className="mt-6.5 mb-2 text-[13px] font-bold tracking-[0.06em] text-primary uppercase first:mt-0"
          key={index}
        >
          {line.slice(1, -1)}
        </p>
      );

    return (
      <p
        className="mb-1 text-[clamp(1.3rem,2.6vw,1.8rem)] leading-[1.45] font-bold tracking-[-0.01em] text-foreground first:mt-0 min-[821px]:text-[48px] min-[821px]:leading-[1.3]"
        key={index}
      >
        {line}
      </p>
    );
  });
}

function LyricsBody({ extras }: { extras: Extras | null }) {
  const lyrics = extras?.lyrics;

  if (!lyrics || lyrics.instrumental || (!lyrics.plain && !lyrics.synced)) {
    return (
      <div className="border px-7 py-6.5 text-[15px] text-muted-foreground">
        <p>
          {lyrics?.instrumental
            ? "Instrumental track — no lyrics to show."
            : "Lyrics not available yet."}
        </p>
        {extras?.isrc ? (
          <p className="mb-0">
            ISRC:{" "}
            <span className="font-mono text-sm text-foreground">
              {extras.isrc}
            </span>
          </p>
        ) : null}
      </div>
    );
  }

  return (
    <>
      <div>
        {lyricsLines(lyrics.plain || stripTimestamps(lyrics.synced || ""))}
      </div>
      <p className="mt-8.5 text-[13px] text-muted-foreground">
        Lyrics from {lyrics.source}.
      </p>
    </>
  );
}

const PIPED_INSTANCES = [
  "https://api.piped.private.coffee",
  "https://pipedapi.ducks.party",
];

async function searchYouTubeVideoId(
  query: string,
  signal?: AbortSignal,
): Promise<string | null> {
  const encoded = encodeURIComponent(query);
  const fetchInstance = async (base: string) => {
    const res = await fetch(`${base}/search?q=${encoded}&filter=videos`, {
      signal: signal ?? AbortSignal.timeout(4500),
    });
    if (!res.ok) throw new Error("Failed");
    const data = await res.json();
    const items = data?.items;
    if (Array.isArray(items)) {
      for (const item of items) {
        if (item?.videoId) return item.videoId as string;
        if (typeof item?.url === "string") {
          const match = item.url.match(/[?&]v=([a-zA-Z0-9_-]{11})/);
          if (match) return match[1];
        }
      }
    }
    throw new Error("No video found");
  };

  try {
    return await Promise.any(
      PIPED_INSTANCES.map((base) => fetchInstance(base)),
    );
  } catch {
    return null;
  }
}

interface FloatingPlayerProps {
  videoId: string;
  title: string;
  artist: string;
  minimized: boolean;
  onToggleMinimize: () => void;
  onClose: () => void;
}

function FloatingPlayer({
  videoId,
  title,
  artist,
  minimized,
  onToggleMinimize,
  onClose,
}: FloatingPlayerProps) {
  const ytUrl = `https://www.youtube.com/watch?v=${videoId}`;

  return (
    <aside
      aria-label="YouTube Player"
      className="fixed right-3 bottom-3 z-50 w-84 max-sm:w-56 overflow-hidden rounded-none border border-border bg-card shadow-2xl transition-all duration-200"
    >
      <div className="flex items-center justify-between gap-1.5 px-3 py-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-semibold text-foreground">
            {title}
          </p>
          <p className="truncate text-[10px] text-muted-foreground">{artist}</p>
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          <a
            href={ytUrl}
            target="_blank"
            rel="noopener noreferrer"
            title="Buka di YouTube"
            className="rounded-none p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <ExternalLink className="size-3.5" />
          </a>
          <button
            type="button"
            onClick={onToggleMinimize}
            title={minimized ? "Buka pemutar" : "Ciutkan pemutar"}
            className="cursor-pointer rounded-none p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <ChevronDown
              className={cn(
                "size-3.5 transition-transform duration-300 ease-in-out",
                minimized && "rotate-180",
              )}
            />
          </button>
          <button
            type="button"
            onClick={onClose}
            title="Tutup pemutar"
            className="cursor-pointer rounded-none p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>

      <div
        className={cn(
          "grid transition-[grid-template-rows,opacity] duration-300 ease-in-out",
          minimized
            ? "grid-rows-[0fr] opacity-0"
            : "grid-rows-[1fr] opacity-100",
        )}
      >
        <div className="overflow-hidden">
          <div className="aspect-video w-full bg-black">
            <iframe
              src={`https://www.youtube-nocookie.com/embed/${videoId}?autoplay=1`}
              title={`${title} - ${artist}`}
              className="h-full w-full border-0"
              allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
              allowFullScreen
            />
          </div>
        </div>
      </div>
    </aside>
  );
}

function TrackView({
  track,
  extras,
  credits,
}: {
  track: TrackDetail;
  extras: Extras | null;
  credits: TrackCredits | null;
}) {
  const [player, setPlayer] = useState<{
    status: "idle" | "searching" | "ready" | "error";
    videoId?: string;
    minimized?: boolean;
  }>({ status: "idle" });

  const artistNames = track.artists.map((a) => a.name).join(", ");
  const searchQuery =
    `${track.artists.map((a) => a.name).join(" ")} ${track.name}`.trim();

  useEffect(() => {
    setPlayer({ status: "idle" });
  }, [track.uri]);

  const handlePlay = async () => {
    if (player.status === "ready") {
      setPlayer((p) => ({ ...p, minimized: false }));
      return;
    }
    if (player.status === "searching") return;

    setPlayer({ status: "searching" });
    const videoId = await searchYouTubeVideoId(searchQuery);
    if (videoId) {
      setPlayer({ status: "ready", videoId, minimized: false });
    } else {
      setPlayer({ status: "error" });
    }
  };

  return (
    <section className="grid grid-cols-[minmax(280px,430px)_1fr] items-start gap-10 max-[820px]:grid-cols-1 max-[820px]:gap-8">
      <aside className="min-w-0 min-[821px]:sticky min-[821px]:top-[4.25rem]">
        <div className="flex items-center gap-4 min-[821px]:block">
          {track.image ? (
            <img
              className="size-24 flex-none bg-secondary object-cover min-[821px]:mb-4.5 min-[821px]:aspect-square min-[821px]:h-auto min-[821px]:w-full min-[821px]:max-w-[360px]"
              src={track.image}
              alt=""
            />
          ) : null}
          <div className="min-w-0">
            <p className="mb-1.5 text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
              {track.album?.type || "Track"}
            </p>
            <h1 className="mb-2 text-[clamp(1.6rem,3.6vw,2.25rem)] leading-[1.1] font-bold tracking-[-0.03em]">
              {track.name}
            </h1>

            <p className="m-0 text-[clamp(1rem,1.8vw,1.2rem)] text-muted-foreground">
              {track.artists.map((artist, index) => (
                <Fragment key={`${artist.uri}-${index}`}>
                  {index > 0 ? <Dot /> : null}
                  <ArtistLink person={artist} />
                </Fragment>
              ))}
            </p>
          </div>
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-3">
          <Button
            type="button"
            className="h-9 cursor-pointer rounded-none bg-primary px-4 text-sm font-semibold text-primary-foreground shadow-sm transition-colors hover:bg-primary/80"
            onClick={handlePlay}
            disabled={player.status === "searching"}
          >
            {player.status === "searching" ? (
              <>
                <Loader2 className="size-4 animate-spin" />
                <span>Mencari...</span>
              </>
            ) : (
              <>
                <Play className="size-4 fill-current" />
                <span>
                  {player.status === "ready" ? "Buka Pemutar" : "Putar Lagu"}
                </span>
              </>
            )}
          </Button>
          <SpotifyLink uri={track.uri} className="mt-0" />
        </div>

        <TrackFacts track={track} credits={credits} extras={extras} />
      </aside>

      <div className="min-h-50">
        <LyricsBody extras={extras} />
      </div>

      {player.status === "ready" && player.videoId ? (
        <FloatingPlayer
          videoId={player.videoId}
          title={track.name}
          artist={artistNames}
          minimized={Boolean(player.minimized)}
          onToggleMinimize={() =>
            setPlayer((p) => ({ ...p, minimized: !p.minimized }))
          }
          onClose={() => setPlayer({ status: "idle" })}
        />
      ) : null}

      {player.status === "error" ? (
        <div className="fixed right-3 bottom-3 max-sm:right-2.5 max-sm:bottom-2.5 z-50 w-76 max-sm:w-60 rounded-none border border-destructive/40 bg-card p-3 shadow-2xl">
          <div className="flex items-start justify-between gap-2">
            <p className="text-sm font-semibold text-foreground">
              Video tidak ditemukan
            </p>
            <button
              type="button"
              onClick={() => setPlayer({ status: "idle" })}
              className="cursor-pointer text-muted-foreground hover:text-foreground"
            >
              <X className="size-4" />
            </button>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            Tidak dapat memuat video otomatis. Buka langsung di YouTube.
          </p>
          <div className="mt-3 flex gap-2">
            <a
              href={`https://www.youtube.com/results?search_query=${encodeURIComponent(searchQuery)}`}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-semibold text-primary-foreground hover:bg-primary/90"
            >
              <ExternalLink className="size-3" />
              Buka YouTube
            </a>
            <Button
              variant="outline"
              size="xs"
              className="h-auto cursor-pointer py-1.5 text-xs"
              onClick={handlePlay}
            >
              Coba Lagi
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
