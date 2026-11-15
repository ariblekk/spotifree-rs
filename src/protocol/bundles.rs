//! Parse Spotify's deployed web-player bundles.
//!
//! Spotify ships the persisted GraphQL hashes and the web-player TOTP secret
//! inside its JavaScript. Both change with every deployment, so they are
//! discovered at runtime instead of being hardcoded.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use regex::Regex;

use crate::error::{Error, Result};
use crate::protocol::routes::CDN_PREFIX;

/// Fields consumed from Spotify's base64 `appServerConfig` payload.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppServerConfig {
    pub client_version: String,
    pub build_version: String,
    #[serde(default)]
    pub recaptcha_web_player_fraud_site_key: String,
    #[serde(default)]
    pub correlation_id: String,
}

fn js_link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)["'](https?://[^"']+\.js(?:\?[^"']*)?)["']"#).unwrap())
}

fn secret_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)secret\s*:\s*["'](.*?)["']\s*,?\s*version\s*:\s*(\d+)"#).unwrap()
    })
}

fn query_hash_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""([^"]+)",\s*"(?:query|mutation)",\s*"([^"]+)""#).unwrap())
}

fn chunk_entry_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(\d+):"([^"]+)""#).unwrap())
}

/// Extract distinct Spotify web-player JavaScript URLs from the HTML shell.
pub fn extract_js_links(html: &str) -> Vec<String> {
    let mut links: Vec<String> = js_link_re()
        .captures_iter(html)
        .map(|caps| caps[1].to_string())
        .filter(|url| url.starts_with(CDN_PREFIX))
        .collect();

    links.sort();
    links.dedup();
    links
}

/// Decode the base64 `appServerConfig` embedded in the web-player HTML.
pub fn parse_app_server_config(html: &str) -> Result<AppServerConfig> {
    let tag = crate::protocol::routes::APP_CONFIG_TAG;
    let start = html
        .find(tag)
        .ok_or_else(|| Error::Protocol("appServerConfig tag not found in HTML".into()))?;

    let rest = &html[start + tag.len()..];
    let end = rest
        .find("</script>")
        .ok_or_else(|| Error::Protocol("unterminated appServerConfig tag".into()))?;

    let encoded = rest[..end].trim();
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|err| Error::Protocol(format!("appServerConfig is not valid base64: {err}")))?;

    Ok(serde_json::from_slice(&bytes)?)
}

/// Decode a base64 JSON payload embedded in a `<script id="...">` tag.
pub fn extract_script_base64(html: &str, tag_id: &str) -> Result<serde_json::Value> {
    let tag = format!(r#"id="{tag_id}""#);
    let start = html
        .find(&tag)
        .ok_or_else(|| Error::Protocol(format!("{tag_id} tag not found in HTML")))?;

    let rest = &html[start..];
    let tag_close = rest
        .find('>')
        .ok_or_else(|| Error::Protocol(format!("unclosed {tag_id} tag")))?;
    let content = &rest[tag_close + 1..];
    let end = content
        .find("</script>")
        .ok_or_else(|| Error::Protocol(format!("unterminated {tag_id} tag")))?;

    let encoded = content[..end].trim();
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|err| Error::Protocol(format!("{tag_id} is not valid base64: {err}")))?;

    Ok(serde_json::from_slice(&bytes)?)
}

/// Find the newest TOTP secret and its version in the web-player bundle.
pub fn extract_totp_secret(js: &str) -> Option<(String, u32)> {
    secret_re()
        .captures_iter(js)
        .filter_map(|caps| {
            let version = caps[2].parse::<u32>().ok()?;
            Some((caps[1].to_string(), version))
        })
        .max_by_key(|(_, version)| *version)
}

/// Collect persisted GraphQL operation hashes (`name -> sha256`).
pub fn extract_query_hashes(js: &str) -> HashMap<String, String> {
    query_hash_re()
        .captures_iter(js)
        .map(|caps| (caps[1].to_string(), caps[2].to_string()))
        .collect()
}

/// Build CDN URLs for route chunks whose name contains `needle`.
///
/// The web player names chunk files with a webpack filename function shaped
/// like `.u=e=>""+(({<names>})[e]||e)+"."+({<hashes>})[e]+".js"`. Route code
/// such as search lives in its own chunk, so the persisted hash for
/// `searchDesktop` is not in the main bundle.
pub fn chunk_urls(js: &str, needle: &str) -> Vec<String> {
    let Some(start) = js.find(".u=e=>").or_else(|| js.find(".u=function")) else {
        return Vec::new();
    };

    let window = &js[start..(start + 200_000).min(js.len())];
    let maps = object_maps(window);

    if maps.len() < 2 {
        return Vec::new();
    }

    let names = &maps[0];
    let hashes = &maps[1];

    let mut seen = HashSet::new();
    let mut urls = Vec::new();

    for (id, hash) in hashes {
        let name = names.get(id).cloned().unwrap_or_else(|| id.clone());

        if !name.contains(needle) {
            continue;
        }

        let url = format!("{CDN_PREFIX}{name}.{hash}.js");
        if seen.insert(url.clone()) {
            urls.push(url);
        }
    }

    urls
}

/// Collect the first two `{id:"value",...}` maps in `source`.
fn object_maps(source: &str) -> Vec<HashMap<String, String>> {
    let mut maps = Vec::new();
    let mut cursor = 0;

    while maps.len() < 2 {
        let Some(open) = source[cursor..].find('{').map(|offset| cursor + offset) else {
            break;
        };

        let Some(close) = source[open..].find('}').map(|offset| open + offset) else {
            break;
        };

        let mut map = HashMap::new();

        for caps in chunk_entry_re().captures_iter(&source[open..close]) {
            map.insert(caps[1].to_string(), caps[2].to_string());
        }

        if !map.is_empty() {
            maps.push(map);
        }

        cursor = close + 1;
    }

    maps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::totp::{decode_secret, generate_totp};

    #[test]
    fn extracts_persisted_hashes() {
        let js = r#""searchDesktop","query","abc","addToPlaylist","mutation","def""#;
        let hashes = extract_query_hashes(js);
        assert_eq!(hashes.get("searchDesktop").map(String::as_str), Some("abc"));
        assert_eq!(hashes.get("addToPlaylist").map(String::as_str), Some("def"));
    }

    #[test]
    fn extracts_totp_secret() {
        let js = r#"var a={secret:"AB",version:3};"#;
        assert_eq!(extract_totp_secret(js), Some(("AB".into(), 3)));
    }

    #[test]
    fn decodes_and_generates_totp() {
        let secret = decode_secret("AB");
        assert_eq!(generate_totp(&secret, 59).len(), 6);
    }

    #[test]
    fn resolves_search_chunk_urls() {
        let js = r#"u.e=e=>Promise.all([]),u.u=e=>""+(({4406:"xpui-routes-search",4935:"xpui-routes-album"})[e]||e)+"."+({4406:"aaaaaaaa",4935:"bbbbbbbb"})[e]+".js""#;
        let urls = chunk_urls(js, "xpui-routes-search");
        assert_eq!(
            urls,
            vec![format!("{CDN_PREFIX}xpui-routes-search.aaaaaaaa.js")]
        );
    }
}
