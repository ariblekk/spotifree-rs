//! Lyrics enrichment.
//!
//! Spotify's own lyrics endpoint (`color-lyrics/v2`) answers `400` to anonymous
//! server-side callers, so lyrics come from public, keyless sources:
//!
//! 1. [lrc.red](https://lrc.red) — keyed by **ISRC**, so a match is the exact
//!    release Spotify resolved, not a namesake cover. Its catalog is smaller
//!    than LRCLIB's, hence the fallback.
//! 2. [LRCLIB](https://lrclib.net) — searched by artist + title with duration
//!    as a tie-breaker. Loose by nature, so a match needs the artist and title
//!    to agree and the duration to be close.
//!
//! Either way a miss degrades to `None`, never an error.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const LRC_RED_SONG: &str = "https://lrc.red/s";
const LRCLIB_GET: &str = "https://lrclib.net/api/get";
const LRCLIB_SEARCH: &str = "https://lrclib.net/api/search";
const USER_AGENT: &str = "spotifree-rs/0.1.0 (+https://github.com/ariblekk/spotifree-rs)";

/// Seconds of slack allowed when matching an LRCLIB duration.
const DURATION_TOLERANCE: f64 = 4.0;

/// What `GET /api/lyrics/{id}` returns.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TrackExtras {
    /// From Spotify's protobuf metadata; `None` when unavailable.
    pub isrc: Option<String>,
    pub lyrics: Option<Lyrics>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Lyrics {
    /// `plain`, `synced` or `instrumental`.
    pub kind: String,
    /// Provider name, e.g. `lrc.red`.
    pub source: String,
    pub instrumental: bool,
    /// Plain lyrics with the original line breaks.
    pub plain: Option<String>,
    /// LRC document when the provider has time-synced lyrics.
    pub synced: Option<String>,
}

impl Lyrics {
    /// True when there is nothing to render.
    pub fn is_empty(&self) -> bool {
        self.instrumental || (self.plain.is_none() && self.synced.is_none())
    }
}

/// Track hints used to build the provider query.
#[derive(Debug, Clone, Default)]
pub struct TrackHints {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: Option<u64>,
    /// Enables the exact, ISRC-keyed lrc.red lookup.
    pub isrc: Option<String>,
}

impl TrackHints {
    /// Build hints from a pathfinder payload. Accepts either the `trackUnion`
    /// itself or the surrounding `{ "trackUnion": ... }` object.
    pub fn from_track_union(union: &Value) -> Self {
        let union = union.get("trackUnion").unwrap_or(union);

        let artist = union
            .pointer("/firstArtist/items/0/profile/name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default();

        let duration_secs = union
            .pointer("/duration/totalMilliseconds")
            .and_then(Value::as_u64)
            .map(|ms| ms.div_ceil(1000));

        Self {
            title: union
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            artist,
            album: union
                .pointer("/albumOfTrack/name")
                .and_then(Value::as_str)
                .map(str::to_string),
            duration_secs,
            isrc: None,
        }
    }

    /// Same hints, but able to use the ISRC-keyed provider.
    pub fn with_isrc(mut self, isrc: Option<String>) -> Self {
        self.isrc = isrc;
        self
    }

    fn is_usable(&self) -> bool {
        !self.title.trim().is_empty() && !self.artist.trim().is_empty()
    }
}

/// Lyrics client. Cheap to clone: the inner `reqwest::Client` owns the pool.
#[derive(Clone)]
pub struct Provider {
    http: reqwest::Client,
}

impl Provider {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .gzip(true)
            .timeout(Duration::from_secs(12))
            .build()
            .unwrap_or_default();

        Self { http }
    }

    /// Lyrics for the given track from the first provider that has them.
    ///
    /// The two providers are independent, so they run concurrently; lrc.red
    /// stays preferred because an ISRC hit is the exact release.
    pub async fn lyrics(&self, hints: &TrackHints) -> Option<Lyrics> {
        let (lrc_red, lrclib) = tokio::join!(self.lrc_red(hints), self.lrclib(hints));

        lrc_red.or(lrclib)
    }

    /// LRCLIB's two-step lookup: exact match first, then a fuzzy search.
    async fn lrclib(&self, hints: &TrackHints) -> Option<Lyrics> {
        if !hints.is_usable() {
            return None;
        }

        if let Some(found) = self.lrclib_get(hints).await {
            return Some(found);
        }

        self.lrclib_search(hints).await
    }

    /// lrc.red is keyed by ISRC, so the answer is the exact release Spotify
    /// resolved — no fuzzy matching. `None` means "not in their catalog".
    async fn lrc_red(&self, hints: &TrackHints) -> Option<Lyrics> {
        let isrc = hints.isrc.as_deref().filter(|isrc| is_isrc(isrc))?;
        let song = format!("{LRC_RED_SONG}/{isrc}");

        if let Some(lrc) = self.get_text(&format!("{song}.lrc")).await {
            let plain = to_plain(&lrc);
            if !plain.trim().is_empty() {
                return Some(Lyrics {
                    kind: if has_timestamp(&lrc) {
                        "synced"
                    } else {
                        "plain"
                    }
                    .into(),
                    source: "lrc.red".into(),
                    instrumental: false,
                    plain: Some(plain),
                    synced: has_timestamp(&lrc).then_some(lrc),
                });
            }
        }

        // Untimed releases only ship the YAML lyric file, one `text:` per line.
        let yaml = self.get_text(&format!("{song}.lyricsfile.yaml")).await?;
        let plain = yaml_lines(&yaml);
        if plain.trim().is_empty() {
            return None;
        }

        Some(Lyrics {
            kind: "plain".into(),
            source: "lrc.red".into(),
            instrumental: false,
            plain: Some(plain),
            synced: None,
        })
    }

    async fn get_text(&self, url: &str) -> Option<String> {
        let response = self.http.get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }

        response
            .text()
            .await
            .ok()
            .filter(|body| !body.trim().is_empty())
    }

    /// `GET /api/get` — exact match on artist + title + album + duration.
    async fn lrclib_get(&self, hints: &TrackHints) -> Option<Lyrics> {
        let mut request = self.http.get(LRCLIB_GET).query(&[
            ("artist_name", hints.artist.as_str()),
            ("track_name", hints.title.as_str()),
        ]);

        if let Some(album) = hints.album.as_deref().filter(|album| !album.is_empty()) {
            request = request.query(&[("album_name", album)]);
        }
        if let Some(seconds) = hints.duration_secs {
            request = request.query(&[("duration", seconds.to_string())]);
        }

        let response = request.send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }

        let record: LrcLibRecord = response.json().await.ok()?;
        record.into_lyrics()
    }

    /// `GET /api/search` — closest duration wins. The records must still be the
    /// same song: same artist, and either the same title or the same duration,
    /// otherwise a cover by another singer would be served.
    async fn lrclib_search(&self, hints: &TrackHints) -> Option<Lyrics> {
        let mut request = self.http.get(LRCLIB_SEARCH).query(&[
            ("track_name", hints.title.as_str()),
            ("artist_name", hints.artist.as_str()),
        ]);

        if let Some(album) = hints.album.as_deref().filter(|album| !album.is_empty()) {
            request = request.query(&[("album_name", album)]);
        }

        let response = request.send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }

        let mut records: Vec<LrcLibRecord> = response.json().await.ok()?;
        records.retain(|record| record.agrees_with(hints));
        if records.is_empty() {
            return None;
        }

        records.sort_by_key(|record| match (hints.duration_secs, record.duration) {
            (Some(wanted), Some(found)) => (1000.0 * (found - wanted as f64).abs()) as u64,
            _ => 0,
        });

        records.into_iter().find_map(LrcLibRecord::into_lyrics)
    }
}

impl Default for Provider {
    fn default() -> Self {
        Self::new()
    }
}

fn is_isrc(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 12
        && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_uppercase()
        && bytes[2..5]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && bytes[5..].iter().all(u8::is_ascii_digit)
}

/// True when a document carries at least one `[mm:ss.xx]` marker.
fn has_timestamp(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with('[')
            && line
                .find(']')
                .is_some_and(|end| is_timestamp(&line[1..end]))
    })
}

/// Drop `[mm:ss.xx]` prefixes, LRC header tags (`[ti:…]`) and enhanced-LRC
/// word markers (`<mm:ss.xx>`) so lyrics can be rendered as plain text.
fn to_plain(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut rest = line.trim_start();
            while rest.starts_with('[') {
                match rest.find(']') {
                    Some(end) if is_timestamp(&rest[1..end]) || is_tag(&rest[1..end]) => {
                        rest = rest[end + 1..].trim_start()
                    }
                    _ => break,
                }
            }

            drop_word_markers(rest).trim().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Enhanced LRC splits a line into per-word `<mm:ss.xx>` segments; keep only
/// the words.
fn drop_word_markers(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(end) if is_timestamp(&rest[start + 1..start + end]) => {
                rest = &rest[start + end + 1..]
            }
            _ => {
                out.push('<');
                rest = &rest[start + 1..];
            }
        }
    }

    out.push_str(rest);
    out
}

/// Pull `text: "…"` values out of a lyricsfile YAML, in document order.
fn yaml_lines(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let Some(value) = line
            .strip_prefix("- text:")
            .or_else(|| line.strip_prefix("text:"))
        else {
            continue;
        };

        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        if !value.is_empty() {
            lines.push(unescape(value));
        }
    }

    lines.join("\n")
}

fn unescape(value: &str) -> String {
    value
        .replace("\\\"", "\"")
        .replace("\\n", "\n")
        .replace("\\\\", "\\")
}

fn is_timestamp(field: &str) -> bool {
    !field.is_empty()
        && field.contains(':')
        && field
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == ':' || ch == '.')
}

/// LRC header tag such as `ti:Title` or `offset:0`.
fn is_tag(field: &str) -> bool {
    field.split_once(':').is_some_and(|(key, _)| {
        !key.is_empty() && key.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '_')
    })
}

#[derive(Debug, Deserialize)]
struct LrcLibRecord {
    #[serde(rename = "trackName")]
    track: Option<String>,
    #[serde(rename = "artistName")]
    artist: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced: Option<String>,
    #[serde(default)]
    instrumental: bool,
    duration: Option<f64>,
}

impl LrcLibRecord {
    /// The same recording, or close enough to trust the lyrics.
    fn agrees_with(&self, hints: &TrackHints) -> bool {
        let artist = self.artist.as_deref().unwrap_or_default();
        if normalize(artist) != normalize(&hints.artist) {
            return false;
        }

        let title_matches =
            normalize(self.track.as_deref().unwrap_or_default()) == normalize(&hints.title);

        let duration_matches = match (hints.duration_secs, self.duration) {
            (Some(wanted), Some(found)) => (found - wanted as f64).abs() <= DURATION_TOLERANCE,
            _ => false,
        };

        title_matches || duration_matches
    }

    fn into_lyrics(self) -> Option<Lyrics> {
        if self.instrumental {
            return Some(Lyrics {
                kind: "instrumental".into(),
                source: "LRCLIB".into(),
                instrumental: true,
                plain: None,
                synced: None,
            });
        }

        let plain = self.plain.filter(|text| !text.trim().is_empty());
        let synced = self.synced.filter(|text| !text.trim().is_empty());

        if plain.is_none() && synced.is_none() {
            return None;
        }

        Some(Lyrics {
            kind: if synced.is_some() { "synced" } else { "plain" }.into(),
            source: "LRCLIB".into(),
            instrumental: false,
            plain,
            synced,
        })
    }
}

/// Lowercase, drop a trailing `(feat. …)` and keep only alphanumerics, so
/// `Menua Bersamamu (feat. Alma Ghinaya)` matches `menua bersamamu`.
fn normalize(value: &str) -> String {
    let lowered = value.to_lowercase();
    let base = match lowered.find('(') {
        Some(index) => &lowered[..index],
        None => lowered.as_str(),
    };

    base.chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hints_come_from_track_union() {
        // The handler passes the full `data` object, so the wrapper is unwrapped.
        let data = json!({
            "trackUnion": {
                "name": "golfing",
                "duration": {"totalMilliseconds": 149495},
                "firstArtist": {"items": [{"profile": {"name": "wester bunny"}}]},
                "albumOfTrack": {"name": "golfing"}
            }
        });

        let hints = TrackHints::from_track_union(&data);
        assert_eq!(hints.title, "golfing");
        assert_eq!(hints.artist, "wester bunny");
        assert_eq!(hints.album.as_deref(), Some("golfing"));
        // 149_495ms rounds up to 150s, same as the web player's display.
        assert_eq!(hints.duration_secs, Some(150));
        assert_eq!(hints.isrc, None);
    }

    #[test]
    fn isrc_accepts_only_the_standard_shape() {
        assert!(is_isrc("IDB832600861"));
        assert!(!is_isrc("idb832600861"));
        assert!(!is_isrc("IDB83260086"));
        assert!(!is_isrc("1DB832600861"));
        assert!(!is_isrc("IDB83260086X"));
    }

    #[test]
    fn timestamps_and_header_tags_leave_plain_lines() {
        let lrc = "[ti:Menua Bersamamu]\n[00:20.75]Maafkan diriku yang salah\n[01:07.93]Takkan pergi\n[03:19.66]\n";

        assert_eq!(to_plain(lrc), "Maafkan diriku yang salah\nTakkan pergi");
        assert!(has_timestamp(lrc));
        assert!(!has_timestamp("[ti:x]\nplain line"));
    }

    #[test]
    fn word_markers_are_dropped_too() {
        let enhanced =
            "[00:33.52]<00:33.52>Look <00:33.84>at <00:34.16>the <00:34.46>stars <00:35.79>\n";
        assert_eq!(to_plain(enhanced), "Look at the stars");
        // A stray `<` that is not a timestamp is kept verbatim.
        assert_eq!(drop_word_markers("a < b"), "a < b");
    }

    #[test]
    fn yaml_lyrics_are_read_line_by_line() {
        let yaml = "version: '1.0'\n\nmetadata:\n  title: \"Menua Bersamamu\"\n\nlines:\n  - text: \"Maafkan diriku yang salah\"\n    start_ms: 20750\n  - text: \"Membuat hatimu lelah\"\n    start_ms: 25870\n";

        assert_eq!(
            yaml_lines(yaml),
            "Maafkan diriku yang salah\nMembuat hatimu lelah"
        );
    }

    #[test]
    fn normalization_ignores_features_and_punctuation() {
        assert_eq!(
            normalize("Menua Bersamamu (feat. Alma Ghinaya)"),
            normalize("Menua Bersamamu")
        );
        assert_eq!(normalize("Yellow"), normalize("yellow"));
    }

    #[test]
    fn lrclib_records_need_the_same_artist() {
        let hints = TrackHints {
            title: "Menua Bersamamu (feat. Alma Ghinaya)".into(),
            artist: "Ari Haryanto".into(),
            duration_secs: Some(213),
            ..TrackHints::default()
        };

        let cover = LrcLibRecord {
            track: Some("Menua Bersamamu".into()),
            artist: Some("Tri Suaka".into()),
            plain: Some("a".into()),
            synced: None,
            instrumental: false,
            duration: Some(252.0),
        };
        assert!(!cover.agrees_with(&hints));

        let same = LrcLibRecord {
            track: Some("Menua Bersamamu".into()),
            artist: Some("Ari Haryanto".into()),
            plain: Some("a".into()),
            synced: None,
            instrumental: false,
            duration: Some(252.0),
        };
        assert!(same.agrees_with(&hints));
    }

    #[test]
    fn records_are_reported_by_kind() {
        let synced = LrcLibRecord {
            track: None,
            artist: None,
            plain: Some("a\nb".into()),
            synced: Some("[00:01.00] a".into()),
            instrumental: false,
            duration: Some(120.0),
        }
        .into_lyrics()
        .expect("synced lyrics are valid");

        assert_eq!(synced.kind, "synced");
        assert!(!synced.is_empty());

        let instrumental = LrcLibRecord {
            track: None,
            artist: None,
            plain: None,
            synced: None,
            instrumental: true,
            duration: Some(120.0),
        }
        .into_lyrics()
        .expect("instrumental is a valid answer");

        assert!(instrumental.is_empty());
    }

    #[test]
    fn records_without_text_are_dropped() {
        let lyrics = LrcLibRecord {
            track: None,
            artist: None,
            plain: Some("   ".into()),
            synced: None,
            instrumental: false,
            duration: None,
        }
        .into_lyrics();

        assert!(lyrics.is_none());
    }
}
