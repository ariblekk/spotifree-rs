import { Fragment, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Link, useSearchParams } from "react-router-dom";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { DetailType, Extras } from "@/lib/api";
import {
  detail,
  detailHref,
  duration,
  extras as fetchExtras,
  formatNumber,
  idFromUri,
  openOnSpotify,
  spotifyUrl,
} from "@/lib/api";
import type {
  AlbumDetail,
  ArtistDetail,
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
  | { kind: "track"; track: TrackDetail; extras: Extras | null };

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
        const extras = await fetchExtras(id, controller.signal);
        if (controller.signal.aborted) return;
        setView({ status: "ok", loaded: { kind: "track", track, extras } });
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
      return <TrackView track={loaded.track} extras={loaded.extras} />;
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

function SpotifyLink({ uri }: { uri: string }) {
  const href = spotifyUrl(uri);
  if (!href) return null;

  return (
    <a
      className="mt-3 inline-block text-sm text-primary underline-offset-2 hover:underline"
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
              <TableCell className="px-2 py-3 align-top whitespace-normal">
                <div className="truncate font-bold text-foreground">
                  {track.name || "(untitled)"}
                  {track.explicit ? (
                    <span className="ml-1.5 inline-block bg-muted px-1 align-middle text-[11px] leading-4 font-semibold text-muted-foreground">
                      E
                    </span>
                  ) : null}
                </div>
                {track.artists.length ? (
                  <div className="truncate text-sm text-muted-foreground">
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

function ArtistView({ artist }: { artist: ArtistDetail }) {
  const stats: string[] = [];
  if (artist.listeners)
    stats.push(`${formatNumber(artist.listeners)} monthly listeners`);
  if (artist.followers)
    stats.push(`${formatNumber(artist.followers)} followers`);

  return (
    <>
      <section className="flex flex-col gap-4">
        <div className="flex items-center gap-5 max-[400px]:gap-3.5">
          {artist.image ? (
            <img
              className="size-20 flex-none rounded-full bg-secondary object-cover sm:size-28"
              src={artist.image}
              alt=""
            />
          ) : null}
          <div className="min-w-0">
            <p className="mb-1 text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
              Artist
            </p>
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
        <SpotifyLink uri={artist.uri} />
      </section>

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

function TrackFacts({
  track,
  extras,
}: {
  track: TrackDetail;
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
  if (track.duration) rows.push(["Length", duration(track.duration)]);
  if (track.playcount) rows.push(["Plays", formatNumber(track.playcount)]);
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
  return text.replace(/^\s*(?:\[[0-9:.]+\]\s*)+/gm, "");
}

function lyricsLines(text: string) {
  return text.split(/\r?\n/).map((raw, index) => {
    const line = raw.trim();

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
        className="mb-1 text-[clamp(1.3rem,2.6vw,1.8rem)] leading-[1.45] font-bold tracking-[-0.01em] text-foreground min-[821px]:text-[48px] min-[821px]:leading-[1.3]"
        key={index}
      >
        {line}
      </p>
    );
  });
}

function LyricsBody({
  extras,
  format,
}: {
  extras: Extras | null;
  format: "text" | "lrc";
}) {
  const lyrics = extras?.lyrics;

  if (!lyrics || lyrics.instrumental || (!lyrics.plain && !lyrics.synced)) {
    return (
      <div className="border px-7 py-6.5 text-[15px] text-muted-foreground">
        <p>
          {lyrics?.instrumental
            ? "Instrumental track — no lyrics to show."
            : "Lyrics not available yet."}
        </p>
        <p className="mb-0">
          ISRC:{" "}
          <span className="font-mono text-sm text-foreground">
            {extras?.isrc || "unavailable"}
          </span>
        </p>
      </div>
    );
  }

  return (
    <>
      {format === "lrc" && lyrics.synced ? (
        <pre className="m-0 font-mono text-[13px] leading-[1.7] whitespace-pre-wrap text-foreground/80">
          {lyrics.synced}
        </pre>
      ) : (
        <div>
          {lyricsLines(lyrics.plain || stripTimestamps(lyrics.synced || ""))}
        </div>
      )}
      <p className="mt-8.5 text-[13px] text-muted-foreground">
        Lyrics from {lyrics.source}.
      </p>
    </>
  );
}

function TrackView({
  track,
  extras,
}: {
  track: TrackDetail;
  extras: Extras | null;
}) {
  const [format, setFormat] = useState<"text" | "lrc">("text");
  const missing = lyricsMissing(extras);

  const formats = [
    {
      key: "lrc" as const,
      label: "LRC",
      available: Boolean(extras?.lyrics?.synced),
    },
    { key: "text" as const, label: "Text", available: true },
  ];

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

        <TrackFacts track={track} extras={extras} />

        {!missing ? (
          <div className="mt-6.5 flex gap-5">
            {formats.map((entry) => (
              <button
                key={entry.key}
                type="button"
                className={cn(
                  "border-b-2 pb-1.5 text-sm transition-colors",
                  entry.key === format
                    ? "border-primary font-bold text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground",
                  !entry.available &&
                    "cursor-not-allowed text-muted-foreground/40 hover:text-muted-foreground/40",
                )}
                disabled={!entry.available}
                onClick={() => setFormat(entry.key)}
              >
                {entry.label}
              </button>
            ))}
          </div>
        ) : null}

        <SpotifyLink uri={track.uri} />
      </aside>

      <div className="min-h-50">
        <LyricsBody extras={extras} format={format} />
      </div>
    </section>
  );
}
