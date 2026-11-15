import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { useNavigate, useSearchParams, Link } from "react-router-dom";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { Chart, Charts } from "@/lib/api";
import {
  charts as fetchCharts,
  detailHref,
  duration,
  formatNumber,
  openOnSpotify,
  spotifyUrl,
} from "@/lib/api";
import { cn } from "@/lib/utils";

/// Only two markets are served, and both are picked from a segmented control.
const MARKETS = [
  { code: "id", label: "Indonesia" },
  { code: "global", label: "Global" },
] as const;

type Market = (typeof MARKETS)[number]["code"];

/// `daily` is Spotify's "Top 50", `alltime` its "Top Songs" chart.
const TOPS = [
  { code: "daily", label: "Daily" },
  { code: "alltime", label: "All-time" },
] as const;

type Top = (typeof TOPS)[number]["code"];

const ROWS = 50;

const MOVERS = 20;

const SECTION_TITLE =
  "mt-11 mb-3.5 border-b-2 border-primary pb-2.5 text-[22px] font-bold tracking-[-0.01em]";

const SELECT =
  "min-w-36 border bg-background px-3 py-2 text-[15px] text-foreground outline-none focus-visible:border-primary";

/// The API can answer with fewer keys (a market with no shelves, or a cached
/// `null`), so every list is normalized before it reaches the page.
function normalizeCharts(body: Charts | null, country: string): Charts {
  return {
    country: body?.country || country,
    color: body?.color ?? null,
    charts: (body?.charts ?? []).filter(
      (chart): chart is Chart => Boolean(chart && Array.isArray(chart.entries)),
    ),
    trending: body?.trending ?? [],
    popular_albums: body?.popular_albums ?? [],
    popular_artists: body?.popular_artists ?? [],
  };
}

type View =
  | { status: "loading" }
  | { status: "error"; text: string }
  | { status: "ok"; charts: Charts };

function visibleEntries(chart: Chart | null, limit: number) {
  return (chart?.entries ?? []).slice(0, limit);
}

/// `position - previous` > 0 means the track climbed.
function movement(entry: { position: number; previous: number | null }) {
  if (entry.previous == null) return null;
  return entry.previous - entry.position;
}

function Movement({ entry }: { entry: { position: number; previous: number | null } }) {
  const delta = movement(entry);

  if (delta == null) {
    return <span className="text-[13px] font-semibold text-primary">NEW</span>;
  }
  if (delta === 0) return <span className="text-[13px] text-muted-foreground">—</span>;

  const up = delta > 0;
  return (
    <span
      className={cn(
        "text-[13px] font-semibold tabular-nums",
        up ? "text-primary" : "text-muted-foreground",
      )}
    >
      {up ? "▲" : "▼"} {Math.abs(delta)}
    </span>
  );
}

function ChartTable({ chart, limit }: { chart: Chart; limit: number }) {
  const navigate = useNavigate();
  const rows = visibleEntries(chart, limit);

  if (!rows.length) {
    return (
      <p className="py-6 text-[15px] text-muted-foreground">
        This chart has no rows right now.
      </p>
    );
  }

  return (
    <Table className="table-fixed">
      <TableHeader>
        <TableRow className="hover:bg-transparent">
          <TableHead className="h-auto w-10 px-2 py-2.5 text-right text-sm font-normal text-muted-foreground">
            #
          </TableHead>
          <TableHead className="h-auto px-2 py-2.5 text-sm font-normal text-muted-foreground">
            Title
          </TableHead>
          <TableHead className="h-auto w-16 px-2 py-2.5 text-right text-sm font-normal text-muted-foreground max-[700px]:hidden">
            Move
          </TableHead>
          <TableHead className="h-auto w-24 px-2 py-2.5 text-right text-sm font-normal text-muted-foreground max-[960px]:hidden">
            Plays
          </TableHead>
          <TableHead className="h-auto w-16 px-2 py-2.5 text-right text-sm font-normal text-muted-foreground max-[640px]:hidden">
            Time
          </TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((entry) => {
          const url = spotifyUrl(entry.uri);
          const open = () => {
            if (entry.id) {
              navigate(detailHref("track", entry.id));
            } else {
              openOnSpotify(entry.uri);
            }
          };

          return (
            <TableRow
              key={`${entry.uri}-${entry.position}`}
              className="cursor-pointer"
              tabIndex={0}
              role="link"
              title={url ? "Open track" : undefined}
              onClick={open}
              onKeyDown={(event) => {
                if (event.key === "Enter") open();
              }}
            >
              <TableCell className="px-2 py-3 text-right text-[15px] font-bold tabular-nums text-muted-foreground">
                {entry.position}
              </TableCell>
              <TableCell className="max-w-0 overflow-hidden px-2 py-3 align-top whitespace-normal">
                <div className="flex min-w-0 items-center gap-3">
                  <img
                    className="size-10 flex-none bg-secondary object-cover"
                    alt=""
                    loading="lazy"
                    src={entry.image || undefined}
                  />
                  <div className="min-w-0 flex-1">
                    <div
                      className="truncate font-bold text-foreground"
                      title={entry.name}
                    >
                      {entry.name || "(untitled)"}
                    </div>
                    {entry.artists.length ? (
                      <div
                        className="truncate text-sm text-muted-foreground"
                        title={entry.artists.join(", ")}
                      >
                        {entry.artists.join(", ")}
                      </div>
                    ) : null}
                  </div>
                </div>
              </TableCell>
              <TableCell className="px-2 py-3 text-right max-[700px]:hidden">
                <Movement entry={entry} />
              </TableCell>
              <TableCell className="px-2 py-3 text-right text-sm tabular-nums text-muted-foreground max-[960px]:hidden">
                {entry.plays ? formatNumber(entry.plays) : ""}
              </TableCell>
              <TableCell className="px-2 py-3 text-right text-sm text-muted-foreground max-[640px]:hidden">
                {duration(entry.duration_ms)}
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}

function Tile({
  to,
  image,
  title,
  subtitle,
  round,
  onClick,
}: {
  to?: string | null;
  image: string | null;
  title: string;
  subtitle?: string | null;
  round?: boolean;
  onClick?: () => void;
}) {
  const body = (
    <>
      <img
        className={cn(
          "mb-2 aspect-square w-full bg-secondary object-cover",
          round && "rounded-full",
        )}
        alt=""
        loading="lazy"
        src={image || undefined}
      />
      <div className="truncate text-[14px] font-semibold text-foreground group-hover:underline">
        {title}
      </div>
      {subtitle ? (
        <div className="truncate text-[13px] text-muted-foreground">{subtitle}</div>
      ) : null}
    </>
  );

  const className = "group block min-w-0 cursor-pointer no-underline hover:no-underline";

  return to ? (
    <Link to={to} className={className} onClick={onClick}>
      {body}
    </Link>
  ) : (
    <button type="button" className={cn(className, "text-left")} onClick={onClick}>
      {body}
    </button>
  );
}

function TileGrid({ children }: { children: ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-x-4.5 gap-y-6 max-[400px]:grid-cols-[repeat(auto-fill,minmax(104px,1fr))] max-[400px]:gap-x-3 max-[400px]:gap-y-4">
      {children}
    </div>
  );
}

export default function ChartsPage() {
  const [params, setParams] = useSearchParams();

  const marketParam = (params.get("country") || "id").toLowerCase();
  const market: Market = MARKETS.some((entry) => entry.code === marketParam)
    ? (marketParam as Market)
    : "id";

  const topParam = (params.get("top") || "daily").toLowerCase();
  const top: Top = TOPS.some((entry) => entry.code === topParam)
    ? (topParam as Top)
    : "daily";

  const label = MARKETS.find((entry) => entry.code === market)?.label ?? "Indonesia";
  const [view, setView] = useState<View>({ status: "loading" });

  useEffect(() => {
    const controller = new AbortController();
    setView({ status: "loading" });

    (async () => {
      try {
        const body = await fetchCharts(market, top, ROWS, controller.signal);
        if (controller.signal.aborted) return;
        setView({ status: "ok", charts: normalizeCharts(body, market) });
      } catch (error) {
        if (controller.signal.aborted) return;

        const hint = error instanceof Error ? error.message : String(error);
        setView({ status: "error", text: `Charts failed — ${hint}` });
      }
    })();

    return () => controller.abort();
  }, [market, top]);

  const chart = view.status === "ok" ? (view.charts.charts[0] ?? null) : null;

  /// The rest of the shelf, minus the chart already shown above.
  const others =
    view.status === "ok"
      ? view.charts.charts.filter((entry) => entry.id !== chart?.id)
      : [];

  /// Biggest climbs and drops inside the fetched chart.
  const movers = useMemo(() => {
    const rows = visibleEntries(chart, MOVERS * 2).flatMap((entry) => {
      const delta = movement(entry);
      return delta != null && delta !== 0 ? [{ entry, delta }] : [];
    });

    const climbers = [...rows].sort((a, b) => b.delta - a.delta).slice(0, 5);
    const drops = [...rows].sort((a, b) => a.delta - b.delta).slice(0, 5);
    return { climbers, drops };
  }, [chart]);

  const pick = (next: { country?: Market; top?: Top }) =>
    setParams(
      {
        country: next.country ?? market,
        top: next.top ?? top,
      },
      { replace: true },
    );

  return (
    <main className="mx-auto max-w-[1400px] px-6 pt-9 pb-22 max-sm:px-4 max-sm:pt-6 max-sm:pb-15">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <p className="mb-1.5 text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
            Charts
          </p>
          <h1 className="mb-1 text-[clamp(2rem,4.5vw,3rem)] leading-[1.05] font-bold tracking-[-0.03em]">
            {label}
          </h1>
          {chart?.updated_at ? (
            <p className="m-0 text-[15px] text-muted-foreground">
              Updated {new Date(chart.updated_at).toLocaleString()}
            </p>
          ) : null}
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
              Region
            </span>
            <select
              value={market}
              onChange={(event) =>
                pick({ country: event.target.value as Market })
              }
              className={SELECT}
            >
              {MARKETS.map((entry) => (
                <option key={entry.code} value={entry.code}>
                  {entry.label}
                </option>
              ))}
            </select>
          </label>

          <label className="flex flex-col gap-1.5">
            <span className="text-[13px] tracking-[0.08em] text-muted-foreground uppercase">
              Chart
            </span>
            <select
              value={top}
              onChange={(event) => pick({ top: event.target.value as Top })}
              className={SELECT}
            >
              {TOPS.map((entry) => (
                <option key={entry.code} value={entry.code}>
                  {entry.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </div>

      {view.status === "loading" ? (
        <p className="py-8 text-[15px] text-muted-foreground">Loading charts…</p>
      ) : view.status === "error" ? (
        <p className="py-8 text-[15px] text-destructive">{view.text}</p>
      ) : (
        <>
          {chart ? (
            <>
              <h2 className={SECTION_TITLE}>
                {label}
                <span className="ml-2 align-middle text-[13px] font-normal tracking-normal text-muted-foreground uppercase">
                  {top === "daily" ? "Daily · Top 50" : "All-time · Top 50"}
                </span>
              </h2>
              <ChartTable chart={chart} limit={ROWS} />
            </>
          ) : (
            <p className="py-8 text-[15px] text-muted-foreground">
              No {top === "daily" ? "daily" : "all-time"} chart is published for{" "}
              {label}.
            </p>
          )}

          {others.length ? (
            <>
              <h2 className={SECTION_TITLE}>More charts</h2>
              <TileGrid>
                {others.map((entry) => (
                  <Tile
                    key={entry.id}
                    to={detailHref("playlist", entry.id)}
                    image={entry.image}
                    title={entry.name}
                    subtitle={entry.frequency}
                  />
                ))}
              </TileGrid>
            </>
          ) : null}

          {movers.climbers.length || movers.drops.length ? (
            <>
              <h2 className={SECTION_TITLE}>Movers</h2>
              <div className="grid gap-x-10 gap-y-6 sm:grid-cols-2">
                {(
                  [
                    ["Biggest climbs", movers.climbers],
                    ["Biggest drops", movers.drops],
                  ] as const
                ).map(([title, rows]) => (
                  <div key={title} className="min-w-0">
                    <h3 className="mb-2 text-[15px] font-bold">{title}</h3>
                    <ol className="m-0 list-none p-0">
                      {rows.map(({ entry, delta }) => (
                        <li key={`${entry.uri}-${entry.position}`}>
                          <Link
                            to={detailHref("track", entry.id)}
                            className="flex items-center gap-3 border-b py-2 no-underline last:border-b-0 hover:no-underline"
                          >
                            <span className="w-6 flex-none text-right text-[13px] tabular-nums text-muted-foreground">
                              {entry.position}
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="block truncate text-[15px] font-semibold text-foreground">
                                {entry.name}
                              </span>
                              <span className="block truncate text-[13px] text-muted-foreground">
                                {entry.artists.join(", ")}
                              </span>
                            </span>
                            <span
                              className={cn(
                                "flex-none text-[13px] font-semibold tabular-nums",
                                delta > 0 ? "text-primary" : "text-muted-foreground",
                              )}
                            >
                              {delta > 0 ? "▲" : "▼"} {Math.abs(delta)}
                            </span>
                          </Link>
                        </li>
                      ))}
                    </ol>
                  </div>
                ))}
              </div>
            </>
          ) : null}

          {view.charts.trending.length ? (
            <>
              <h2 className={SECTION_TITLE}>Trending now</h2>
              <TileGrid>
                {view.charts.trending.map((track) => (
                  <Tile
                    key={track.uri}
                    to={detailHref("track", track.id)}
                    image={track.image}
                    title={track.name}
                    subtitle={track.artists.join(", ")}
                  />
                ))}
              </TileGrid>
            </>
          ) : null}

          {view.charts.popular_albums.length ? (
            <>
              <h2 className={SECTION_TITLE}>Popular albums</h2>
              <TileGrid>
                {view.charts.popular_albums.map((album) => (
                  <Tile
                    key={album.uri}
                    to={detailHref("album", album.id)}
                    image={album.image}
                    title={album.name}
                    subtitle={album.artists.join(", ")}
                  />
                ))}
              </TileGrid>
            </>
          ) : null}

          {view.charts.popular_artists.length ? (
            <>
              <h2 className={SECTION_TITLE}>Popular artists</h2>
              <TileGrid>
                {view.charts.popular_artists.map((artist) => (
                  <Tile
                    key={artist.uri}
                    to={detailHref("artist", artist.id)}
                    image={artist.image}
                    title={artist.name}
                    round
                  />
                ))}
              </TileGrid>
            </>
          ) : null}
        </>
      )}
    </main>
  );
}
