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
            .and_then(Value::as_str)
            .map(str::to_string),
        duration_ms: value
            .pointer("/duration/totalMilliseconds")
            .or_else(|| value.pointer("/trackDuration/totalMilliseconds"))
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
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/owner/name").and_then(Value::as_str))
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
}
