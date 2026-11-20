//! Spotify catalogue models returned by the client.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchResults {
    pub query: String,
    pub tracks: Vec<Track>,
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Track {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Artist {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Album {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub artists: Vec<String>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Playlist {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub owner: Option<String>,
    pub image: Option<String>,
}

/// A track on a Spotify chart, in rank order.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ChartEntry {
    pub position: usize,
    /// Previous rank, when Spotify reports one. `None` for new entries.
    pub previous: Option<usize>,
    pub peak: Option<usize>,
    pub entries: Option<usize>,
    pub track: Option<Track>,
    pub name: String,
    pub id: String,
    pub uri: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub image: Option<String>,
    /// Number of plays, when the chart exposes it (usually only for its top 200).
    pub plays: Option<u64>,
}

/// One chart: a Spotify editorial playlist with `format: "chart"`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Chart {
    pub id: String,
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub image: Option<String>,
    /// `daily`, `weekly` or `monthly`, read from the chart playlist description
    /// or its artwork path.
    pub frequency: Option<String>,
    pub updated_at: Option<String>,
    pub rank_type: Option<String>,
    pub entries: Vec<ChartEntry>,
}

/// Country hub payload grouped by shelf.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Charts {
    pub country: String,
    pub color: Option<String>,
    /// Chart playlists (`CHARTS` shelf).
    pub charts: Vec<Chart>,
    pub trending: Vec<Track>,
    pub popular_albums: Vec<Album>,
    pub popular_artists: Vec<Artist>,
}

/// Category card on the browse/search explore grid.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BrowseCategory {
    pub id: String,
    pub title: String,
    pub color: Option<String>,
    pub image_url: Option<String>,
    pub href: String,
}

/// A curated genre / browse page with sections of playlists and releases.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GenrePage {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub sections: Vec<GenreSection>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GenreSection {
    pub title: String,
    pub items: Vec<GenreItem>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GenreItem {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artists: Option<Vec<String>>,
}

/// Authenticated user profile and account attributes (`GET /v1/me`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UserProfile {
    pub id: String,
    pub uri: String,
    pub username: String,
    pub name: String,
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_filter: Option<bool>,
}

/// Paginated list of playlists from the authenticated user's library (`GET /v1/me/playlists`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UserPlaylists {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub items: Vec<Playlist>,
}

/// A liked track in the authenticated user's library.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SavedTrack {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_at: Option<String>,
    #[serde(flatten)]
    pub track: Track,
}

/// Paginated list of the authenticated user's Liked Songs (`GET /v1/me/tracks`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UserTracks {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub items: Vec<SavedTrack>,
}

/// Authenticated user's top tracks and artists (`GET /v1/me/top`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UserTop {
    pub range: String,
    pub tracks: Vec<Track>,
    pub artists: Vec<Artist>,
}

/// Parse a `countryHubsPage` payload into typed charts.
pub fn parse_charts(country: &str, data: &Value) -> Charts {
    let mut charts = Charts {
        country: country.to_uppercase(),
        ..Default::default()
    };

    let hub = &data["countryHub"];

    charts.color = hub
        .pointer("/country/hexColor")
        .and_then(Value::as_str)
        .map(str::to_string);

    let Some(shelves) = hub.get("shelves").and_then(Value::as_array) else {
        return charts;
    };

    for shelf in shelves {
        let content_id = shelf
            .get("contentId")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let items: Vec<&Value> = shelf
            .pointer("/items/items")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(payload).collect())
            .unwrap_or_default();

        match content_id {
            "CHARTS" => charts.charts = items.iter().filter_map(|item| parse_chart(item)).collect(),
            "TRENDING_SONGS" => charts.trending = items.iter().filter_map(|item| parse_track(item)).collect(),
            "POPULAR_ALBUMS" => {
                charts.popular_albums = items.iter().filter_map(|item| parse_album(item)).collect()
            }
            "POPULAR_ARTISTS" => {
                charts.popular_artists = items.iter().filter_map(|item| parse_artist(item)).collect()
            }
            _ => {}
        }
    }

    charts
}

/// Which market a chart covers, read from its artwork path
/// (`…/regional/daily/region_global_default.jpg`).
pub fn market_of(chart: &Chart) -> Option<&'static str> {
    let image = chart.image.as_deref()?;
    if image.contains("region_global") {
        Some("global")
    } else if image.contains("/regional/") {
        Some("local")
    } else {
        None
    }
}

/// Rewrite the market suffix of a chart, so a global chart never reads
/// "Top 50 - USA" and a local one matches the market we serve.
pub fn relabel(chart: &mut Chart, market: &str) {
    let rename = |text: &str| match text.split_once(" - ") {
        Some((head, _)) => format!("{head} - {market}"),
        None => text.to_string(),
    };

    chart.name = rename(&chart.name);
    if let Some(description) = chart.description.as_mut() {
        *description = rename(description);
    }
}

fn parse_chart(value: &Value) -> Option<Chart> {
    let uri = value.get("uri")?.as_str()?.to_string();
    let name = value.get("name")?.as_str()?.to_string();
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);

    let attribute = |key: &str| {
        value
            .pointer("/attributes")
            .and_then(Value::as_array)
            .and_then(|list| {
                list.iter()
                    .find(|entry| entry.get("key").and_then(Value::as_str) == Some(key))
            })
            .and_then(|entry| entry.get("value"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };

    let image = find_image(value);

    let frequency = image
        .as_deref()
        .and_then(|url| ["daily", "weekly", "monthly"].into_iter().find(|f| url.contains(&format!("/{f}/"))))
        .or_else(|| {
            let description = description.as_deref().unwrap_or_default().to_lowercase();
            ["daily", "weekly", "monthly"]
                .into_iter()
                .find(|f| description.contains(f))
        })
        .map(str::to_string);

    Some(Chart {
        id: id_from_uri(&uri),
        uri,
        name,
        description,
        image,
        frequency,
        updated_at: attribute("last_updated"),
        rank_type: attribute("rank_type"),
        entries: Vec::new(),
    })
}

/// Fill `chart.entries` from the playlist payload behind a chart playlist.
///
/// Chart playlists carry the ranking in the item `attributes`
/// (`current_pos`, `previous_pos`, `rank` = play count) and the play total in
/// `data.playcount`. When those are missing only the position and the track
/// metadata are kept.
pub fn parse_chart_entries(chart: &mut Chart, data: &Value) {
    let Some(items) = data
        .pointer("/playlistV2/content/items")
        .and_then(Value::as_array)
    else {
        return;
    };

    for (index, item) in items.iter().enumerate() {
        let Some(data) = item.pointer("/itemV2/data") else {
            continue;
        };

        let Some(track) = parse_track(data) else {
            continue;
        };

        let attribute = |key: &str| {
            item.pointer("/attributes")
                .and_then(Value::as_array)
                .and_then(|list| {
                    list.iter()
                        .find(|entry| entry.get("key").and_then(Value::as_str) == Some(key))
                })
                .and_then(|entry| entry.get("value"))
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<u64>().ok())
        };

        let plays = data
            .get("playcount")
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
            // `rank` is a per-chart play count, not a position: fall back to it
            // only when the track has no total.
            .filter(|plays| *plays > 0)
            .or_else(|| attribute("rank").filter(|plays| *plays > 0));

        chart.entries.push(ChartEntry {
            position: attribute("current_pos")
                .map(|pos| pos as usize)
                .unwrap_or(index + 1),
            previous: attribute("previous_pos").map(|pos| pos as usize),
            peak: None,
            entries: None,
            plays,
            name: track.name.clone(),
            id: track.id.clone(),
            uri: track.uri.clone(),
            artists: track.artists.clone(),
            album: track.album.clone(),
            duration_ms: track.duration_ms,
            image: track.image.clone(),
            track: Some(track),
        });
    }
}

/// Parse a `searchDesktop` payload into typed results.
pub fn parse_search(query: &str, data: &Value) -> SearchResults {
    let mut results = SearchResults {
        query: query.to_string(),
        ..Default::default()
    };

    let Some(root) = data.get("searchV2").or_else(|| data.get("search")) else {
        return results;
    };

    results.tracks = section(root, &["tracksV2", "tracks"])
        .into_iter()
        .filter_map(parse_track)
        .collect();
    results.artists = section(root, &["artists", "artistsV2"])
        .into_iter()
        .filter_map(parse_artist)
        .collect();
    results.albums = section(root, &["albums", "albumsV2"])
        .into_iter()
        .filter_map(parse_album)
        .collect();
    results.playlists = section(root, &["playlists", "playlistsV2"])
        .into_iter()
        .filter_map(parse_playlist)
        .collect();

    results
}

/// Parse the browse/explore grid from a search page's `initialState`.
pub fn parse_browse_categories(data: &Value) -> Vec<BrowseCategory> {
    let mut categories = Vec::new();
    let Some(sections) = data.pointer("/browse/grid/sections").and_then(Value::as_array) else {
        return categories;
    };

    for section in sections {
        let Some(items) = section.get("items").and_then(Value::as_array) else {
            continue;
        };

        for item in items {
            let Some(title) = item.get("title").and_then(Value::as_str) else {
                continue;
            };
            if title.trim().is_empty() {
                continue;
            }

            let href = item.get("href").and_then(Value::as_str).unwrap_or_default();
            let id = href
                .strip_prefix("/genre/")
                .or_else(|| href.strip_prefix("/cultural-moment/hub/"))
                .unwrap_or(href)
                .to_string();

            categories.push(BrowseCategory {
                id,
                title: title.to_string(),
                color: item.get("color").and_then(Value::as_str).map(str::to_string),
                image_url: item.get("imageUrl").and_then(Value::as_str).map(str::to_string),
                href: href.to_string(),
            });
        }
    }

    categories
}

/// Parse a genre page from its `initialState`.
pub fn parse_genre_page(id: &str, data: &Value) -> Option<GenrePage> {
    let pages = data.pointer("/browse/pages")?.as_object()?;
    let page = pages
        .get(&format!("spotify:page:{id}"))
        .or_else(|| pages.values().find(|p| p.get("title").is_some()))?;

    let title = page.get("title")?.as_str()?.to_string();
    let subtitle = page
        .get("subtitle")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);

    let mut sections = Vec::new();
    if let Some(secs) = page.get("sections").and_then(Value::as_array) {
        for sec in secs {
            let sec_title = sec
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            let mut items = Vec::new();
            if let Some(raw_items) = sec.get("items").and_then(Value::as_array) {
                for item in raw_items {
                    if let Some(item_title) = item.get("title").and_then(Value::as_str) {
                        let href = item.get("href").and_then(Value::as_str).unwrap_or_default().to_string();
                        let description = item.get("description").and_then(Value::as_str).map(str::to_string);
                        let image_url = item.get("imageUrl").and_then(Value::as_str).map(str::to_string);
                        let followers = item.get("followers").and_then(Value::as_u64);
                        let artists = item.get("artistsNames").and_then(Value::as_array).map(|arr| {
                            arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
                        });

                        items.push(GenreItem {
                            title: item_title.to_string(),
                            description,
                            href,
                            image_url,
                            followers,
                            artists,
                        });
                    }
                }
            }

            if !items.is_empty() || !sec_title.is_empty() {
                sections.push(GenreSection {
                    title: sec_title,
                    items,
                });
            }
        }
    }

    Some(GenrePage {
        id: id.to_string(),
        title,
        subtitle,
        sections,
    })
}

fn section<'a>(root: &'a Value, names: &[&str]) -> Vec<&'a Value> {
    for name in names {
        let Some(items) = root
            .get(*name)
            .and_then(|section| section.get("items"))
            .and_then(Value::as_array)
        else {
            continue;
        };

        return items.iter().map(payload).collect();
    }

    Vec::new()
}

/// Search records wrap the entity differently per section: tracks use
/// `{ item: { data: ... } }`, playlists and artists use `{ data: ... }`.
fn payload(item: &Value) -> &Value {
    item.pointer("/item/data")
        .or_else(|| item.get("data"))
        .or_else(|| item.get("item"))
        .unwrap_or(item)
}

fn id_from_uri(uri: &str) -> String {
    uri.rsplit(':').next().unwrap_or(uri).to_string()
}

/// Parse user profile and account attributes from pathfinder (`profileAttributes` +
/// `accountAttributes`) or Spotify Web API (`/v1/me`).
pub fn parse_user_profile(profile_data: &Value, account_data: Option<&Value>) -> Option<UserProfile> {
    let profile = profile_data
        .pointer("/me/profile")
        .or_else(|| profile_data.get("profile"))
        .unwrap_or(profile_data);

    let account = account_data
        .and_then(|data| data.pointer("/me/account").or_else(|| data.get("account")).or(Some(data)))
        .or_else(|| profile_data.pointer("/me/account"));

    let uri = profile
        .get("uri")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            profile
                .get("id")
                .or_else(|| profile.get("username"))
                .and_then(Value::as_str)
                .map(|id| format!("spotify:user:{id}"))
        })?;

    let id = profile
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| id_from_uri(&uri));

    let username = profile
        .get("username")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| id.clone());

    let name = profile
        .get("name")
        .or_else(|| profile.get("display_name"))
        .and_then(Value::as_str)
        .unwrap_or(&username)
        .to_string();

    let image = find_avatar(profile).or_else(|| find_image(profile));

    let country = account
        .and_then(|acc| acc.get("country"))
        .or_else(|| profile.get("country"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let product = account
        .and_then(|acc| acc.get("product"))
        .or_else(|| profile.get("product"))
        .and_then(Value::as_str)
        .map(|p| p.to_ascii_lowercase());

    let explicit_filter = account
        .and_then(|acc| {
            acc.pointer("/explicitContent/filterEnabled")
                .or_else(|| acc.pointer("/explicit_content/filter_enabled"))
        })
        .or_else(|| profile.pointer("/explicit_content/filter_enabled"))
        .and_then(Value::as_bool);

    Some(UserProfile {
        id,
        uri,
        username,
        name,
        image,
        country,
        product,
        explicit_filter,
    })
}

/// Parse user playlists from pathfinder (`libraryV3`) or Web API (`/v1/me/playlists`).
pub fn parse_user_playlists(data: &Value, limit: usize, offset: usize) -> UserPlaylists {
    let lib = data
        .pointer("/me/libraryV3")
        .or_else(|| data.get("libraryV3"))
        .unwrap_or(data);

    let raw_items = lib
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();
    for entry in &raw_items {
        let candidate = entry
            .pointer("/item/data")
            .or_else(|| entry.get("item"))
            .or_else(|| entry.get("data"))
            .unwrap_or(entry);

        let typename = candidate
            .get("__typename")
            .or_else(|| candidate.get("type"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let uri = candidate
            .get("uri")
            .or_else(|| entry.pointer("/item/_uri"))
            .and_then(Value::as_str)
            .unwrap_or_default();

        if !typename.eq_ignore_ascii_case("playlist") && !uri.contains(":playlist:") {
            continue;
        }

        if let Some(playlist) = parse_playlist(candidate) {
            items.push(playlist);
        }
    }

    let total = lib
        .get("totalCount")
        .or_else(|| lib.get("total"))
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(items.len());

    UserPlaylists {
        total,
        limit,
        offset,
        items,
    }
}

/// Parse user's Liked Songs from pathfinder (`fetchLibraryTracks`) or Web API (`/v1/me/tracks`).
pub fn parse_user_tracks(data: &Value, limit: usize, offset: usize) -> UserTracks {
    let tracks_node = data
        .pointer("/me/library/tracks")
        .or_else(|| data.pointer("/library/tracks"))
        .unwrap_or(data);

    let raw_items = tracks_node
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();
    for entry in &raw_items {
        let added_at = entry
            .pointer("/addedAt/isoString")
            .or_else(|| entry.get("added_at"))
            .and_then(Value::as_str)
            .map(str::to_string);

        let track_node = entry
            .pointer("/track/data")
            .or_else(|| entry.get("track"))
            .unwrap_or(entry);

        let mut normalized = track_node.clone();
        if normalized.get("uri").and_then(Value::as_str).is_none()
            && let Some(fallback_uri) = entry
                .pointer("/track/_uri")
                .or_else(|| entry.pointer("/track/uri"))
                .and_then(Value::as_str)
        {
            normalized["uri"] = Value::String(fallback_uri.to_string());
        }

        if let Some(track) = parse_track(&normalized) {
            items.push(SavedTrack { added_at, track });
        }
    }

    let total = tracks_node
        .get("totalCount")
        .or_else(|| tracks_node.get("total"))
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(items.len());

    UserTracks {
        total,
        limit,
        offset,
        items,
    }
}

/// Parse top tracks and artists from pathfinder (`userTopContent`) or Web API (`/v1/me/top/*`).
pub fn parse_user_top(data: &Value, range: &str) -> UserTop {
    let tracks_items = data
        .pointer("/me/profile/topTracks/items")
        .or_else(|| data.pointer("/me/topTracks/items"))
        .or_else(|| data.pointer("/topTracks/items"))
        .or_else(|| data.pointer("/tracks/items"))
        .and_then(Value::as_array);

    let artists_items = data
        .pointer("/me/profile/topArtists/items")
        .or_else(|| data.pointer("/me/topArtists/items"))
        .or_else(|| data.pointer("/topArtists/items"))
        .or_else(|| data.pointer("/artists/items"))
        .and_then(Value::as_array);

    let tracks = tracks_items
        .map(|list| list.iter().map(payload).filter_map(parse_track).collect())
        .unwrap_or_default();

    let artists = artists_items
        .map(|list| list.iter().map(payload).filter_map(parse_artist).collect())
        .unwrap_or_default();

    UserTop {
        range: range.to_string(),
        tracks,
        artists,
    }
}

fn find_avatar(profile: &Value) -> Option<String> {
    let sources = profile
        .pointer("/avatar/sources")
        .or_else(|| profile.get("images"))
        .and_then(Value::as_array)?;

    sources
        .iter()
        .max_by_key(|source| source.get("width").and_then(Value::as_u64).unwrap_or(0))
        .and_then(|source| source.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn parse_track(value: &Value) -> Option<Track> {
    let uri = value.get("uri")?.as_str()?.to_string();
    let name = value.get("name")?.as_str()?.to_string();

    Some(Track {
        id: id_from_uri(&uri),
        uri,
        name,
        artists: artist_names(value),
        album: value
            .pointer("/albumOfTrack/name")
            .or_else(|| value.pointer("/album/name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        duration_ms: value
            .pointer("/duration/totalMilliseconds")
            .or_else(|| value.pointer("/trackDuration/totalMilliseconds"))
            .or_else(|| value.get("duration_ms"))
            .and_then(Value::as_u64),
        image: find_image(value),
    })
}

fn parse_artist(value: &Value) -> Option<Artist> {
    let uri = value.get("uri")?.as_str()?.to_string();
    let name = person_name(value)?;

    Some(Artist {
        id: id_from_uri(&uri),
        uri,
        name,
        image: find_image(value),
    })
}

fn parse_album(value: &Value) -> Option<Album> {
    let uri = value.get("uri")?.as_str()?.to_string();
    let name = value.get("name")?.as_str()?.to_string();

    Some(Album {
        id: id_from_uri(&uri),
        uri,
        name,
        artists: artist_names(value),
        image: find_image(value),
    })
}

fn parse_playlist(value: &Value) -> Option<Playlist> {
    let uri = value.get("uri")?.as_str()?.to_string();
    let name = value.get("name")?.as_str()?.to_string();

    Some(Playlist {
        id: id_from_uri(&uri),
        uri,
        name,
        owner: owner_name(value),
        image: find_image(value),
    })
}

fn artist_names(value: &Value) -> Vec<String> {
    value
        .pointer("/artists/items")
        .or_else(|| value.get("artists"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(person_name).collect())
        .unwrap_or_default()
}

fn person_name(value: &Value) -> Option<String> {
    value
        .pointer("/profile/name")
        .and_then(Value::as_str)
        .or_else(|| value.get("name").and_then(Value::as_str))
        .map(str::to_string)
}

fn owner_name(value: &Value) -> Option<String> {
    value
        .pointer("/ownerV2/data/name")
        .or_else(|| value.pointer("/owner/name"))
        .or_else(|| value.pointer("/owner/display_name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn find_image(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(url) = map.get("url").and_then(Value::as_str)
                && (url.contains("scdn.co") || url.contains("spotifycdn.com"))
            {
                return Some(url.to_string());
            }

            map.values().find_map(find_image)
        }
        Value::Array(items) => items.iter().find_map(find_image),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_desktop_search() {
        let data = json!({
            "searchV2": {
                "tracksV2": {
                    "items": [{
                        "item": {"data": {
                            "uri": "spotify:track:abc",
                            "name": "Blinding Lights",
                            "artists": {"items": [
                                {"profile": {"name": "The Weeknd"}}
                            ]},
                            "duration": {"totalMilliseconds": 200040},
                            "albumOfTrack": {
                                "name": "After Hours",
                                "coverArt": {"sources": [
                                    {"url": "https://i.scdn.co/image/cover", "width": 640}
                                ]}
                            }
                        }}
                    }]
                },
                "artists": {
                    "items": [{
                        "item": {"data": {
                            "uri": "spotify:artist:xyz",
                            "profile": {"name": "The Weeknd"}
                        }}
                    }]
                }
            }
        });

        let results = parse_search("weeknd", &data);
        assert_eq!(results.tracks.len(), 1);
        assert_eq!(results.tracks[0].artists, vec!["The Weeknd"]);
        assert_eq!(results.tracks[0].duration_ms, Some(200040));
        assert_eq!(
            results.tracks[0].image.as_deref(),
            Some("https://i.scdn.co/image/cover")
        );
        assert_eq!(results.artists[0].name, "The Weeknd");
    }

    #[test]
    fn parses_charts_and_entries() {
        let data = json!({
            "countryHub": {
                "country": {"hexColor": "#F037A5", "isoCountryCode": "ID"},
                "shelves": [{
                    "contentId": "CHARTS",
                    "items": {"items": [{
                        "data": {
                            "uri": "spotify:playlist:37i9dQZEVXbIZK8aUquyx8",
                            "name": "Top Songs - Indonesia",
                            "description": "Your weekly update of the most played tracks right now - Indonesia.",
                            "format": "chart",
                            "attributes": [
                                {"key": "last_updated", "value": "2026-11-13T15:04:44Z"},
                                {"key": "rank_type", "value": "plays"}
                            ],
                            "images": {"items": [{"sources": [
                                {"url": "https://charts-images.scdn.co/assets/locale_en/regional/weekly/region_id_default.jpg"}
                            ]}]}
                        }
                    }]}
                }]
            }
        });

        let mut charts = parse_charts("id", &data);

        assert_eq!(charts.country, "ID");
        assert_eq!(charts.color.as_deref(), Some("#F037A5"));
        assert_eq!(charts.charts.len(), 1);

        let chart = &mut charts.charts[0];
        assert_eq!(chart.id, "37i9dQZEVXbIZK8aUquyx8");
        assert_eq!(chart.frequency.as_deref(), Some("weekly"));
        assert_eq!(chart.updated_at.as_deref(), Some("2026-11-13T15:04:44Z"));

        let playlist = json!({
            "playlistV2": {"content": {"items": [{
                "attributes": [
                    {"key": "current_pos", "value": "1"},
                    {"key": "previous_pos", "value": "3"},
                    {"key": "rank", "value": "1149177"}
                ],
                "itemV2": {"data": {
                    "uri": "spotify:track:abc",
                    "name": "Judul",
                    "playcount": "12345",
                    "trackDuration": {"totalMilliseconds": 243000},
                    "artists": {"items": [{"profile": {"name": "Penyanyi"}}]},
                    "albumOfTrack": {"name": "Album"}
                }}
            }]}}
        });

        parse_chart_entries(chart, &playlist);

        assert_eq!(chart.entries.len(), 1);
        assert_eq!(chart.entries[0].position, 1);
        assert_eq!(chart.entries[0].plays, Some(12345));
        assert_eq!(chart.entries[0].previous, Some(3));
        assert_eq!(chart.entries[0].duration_ms, Some(243000));
        assert_eq!(chart.entries[0].artists, vec!["Penyanyi"]);
    }

    #[test]
    fn parses_browse_categories_grid() {
        let data = json!({
            "browse": {
                "grid": {
                    "sections": [{
                        "items": [
                            {
                                "title": "Anime",
                                "color": "#0d73ec",
                                "href": "/genre/0JQ5DAqbMKFziKOShCi009",
                                "imageUrl": "https://i.scdn.co/image/anime.jpg"
                            },
                            {
                                "title": "Punk",
                                "color": "#e61e32",
                                "href": "/genre/0JQ5DAqbMKFAjfauKLOZiv",
                                "imageUrl": "https://i.scdn.co/image/punk.jpg"
                            }
                        ]
                    }]
                }
            }
        });

        let categories = parse_browse_categories(&data);
        assert_eq!(categories.len(), 2);
        assert_eq!(categories[0].id, "0JQ5DAqbMKFziKOShCi009");
        assert_eq!(categories[0].title, "Anime");
        assert_eq!(categories[0].color.as_deref(), Some("#0d73ec"));
        assert_eq!(categories[0].image_url.as_deref(), Some("https://i.scdn.co/image/anime.jpg"));
        assert_eq!(categories[1].id, "0JQ5DAqbMKFAjfauKLOZiv");
        assert_eq!(categories[1].title, "Punk");
    }

    #[test]
    fn parses_genre_page_sections() {
        let data = json!({
            "browse": {
                "pages": {
                    "spotify:page:0JQ5DAqbMKFziKOShCi009": {
                        "title": "Anime",
                        "subtitle": "Anime hits and classics",
                        "sections": [{
                            "title": "Popular Anime Playlists",
                            "items": [{
                                "title": "Anime Now",
                                "description": "The best new anime hits",
                                "href": "/playlist/37i9dQZF1DX6uhv2X9VqT5",
                                "imageUrl": "https://i.scdn.co/image/animenow.jpg",
                                "followers": 1500000
                            }]
                        }]
                    }
                }
            }
        });

        let page = parse_genre_page("0JQ5DAqbMKFziKOShCi009", &data).unwrap();
        assert_eq!(page.id, "0JQ5DAqbMKFziKOShCi009");
        assert_eq!(page.title, "Anime");
        assert_eq!(page.subtitle.as_deref(), Some("Anime hits and classics"));
        assert_eq!(page.sections.len(), 1);
        assert_eq!(page.sections[0].title, "Popular Anime Playlists");
        assert_eq!(page.sections[0].items.len(), 1);
        assert_eq!(page.sections[0].items[0].title, "Anime Now");
        assert_eq!(page.sections[0].items[0].followers, Some(1500000));
        assert_eq!(page.sections[0].items[0].href, "/playlist/37i9dQZF1DX6uhv2X9VqT5");
    }

    #[test]
    fn parses_user_profile_from_pathfinder_and_web_api() {
        let profile = json!({
            "me": {
                "profile": {
                    "uri": "spotify:user:ariblekk",
                    "username": "ariblekk",
                    "name": "Ari Haryanto",
                    "avatar": {
                        "sources": [
                            {"url": "https://i.scdn.co/image/small", "width": 64},
                            {"url": "https://i.scdn.co/image/large", "width": 300}
                        ]
                    }
                }
            }
        });
        let account = json!({
            "me": {
                "account": {
                    "country": "ID",
                    "product": "PREMIUM",
                    "explicitContent": {"filterEnabled": false}
                }
            }
        });

        let parsed = parse_user_profile(&profile, Some(&account)).unwrap();
        assert_eq!(parsed.id, "ariblekk");
        assert_eq!(parsed.uri, "spotify:user:ariblekk");
        assert_eq!(parsed.username, "ariblekk");
        assert_eq!(parsed.name, "Ari Haryanto");
        assert_eq!(parsed.image.as_deref(), Some("https://i.scdn.co/image/large"));
        assert_eq!(parsed.country.as_deref(), Some("ID"));
        assert_eq!(parsed.product.as_deref(), Some("premium"));
        assert_eq!(parsed.explicit_filter, Some(false));

        let web_api = json!({
            "id": "ariblekk",
            "uri": "spotify:user:ariblekk",
            "display_name": "Ari Haryanto",
            "country": "ID",
            "product": "free",
            "explicit_content": {"filter_enabled": true},
            "images": [{"url": "https://i.scdn.co/image/avatar", "width": 300}]
        });
        let parsed_web = parse_user_profile(&web_api, None).unwrap();
        assert_eq!(parsed_web.id, "ariblekk");
        assert_eq!(parsed_web.name, "Ari Haryanto");
        assert_eq!(parsed_web.product.as_deref(), Some("free"));
        assert_eq!(parsed_web.explicit_filter, Some(true));
    }

    #[test]
    fn parses_user_playlists_tracks_and_top() {
        let lib = json!({
            "me": {
                "libraryV3": {
                    "totalCount": 2,
                    "items": [
                        {
                            "item": {
                                "data": {
                                    "__typename": "Playlist",
                                    "uri": "spotify:playlist:pl1",
                                    "name": "Late Night Coding",
                                    "ownerV2": {"data": {"name": "Ari"}},
                                    "images": {"items": [{"sources": [{"url": "https://i.scdn.co/image/pl1"}]}]}
                                }
                            }
                        },
                        {
                            "item": {
                                "data": {
                                    "__typename": "Folder",
                                    "uri": "spotify:user:ari:folder:1",
                                    "name": "Ignored Folder"
                                }
                            }
                        }
                    ]
                }
            }
        });
        let playlists = parse_user_playlists(&lib, 20, 0);
        assert_eq!(playlists.total, 2);
        assert_eq!(playlists.items.len(), 1);
        assert_eq!(playlists.items[0].id, "pl1");
        assert_eq!(playlists.items[0].name, "Late Night Coding");
        assert_eq!(playlists.items[0].owner.as_deref(), Some("Ari"));

        let saved = json!({
            "me": {
                "library": {
                    "tracks": {
                        "totalCount": 1,
                        "items": [{
                            "addedAt": {"isoString": "2026-11-20T10:00:00Z"},
                            "track": {
                                "_uri": "spotify:track:tr1",
                                "data": {
                                    "name": "Sesi Potret",
                                    "artists": {"items": [{"profile": {"name": "enau"}}]},
                                    "albumOfTrack": {"name": "Sesi Potret"},
                                    "duration": {"totalMilliseconds": 210000}
                                }
                            }
                        }]
                    }
                }
            }
        });
        let tracks = parse_user_tracks(&saved, 20, 0);
        assert_eq!(tracks.total, 1);
        assert_eq!(tracks.items.len(), 1);
        assert_eq!(tracks.items[0].track.id, "tr1");
        assert_eq!(tracks.items[0].added_at.as_deref(), Some("2026-11-20T10:00:00Z"));
        assert_eq!(tracks.items[0].track.artists, vec!["enau"]);

        let top_data = json!({
            "me": {
                "profile": {
                    "topTracks": {
                        "items": [{
                            "data": {
                                "uri": "spotify:track:top1",
                                "name": "Yellow",
                                "artists": {"items": [{"profile": {"name": "Coldplay"}}]}
                            }
                        }]
                    },
                    "topArtists": {
                        "items": [{
                            "data": {
                                "uri": "spotify:artist:art1",
                                "profile": {"name": "Coldplay"}
                            }
                        }]
                    }
                }
            }
        });
        let top = parse_user_top(&top_data, "medium_term");
        assert_eq!(top.range, "medium_term");
        assert_eq!(top.tracks.len(), 1);
        assert_eq!(top.tracks[0].name, "Yellow");
        assert_eq!(top.artists.len(), 1);
        assert_eq!(top.artists[0].name, "Coldplay");
    }
}
