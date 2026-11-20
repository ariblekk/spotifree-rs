import type { ReactNode } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  CodeBlock,
  CodeBlockActions,
  CodeBlockContent,
  CodeBlockCopyButton,
} from "@/components/ui/code-block";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { API_BASE } from "@/lib/api";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";

/// Docs show the public deployment unless the build is pointed elsewhere, so a
/// self-hosted copy documents its own API instead of this one.
const BASE = API_BASE || "http://localhost:8080";

const CURL = `# Search
curl "${BASE}/v1/search?q=lofi&limit=5"

# Only tracks and albums
curl "${BASE}/v1/search?q=lofi&type=track,album"

# Details (Spotify id from any list above)
curl "${BASE}/v1/album/4aawyAB9vmqN3uQ7FjRGTy"
curl "${BASE}/v1/track/23NgAzXBCwZdtwBn5hQzD5"`;

const JS_FETCH = `const BASE = "${BASE}";

const search = await fetch(\`\${BASE}/v1/search?q=lofi&limit=5\`);
if (!search.ok) throw new Error((await search.json()).error);
const data = await search.json();

console.log(data.tracks[0].name, data.tracks[0].artists.join(", "));

// Detail endpoints return the resource as-is.
const album = await (await fetch(\`\${BASE}/v1/album/\${data.albums[0].id}\`)).json();
console.log(album.albumUnion.name, album.albumUnion.tracksV2.items.length);

// Charts: market is id or global; top is daily or alltime.
const charts = await (await fetch(\`\${BASE}/v1/charts/id?top=daily\`)).json();
for (const artist of charts.popular_artists) {
  console.log(artist.name, artist.id);
}`;

const PYTHON = `import requests

BASE = "${BASE}"

r = requests.get(f"{BASE}/v1/search", params={"q": "lofi", "limit": 5}, timeout=15)
r.raise_for_status()
data = r.json()

for track in data["tracks"]:
    print(track["name"], "-", ", ".join(track["artists"]))

# Charts: market is id or global, optional top=daily|alltime
charts = requests.get(f"{BASE}/v1/charts/id", params={"top": "daily"}, timeout=20).json()
for row in charts["charts"][0]["entries"]:
    print(row["position"], row["name"], row["plays"])

for artist in charts["popular_artists"]:
    print(artist["name"], artist["id"])`;

const CHARTS = `curl "${BASE}/v1/charts/id"
curl "${BASE}/v1/charts/id?top=daily"
curl "${BASE}/v1/charts/id?top=alltime"
curl "${BASE}/v1/charts/global?top=daily"`;

const CHARTS_JSON = `{
  "country": "id",
  "color": "#F037A5",
  "charts": [
    {
      "id": "37i9dQZEVXbObFQZ3JLcXt",
      "name": "Top 50 - Indonesia",
      "frequency": "daily",
      "updated_at": "2026-11-14T14:01:05Z",
      "entries": [
        {
          "position": 1,
          "previous": 3,
          "plays": 380572470,
          "name": "Sesi Potret",
          "id": "4xoY4lZNoTjEuHsSmhgF1G",
          "uri": "spotify:track:4xoY4lZNoTjEuHsSmhgF1G",
          "artists": ["e\u0144au", "Ari Lesmana"],
          "album": "Sesi Potret",
          "image": "https://i.scdn.co/image/\u2026"
        }
      ]
    }
  ],
  "trending": [
    {
      "id": "5d2Sj3lMKJFefwU26GInQu",
      "uri": "spotify:track:5d2Sj3lMKJFefwU26GInQu",
      "name": "Party With The Boys",
      "artists": ["Don Toliver"],
      "album": "Party With The Boys",
      "duration_ms": null,
      "image": "https://i.scdn.co/image/\u2026"
    }
  ],
  "popular_albums": [
    {
      "id": "3WZZF72ihlKPZBS4zSsNHl",
      "uri": "spotify:album:3WZZF72ihlKPZBS4zSsNHl",
      "name": "you seem pretty sad for a girl so in love",
      "artists": ["Olivia Rodrigo"],
      "image": "https://i.scdn.co/image/\u2026"
    }
  ],
  "popular_artists": [
    {
      "id": "3TVXtAsR1Inumwj472S9r4",
      "uri": "spotify:artist:3TVXtAsR1Inumwj472S9r4",
      "name": "Drake",
      "image": "https://i.scdn.co/image/\u2026"
    }
  ]
}`;

const PLAYBACK = `const res = await fetch(\`${BASE}/v1/track/\${id}\`);
const { trackUnion } = await res.json();

// Cover art, largest source first
const cover = [...(trackUnion.albumOfTrack.coverArt.sources ?? [])]
  .sort((a, b) => b.width - a.width)[0]?.url;

console.log(trackUnion.name, cover);`;

const SEARCH_JSON = `{
  "query": "lofi",
  "tracks": [
    {
      "id": "23NgAzXBCwZdtwBn5hQzD5",
      "uri": "spotify:track:23NgAzXBCwZdtwBn5hQzD5",
      "name": "Mood - Lofi",
      "artists": ["Yagih Mael"],
      "album": "…",
      "duration_ms": 123000,
      "image": "https://i.scdn.co/image/…"
    }
  ],
  "artists": [],
  "albums": [],
  "playlists": []
}`;

const ERROR_BODY = '{"error":"…"}';

const ME_CURL = `# Pass your browser's sp_dc cookie (from open.spotify.com)
curl -H "X-Sp-Dc: <your_sp_dc_cookie>" "${BASE}/v1/me"
curl -H "X-Sp-Dc: <your_sp_dc_cookie>" "${BASE}/v1/me/playlists?limit=20"
curl -H "X-Sp-Dc: <your_sp_dc_cookie>" "${BASE}/v1/me/tracks?limit=20"
curl -H "X-Sp-Dc: <your_sp_dc_cookie>" "${BASE}/v1/me/top?range=short"

# Or pass a Spotify bearer access_token (BQ…) or sp_dc in Authorization
curl -H "Authorization: Bearer <token_or_sp_dc>" "${BASE}/v1/me"`;

const ME_JSON = `{
  "id": "ariblekk",
  "uri": "spotify:user:ariblekk",
  "username": "ariblekk",
  "name": "Ari Haryanto",
  "image": "https://i.scdn.co/image/…",
  "country": "ID",
  "product": "premium",
  "explicit_filter": false
}`;

const SECTION_TITLE =
  "mt-14 mb-1.5 border-b-2 border-primary pb-2.5 text-[22px] font-bold tracking-[-0.01em]";

function Section({
  title,
  lead,
  first,
  children,
}: {
  title: string;
  lead?: ReactNode;
  first?: boolean;
  children?: ReactNode;
}) {
  return (
    <>
      <h2 className={cn(SECTION_TITLE, first && "mt-0")}>{title}</h2>
      {lead ? (
        <p className="mb-4.5 text-[15px] text-muted-foreground">{lead}</p>
      ) : null}
      {children}
    </>
  );
}

function Endpoint({ path, children }: { path: string; children?: ReactNode }) {
  return (
    <div className="mb-3.5 border">
      <div className="flex flex-wrap items-center gap-3 border-b px-4 py-3 font-mono text-[13px]">
        <span className="bg-primary px-1.75 py-0.75 text-[11px] font-bold tracking-[0.06em] text-primary-foreground">
          GET
        </span>
        <span className="break-all">{path}</span>
      </div>
      <div className="px-4 py-3.5 text-[15px] text-foreground/80 [&>:first-child]:mt-0 [&>:last-child]:mb-0">
        {children}
      </div>
    </div>
  );
}

function Code({ children }: { children: string }) {
  return (
    <CodeBlock className="my-3 mb-5">
      <CodeBlockActions>
        <CodeBlockCopyButton value={children} />
      </CodeBlockActions>
      <CodeBlockContent>{children}</CodeBlockContent>
    </CodeBlock>
  );
}

function Th({ children }: { children: ReactNode }) {
  return (
    <TableHead className="h-auto px-2 py-2 text-[13px] font-normal tracking-[0.05em] whitespace-nowrap text-muted-foreground uppercase">
      {children}
    </TableHead>
  );
}

function Td({ children }: { children: ReactNode }) {
  return (
    <TableCell className="px-2 py-2.5 align-top text-[15px] whitespace-normal text-foreground/80">
      {children}
    </TableCell>
  );
}

function TdMono({ children }: { children: ReactNode }) {
  return (
    <TableCell className="px-2 py-2.5 align-top font-mono text-[13px] whitespace-nowrap text-foreground">
      {children}
    </TableCell>
  );
}

function Note({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Alert className="mb-4.5 border-l-2 border-l-primary bg-muted/40">
      <AlertTitle className="text-[15px] font-bold text-foreground">
        {title}
      </AlertTitle>
      <AlertDescription className="text-[15px] text-foreground/80">
        {children}
      </AlertDescription>
    </Alert>
  );
}

export default function DocsPage() {
  return (
    <main className="mx-auto max-w-[1400px] px-6 pt-14 pb-22 max-sm:px-4 max-sm:pt-9 max-sm:pb-15">
      <h1 className="mb-2.5 text-[42px] leading-[1.1] font-bold tracking-[-0.03em] max-sm:text-[clamp(1.6rem,7vw,2.4rem)]">
        API
      </h1>
      <p className="mb-11 max-w-[62ch] text-[17px] text-muted-foreground">
        JSON over HTTP, no key, no login. Base URL{" "}
        <code className="text-foreground">{BASE}</code>. It reads Spotify's
        public catalogue and holds no user data, so there is nothing to protect:
        an API key would ship inside the JavaScript bundle where anyone can read
        it. The server keeps the Spotify session, so your app only sends a
        track, album, artist or playlist id.
      </p>

      <Section title="Endpoints" first>
        <Endpoint path="/v1/search?q=lofi&limit=20">
          Plain search results as typed JSON.
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <Th>Query param</Th>
                <Th>Type</Th>
                <Th>Notes</Th>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow>
                <TdMono>q</TdMono>
                <Td>string</Td>
                <Td>Required, max 120 chars</Td>
              </TableRow>
              <TableRow>
                <TdMono>limit</TdMono>
                <Td>integer</Td>
                <Td>Per category, 1–50 (default 10)</Td>
              </TableRow>
              <TableRow>
                <TdMono>offset</TdMono>
                <Td>integer</Td>
                <Td>Pagination, 0–10000 (default 0)</Td>
              </TableRow>
              <TableRow>
                <TdMono>type</TdMono>
                <Td>string</Td>
                <Td>
                  Keep only some sections: <code>track</code>,{" "}
                  <code>artist</code>, <code>album</code>, <code>playlist</code>
                  . Omit for all
                </Td>
              </TableRow>
            </TableBody>
          </Table>
        </Endpoint>

        <Endpoint path="/v1/search/suggest?q={query}">
          Instant search suggestions (top hits and autocomplete query chips) as
          the user types.
        </Endpoint>

        <Endpoint path="/v1/charts/{country}">
          Charts for <code>id</code> or <code>global</code>: the ranked chart
          (50 rows), trending songs, popular albums and artists. See{" "}
          <a
            href="/charts"
            className="text-primary underline-offset-2 hover:underline"
          >
            Charts
          </a>
          .
        </Endpoint>

        <Endpoint path="/v1/track/{id}">
          One track: name, artists, album, duration, cover.
        </Endpoint>

        <Endpoint path="/v1/track/{id}/credits">
          Track credits: songwriters, producers, performers, publishers, and record
          labels.
        </Endpoint>

        <Endpoint path="/v1/track/{id}/recommendations">
          Seeded recommended tracks related to this track (also available at{" "}
          <code>/v1/track/{"{id}"}/related</code>).
        </Endpoint>

        <Endpoint path="/v1/album/{id}">
          Album with its <strong>complete</strong> track list — long albums are
          paged server-side, not cut off.
        </Endpoint>

        <Endpoint path="/v1/artist/{id}">
          Artist profile, top tracks, albums and singles.
        </Endpoint>

        <Endpoint path="/v1/artist/{id}/discography?type={all|album|single|compilation}&offset={n}&limit={n}">
          Artist discography catalog releases with pagination.
        </Endpoint>

        <Endpoint path="/v1/playlist/{id}">
          Playlist with its items (up to 1000, paged server-side).
        </Endpoint>

        <Endpoint path="/v1/browse">
          Browse / explore categories and genres (title, card color, icon image, ID).
        </Endpoint>

        <Endpoint path="/v1/genre/{id}">
          Curated genre page with playlists, new releases, and subcategories.
        </Endpoint>

        <Endpoint path="/v1/me">
          Authenticated user profile and account attributes (<code>id</code>,{" "}
          <code>username</code>, <code>name</code>, <code>image</code>,{" "}
          <code>country</code>, <code>product</code>,{" "}
          <code>explicit_filter</code>). Pass a Spotify bearer{" "}
          <code>access_token</code> (<code>BQ…</code>) or browser{" "}
          <code>sp_dc</code> cookie via{" "}
          <code>Authorization: Bearer &lt;token|sp_dc&gt;</code>,{" "}
          <code>X-Sp-Dc: &lt;sp_dc&gt;</code>, or{" "}
          <code>Cookie: sp_dc=&lt;cookie&gt;</code>.
        </Endpoint>

        <Endpoint path="/v1/me/playlists?limit={1-50}&offset={n}">
          Playlists in the authenticated user's library (requires user token or{" "}
          <code>sp_dc</code>).
        </Endpoint>

        <Endpoint path="/v1/me/tracks?limit={1-50}&offset={n}">
          Authenticated user's Liked Songs with <code>added_at</code> timestamps
          (requires user token or <code>sp_dc</code>).
        </Endpoint>

        <Endpoint path="/v1/me/top?range={short|medium|long}&type={tracks|artists}&limit={1-50}">
          Authenticated user's top tracks and/or top artists by affinity (also{" "}
          <code>/v1/me/top/tracks</code> and <code>/v1/me/top/artists</code>).
        </Endpoint>

        <Endpoint path="/healthz">
          Uptime probe plus cache and session stats. Returns <code>503</code>{" "}
          while the server is still starting.
        </Endpoint>
      </Section>

      <Section
        title="Use it from another project"
        lead="Any HTTP client works. Responses are cacheable (30s–30m), so repeated calls are cheap."
      >
        <Tabs defaultValue="curl">
          <TabsList>
            <TabsTrigger value="curl">curl</TabsTrigger>
            <TabsTrigger value="javascript">JavaScript</TabsTrigger>
            <TabsTrigger value="python">Python</TabsTrigger>
          </TabsList>
          <TabsContent value="curl">
            <Code>{CURL}</Code>
          </TabsContent>
          <TabsContent value="javascript">
            <Code>{JS_FETCH}</Code>
          </TabsContent>
          <TabsContent value="python">
            <Code>{PYTHON}</Code>
          </TabsContent>
        </Tabs>
      </Section>

      <Section
        title="Search response"
        lead={
          <>
            Detail endpoints return the resource itself, so read{" "}
            <code>body.albumUnion</code>, <code>body.trackUnion</code>,{" "}
            <code>body.artistUnion</code> or <code>body.playlistV2</code>.
          </>
        }
      >
        <Code>{SEARCH_JSON}</Code>
      </Section>

      <Section
        title="Build a player"
        lead="The API is data only: it hands you ids, covers, durations and optional 30-second previews. Audio playback and full tracks need your own player, so use the official SDK where it applies."
      >
        <Code>{PLAYBACK}</Code>
        <Note title="No audio stream.">
          This service never proxies or decrypts Spotify audio. Use the ids it
          returns with the Spotify Web Playback SDK, or an embed such as{" "}
          <code>open.spotify.com/embed/track/…</code>, for real playback.
        </Note>
      </Section>

      <Section
        title="Charts"
        lead={
          <>
            Charts for Indonesia (<code>id</code>) and the world (
            <code>global</code>): the ranked chart plus trending songs, popular
            albums and artists. <code>top</code> picks which chart is expanded
            with its 50 ranked rows.
          </>
        }
      >
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <Th>Query param</Th>
              <Th>Type</Th>
              <Th>Notes</Th>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow>
              <TdMono>top</TdMono>
              <Td>string</Td>
              <Td>
                <code>daily</code> (default) or <code>alltime</code>
              </Td>
            </TableRow>
            <TableRow>
              <TdMono>limit</TdMono>
              <Td>integer</Td>
              <Td>Accepted for compatibility; rows are always capped at 50</Td>
            </TableRow>
          </TableBody>
        </Table>

        <Code>{CHARTS}</Code>
        <Code>{CHARTS_JSON}</Code>
        <Note title="One market at a time.">
          Each chart row carries its rank (<code>position</code>), the previous
          rank (<code>previous</code>) and the play count (<code>plays</code>).
          Charts refresh a few times a day, so they are cached as a whole.
        </Note>
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <Th>Field</Th>
              <Th>Shape</Th>
              <Th>Notes</Th>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow>
              <TdMono>charts[0].entries</TdMono>
              <Td>ChartEntry[]</Td>
              <Td>
                The ranked rows: up to 50, with <code>position</code>,{" "}
                <code>previous</code> (<code>null</code> = new entry),{" "}
                <code>plays</code>, plus the usual track fields
              </Td>
            </TableRow>
            <TableRow>
              <TdMono>trending</TdMono>
              <Td>Track[]</Td>
              <Td>
                Tracks rising in the market right now; 9 per market,{" "}
                <code>duration_ms</code> usually <code>null</code>
              </Td>
            </TableRow>
            <TableRow>
              <TdMono>popular_albums</TdMono>
              <Td>Album[]</Td>
              <Td>Albums charting in the market; 9 per market</Td>
            </TableRow>
            <TableRow>
              <TdMono>popular_artists</TdMono>
              <Td>Artist[]</Td>
              <Td>
                Artists trending in the market; 9 per market. Call{" "}
                <code>/v1/artist/&#123;id&#125;</code> for the full profile
              </Td>
            </TableRow>
          </TableBody>
        </Table>
        <Note title="Sections can be empty.">
          Every list is published per market, so a market with no shelf answers
          with <code>[]</code> — the page hides that section instead of failing.
          Field names are always present.
        </Note>
      </Section>

      <Section
        title="User endpoints (/v1/me)"
        lead={
          <>
            User-scoped endpoints inspect the caller's own Spotify account,
            saved playlists, Liked Songs, and top tracks/artists. They accept
            either a browser <code>sp_dc</code> cookie from{" "}
            <code>open.spotify.com</code> (exchanged automatically via Spotify's
            TOTP token endpoint) or a bearer <code>access_token</code> (
            <code>BQ…</code>).
          </>
        }
      >
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <Th>Header / Param</Th>
              <Th>Format</Th>
              <Th>Notes</Th>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow>
              <TdMono>Authorization</TdMono>
              <TdMono>Bearer &lt;token|sp_dc&gt;</TdMono>
              <Td>
                Accepts a Spotify <code>BQ…</code> access token or raw{" "}
                <code>sp_dc</code> cookie value
              </Td>
            </TableRow>
            <TableRow>
              <TdMono>X-Sp-Dc</TdMono>
              <TdMono>&lt;sp_dc&gt;</TdMono>
              <Td>
                Explicit <code>sp_dc</code> cookie header
              </Td>
            </TableRow>
            <TableRow>
              <TdMono>Cookie</TdMono>
              <TdMono>sp_dc=&lt;value&gt;</TdMono>
              <Td>Standard HTTP cookie header</Td>
            </TableRow>
          </TableBody>
        </Table>

        <Code>{ME_CURL}</Code>
        <Code>{ME_JSON}</Code>
        <Note title="Private and never cached by CDNs.">
          Every <code>/v1/me*</code> response carries{" "}
          <code>Cache-Control: private, no-store</code> and runs on an isolated
          cookie-less HTTP client so user credentials never leak into the shared
          anonymous session.
        </Note>
      </Section>

      <Section title="Errors">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <Th>Status</Th>
              <Th>Body</Th>
              <Th>When</Th>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow>
              <TdMono>400</TdMono>
              <TdMono>{ERROR_BODY}</TdMono>
              <Td>Bad param, or an id that is not a Spotify id</Td>
            </TableRow>
            <TableRow>
              <TdMono>401</TdMono>
              <TdMono>{ERROR_BODY}</TdMono>
              <Td>
                Missing, invalid, or expired Spotify user token /{" "}
                <code>sp_dc</code> cookie on <code>/v1/me*</code>
              </Td>
            </TableRow>
            <TableRow>
              <TdMono>403</TdMono>
              <TdMono>{ERROR_BODY}</TdMono>
              <Td>
                Origin not allowed (only when <code>ALLOWED_ORIGIN</code> is
                set)
              </Td>
            </TableRow>
            <TableRow>
              <TdMono>404</TdMono>
              <TdMono>{ERROR_BODY}</TdMono>
              <Td>Unknown API path</Td>
            </TableRow>
            <TableRow>
              <TdMono>502</TdMono>
              <TdMono>{ERROR_BODY}</TdMono>
              <Td>Spotify upstream failed; details stay in the server log</Td>
            </TableRow>
          </TableBody>
        </Table>
      </Section>

      <Section title="Fair use">
        <p className="mb-4.5 text-[15px] text-foreground/80">
          There is no auth and no rate limit: anyone who can reach the host can
          call it. Please cache ids you already have, and add your own auth if
          you run a public app on top of it.
        </p>
        <p className="text-[15px] text-foreground/80">
          Unofficial and unaffiliated with Spotify. The upstream protocol can
          break at any time, and all traffic spends Spotify's shared anonymous
          session, so abuse can get it throttled.
        </p>
      </Section>
    </main>
  );
}
