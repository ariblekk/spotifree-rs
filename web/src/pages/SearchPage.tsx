import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { SearchResults } from "@/lib/api";
import { apiUrl, duration, spotifyUrl } from "@/lib/api";
import { cn } from "@/lib/utils";

type TabKey = "tracks" | "artists" | "albums" | "playlists";

/// The four search sections share these fields; everything else is optional so
/// one table renderer can cover all of them.
interface SearchItem {
  id: string;
  uri: string;
  name: string;
  image: string | null;
  artists?: string[];
  album?: string | null;
  duration_ms?: number | null;
  owner?: string | null;
}

interface Column {
  label: string;
  cls?: string;
  media?: boolean;
  round?: boolean;
  value: (item: SearchItem) => string;
}

interface Tab {
  key: TabKey;
  label: string;
  detail?: "track" | "artist" | "album" | "playlist";
  columns: Column[];
}

const TABS: Tab[] = [
  {
    key: "tracks",
    label: "Tracks",
    detail: "track",
    columns: [
      {
        label: "Title",
        cls: "w-[42%] max-[960px]:w-[60%] max-[640px]:w-full",
        media: true,
        value: (item) => item.name,
      },
      {
        label: "Artist",
        cls: "w-[24%] max-[960px]:w-[40%] max-[640px]:hidden",
        value: (item) => (item.artists || []).join(", "),
      },
      {
        label: "Album",
        cls: "w-[26%] max-[960px]:hidden",
        value: (item) => item.album || "",
      },
      {
        label: "Time",
        cls: "w-[8%] text-right whitespace-nowrap text-muted-foreground max-[960px]:hidden",
        value: (item) => duration(item.duration_ms),
      },
    ],
  },
  {
    key: "artists",
    label: "Artists",
    detail: "artist",
    columns: [
      {
        label: "Artist",
        cls: "w-[42%] max-[640px]:w-full",
        media: true,
        round: true,
        value: (item) => item.name,
      },
      {
        label: "URI",
        cls: "w-[58%] max-[640px]:hidden",
        value: (item) => item.uri,
      },
    ],
  },
  {
    key: "albums",
    label: "Albums",
    detail: "album",
    columns: [
      {
        label: "Album",
        cls: "w-[42%] max-[640px]:w-full",
        media: true,
        value: (item) => item.name,
      },
      {
        label: "Artist",
        cls: "w-[30%] max-[640px]:hidden",
        value: (item) => (item.artists || []).join(", "),
      },
      {
        label: "URI",
        cls: "w-[28%] max-[640px]:hidden",
        value: (item) => item.uri,
      },
    ],
  },
  {
    key: "playlists",
    label: "Playlists",
    detail: "playlist",
    columns: [
      {
        label: "Name",
        cls: "w-[42%] max-[640px]:w-full",
        media: true,
        value: (item) => item.name,
      },
      {
        label: "Owner",
        cls: "w-[30%] max-[640px]:hidden",
        value: (item) => item.owner || "",
      },
      {
        label: "URI",
        cls: "w-[28%] max-[640px]:hidden",
        value: (item) => item.uri,
      },
    ],
  },
];

const IDLE = "anonymous web-player backend";

export default function SearchPage() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const [query, setQuery] = useState(() => params.get("q") ?? "");
  const [payload, setPayload] = useState<SearchResults | null>(null);
  const [status, setStatus] = useState(IDLE);
  const [active, setActive] = useState<TabKey>("tracks");

  const controller = useRef<AbortController | null>(null);
  const timer = useRef<number | null>(null);

  const runSearch = useCallback(async (term: string) => {
    controller.current?.abort();
    const current = new AbortController();
    controller.current = current;

    setStatus(`Searching for “${term}”…`);

    try {
      const response = await fetch(apiUrl(`/api/search?q=${encodeURIComponent(term)}`), {
        signal: current.signal,
      });
      const body = (await response.json()) as SearchResults & {
        error?: string;
      };

      if (!response.ok)
        throw new Error(body.error || `HTTP ${response.status}`);

      setPayload(body);
      setActive("tracks");

      const total = TABS.reduce(
        (sum, tab) => sum + (body[tab.key] || []).length,
        0,
      );
      setStatus(
        `${total} result${total === 1 ? "" : "s"} for “${body.query || term}”`,
      );
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return;

      setPayload(null);
      const hint =
        location.protocol === "file:"
          ? "Open this page through the server (cargo run --bin serve) instead of the file system."
          : error instanceof Error
            ? error.message
            : String(error);
      setStatus(`Search failed — ${hint}`);
    }
  }, []);

  useEffect(() => {
    if (timer.current) window.clearTimeout(timer.current);

    const term = query.trim();

    // Keep the term in the URL so Back from a detail page restores the search.
    setParams(term ? { q: term } : {}, { replace: true });

    if (!term) {
      controller.current?.abort();
      setPayload(null);
      setActive("tracks");
      setStatus(IDLE);
      return;
    }

    timer.current = window.setTimeout(() => void runSearch(term), 250);

    return () => {
      if (timer.current) window.clearTimeout(timer.current);
    };
  }, [query, runSearch, setParams]);

  const tab = TABS.find((entry) => entry.key === active) ?? TABS[0];
  const items: SearchItem[] = payload ? payload[tab.key] || [] : [];

  function openRow(item: SearchItem) {
    const url = spotifyUrl(item.uri);

    if (tab.detail && item.id) {
      navigate(`/detail?type=${tab.detail}&id=${encodeURIComponent(item.id)}`);
    } else if (url) {
      window.open(url, "_blank", "noopener");
    }
  }

  return (
    <main className="mx-auto max-w-[1400px] px-6 pb-20 max-sm:px-4 max-sm:pb-15">
      <p className="mt-5 text-[15px] text-muted-foreground">{status}</p>

      <form
        autoComplete="off"
        onSubmit={(event) => {
          event.preventDefault();
          if (timer.current) window.clearTimeout(timer.current);
          const term = query.trim();
          if (term) void runSearch(term);
        }}
      >
        <div className="mt-1.5 border-b-2 border-primary">
          <input
            id="q"
            type="text"
            name="q"
            placeholder="Search"
            aria-label="Search"
            autoComplete="off"
            spellCheck={false}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            required
            className="w-full border-0 bg-transparent px-1.5 pb-3.5 text-[clamp(2.75rem,7vw,7rem)] font-bold leading-[1.1] tracking-[-0.02em] text-foreground caret-primary outline-none placeholder:text-muted-foreground/40"
          />
        </div>
      </form>

      <div
        className="mt-3.5 flex gap-x-[26px] overflow-x-auto pb-1 whitespace-nowrap max-sm:gap-x-4 max-[400px]:gap-x-3"
        role="tablist"
      >
        {TABS.map((entry) => {
          const count = payload ? (payload[entry.key] || []).length : null;
          return (
            <button
              key={entry.key}
              type="button"
              role="tab"
              aria-selected={entry.key === active}
              className={cn(
                "pb-2.5 text-base transition-colors max-[400px]:text-sm",
                entry.key === active
                  ? "font-bold text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
              onClick={() => setActive(entry.key)}
            >
              {entry.label}
              {count != null ? (
                <span className="ml-1 font-normal text-muted-foreground/60">
                  {count}
                </span>
              ) : null}
            </button>
          );
        })}
      </div>

      <div className="mt-6.5">
        {payload && items.length === 0 ? (
          <p className="px-2 py-5.5 text-[15px] text-muted-foreground">
            No {tab.label.toLowerCase()} for “{payload.query}”.
          </p>
        ) : null}

        {items.length > 0 ? (
          <Table className="table-fixed">
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                {tab.columns.map((column) => (
                  <TableHead
                    key={column.label}
                    className={cn(
                      "h-auto px-2 py-2.5 text-sm font-normal text-muted-foreground",
                      column.cls,
                    )}
                  >
                    {column.label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {items.map((item) => (
                <TableRow
                  key={`${item.id}-${item.name}`}
                  className="cursor-pointer"
                  tabIndex={0}
                  role="link"
                  title={`Open ${tab.label.toLowerCase()}`}
                  onClick={() => openRow(item)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") openRow(item);
                  }}
                >
                  {tab.columns.map((column) => (
                    <TableCell
                      key={column.label}
                      className={cn(
                        // The table is `table-fixed`, so a long value would
                        // otherwise spill over the next column instead of
                        // ellipsizing: `truncate` needs `overflow-hidden` on the
                        // cell itself, and `max-w-0` lets the fixed width win.
                        "max-w-0 overflow-hidden px-2 py-3",
                        column.cls,
                      )}
                    >
                      {column.media ? (
                        <div className="flex min-w-0 items-center gap-3.5">
                          <img
                            className={cn(
                              "size-11 flex-none bg-secondary object-cover",
                              column.round && "rounded-full",
                            )}
                            alt=""
                            loading="lazy"
                            src={item.image || undefined}
                          />
                          <span
                            className="truncate font-bold text-foreground"
                            title={column.value(item)}
                          >
                            {column.value(item) || "(untitled)"}
                          </span>
                        </div>
                      ) : (
                        <span className="block truncate" title={column.value(item)}>
                          {column.value(item)}
                        </span>
                      )}
                    </TableCell>
                  ))}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        ) : null}
      </div>
    </main>
  );
}
