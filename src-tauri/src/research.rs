//! Local research protocol, page extraction, retrieval and prompt building.
//!
//! This module contains only deterministic, network-free logic so it can be
//! unit-tested without Tauri or a webview. The command layer owns every HTTP
//! request and still decides which URLs are allowed.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::ollama::{
    ChatMessage, MAX_CHAT_MESSAGES, MAX_MESSAGE_CHARS, MAX_MODEL_CHARS, MAX_RUN_ID_CHARS,
    MAX_TOTAL_CHARS,
};

/// The user runs SearXNG outside the app. JSON output must be enabled.
pub const SEARXNG_BASE_URL: &str = "http://127.0.0.1:8080";

pub const MAX_SEARCH_QUERY_CHARS: usize = 500;
pub const MAX_RESEARCH_PROMPT_CHARS: usize = 4_000;
pub const MAX_SEARCH_RESULTS: usize = 6;
pub const MAX_FETCHED_PAGES: usize = 4;
pub const MAX_PAGE_BYTES: usize = 2_500_000;
pub const MAX_PAGE_TEXT_CHARS: usize = 180_000;
pub const MAX_SOURCE_SNIPPET_CHARS: usize = 420;
pub const MAX_CONTEXT_CHUNKS: usize = 6;
pub const CHUNK_CHARS: usize = 1_400;
pub const CHUNK_OVERLAP: usize = 180;
pub const MAX_RESEARCH_HISTORY_MESSAGES: usize = 16;
pub const MAX_REDIRECTS: usize = 4;

const MAX_TITLE_CHARS: usize = 200;
const MAX_URL_CHARS: usize = 2_048;
const MAX_ENGINE_CHARS: usize = 80;

#[derive(Debug, Clone, Deserialize)]
pub struct SearxResponse {
    #[serde(default)]
    pub results: Vec<SearxResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearxResult {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResearchSource {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    /// SearXNG engine that produced this result, useful for transparency.
    pub engine: String,
    pub read: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartResearchRequest {
    pub run_id: String,
    pub model: String,
    pub query: String,
    #[serde(default)]
    pub history: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    Snippet,
    Page,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub source_id: u32,
    pub text: String,
    pub kind: ChunkKind,
}

pub fn validate_research_request(request: &StartResearchRequest) -> Result<(), String> {
    if request.run_id.is_empty()
        || request.run_id.chars().count() > MAX_RUN_ID_CHARS
        || !request
            .run_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err("the run id contains unsupported characters".to_owned());
    }
    if request.model.trim().is_empty()
        || request.model.chars().count() > MAX_MODEL_CHARS
        || !request
            .model
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | ':' | '-' | '_' | '/'))
    {
        return Err("the model name is invalid".to_owned());
    }
    let query = request.query.trim();
    let query_chars = query.chars().count();
    if query_chars < 2 {
        return Err("the research query is too short".to_owned());
    }
    if query_chars > MAX_RESEARCH_PROMPT_CHARS {
        return Err(format!(
            "the research query is limited to {MAX_RESEARCH_PROMPT_CHARS} characters"
        ));
    }

    if request.history.len() > MAX_RESEARCH_HISTORY_MESSAGES {
        return Err(format!(
            "a research run can carry at most {MAX_RESEARCH_HISTORY_MESSAGES} history messages"
        ));
    }
    let mut total_chars = query_chars;
    for (index, message) in request.history.iter().enumerate() {
        if !matches!(message.role.as_str(), "user" | "assistant") {
            return Err(format!(
                "history message {index} must be from the user or assistant"
            ));
        }
        if message.content.trim().is_empty() {
            return Err(format!("history message {index} is empty"));
        }
        let chars = message.content.chars().count();
        if chars > MAX_MESSAGE_CHARS {
            return Err(format!(
                "history message {index} exceeds the {MAX_MESSAGE_CHARS} character limit"
            ));
        }
        total_chars = total_chars.saturating_add(chars);
        if total_chars > MAX_TOTAL_CHARS {
            return Err(format!(
                "the conversation exceeds the {MAX_TOTAL_CHARS} character limit"
            ));
        }
    }

    if request.history.len() >= MAX_CHAT_MESSAGES {
        return Err("the research history is too long".to_owned());
    }
    Ok(())
}

/// Imperative and question preambles that add no value to a search engine.
/// Keep "when", "why", and "how" intact: they carry temporal or causal intent.
const SEARCH_PREFIXES: [&str; 20] = [
    "can you tell me about",
    "could you tell me about",
    "please tell me about",
    "search the web for",
    "please search for",
    "find out about",
    "tell me about",
    "please look up",
    "search for",
    "look up",
    "what are",
    "what is",
    "who was",
    "who is",
    "where is",
    "define",
    "explain",
    "please find",
    "show me",
    "find",
];

/// Strip a conversational preamble and trailing punctuation so that short
/// commands such as "find earth ?" search for the topic instead of the literal
/// instruction. The original prompt still goes to the model in full.
fn clean_search_text(prompt: &str) -> String {
    let mut text = prompt.trim();

    for prefix in SEARCH_PREFIXES {
        let Some(candidate) = text.get(..prefix.len()) else {
            continue;
        };
        if !candidate.eq_ignore_ascii_case(prefix) {
            continue;
        }
        let rest = &text[prefix.len()..];
        let boundary = rest
            .chars()
            .next()
            .is_none_or(|ch| ch.is_whitespace() || matches!(ch, ':' | ',' | ';' | '?' | '!'));
        if boundary {
            text = rest
                .trim_start_matches(|ch: char| ch.is_whitespace() || matches!(ch, ':' | ',' | ';'))
                .trim();
            break;
        }
    }

    text = text
        .trim_end_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, '?' | '!' | '.' | ',' | ':' | ';')
        })
        .trim();
    text.to_owned()
}

/// Search engines work best with a short, topical query. Long prompts still go
/// to the model in full, but the search request uses a word-boundary prefix.
pub fn search_query(prompt: &str) -> String {
    let trimmed = prompt.trim();
    let cleaned = clean_search_text(trimmed);
    let source = if cleaned.is_empty() {
        trimmed
    } else {
        cleaned.as_str()
    };

    if source.chars().count() <= MAX_SEARCH_QUERY_CHARS {
        return source.to_owned();
    }

    let mut query = String::new();
    for word in source.split_whitespace() {
        let extra = usize::from(!query.is_empty());
        if query.chars().count() + word.chars().count() + extra > MAX_SEARCH_QUERY_CHARS {
            break;
        }
        if !query.is_empty() {
            query.push(' ');
        }
        query.push_str(word);
    }

    if query.is_empty() {
        source.chars().take(MAX_SEARCH_QUERY_CHARS).collect()
    } else {
        query
    }
}

pub fn normalize_results(response: SearxResponse, limit: usize) -> Vec<SearchHit> {
    let limit = limit.min(MAX_SEARCH_RESULTS);
    let mut seen = HashSet::new();
    let mut hits = Vec::new();

    for result in response.results {
        if hits.len() >= limit {
            break;
        }
        let Ok(mut url) = Url::parse(result.url.trim()) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            continue;
        }
        if let Some(host) = url.host_str() {
            if is_blocked_hostname(host) {
                continue;
            }
            if let Some(ip) = host_ip(host) {
                if !is_public_ip(ip) {
                    continue;
                }
            }
        }
        url.set_fragment(None);
        let url = url.to_string();
        if url.chars().count() > MAX_URL_CHARS || !seen.insert(url.clone()) {
            continue;
        }

        let raw_title = clean_fragment(&result.title, MAX_TITLE_CHARS);
        let host = domain_for(&url);
        let title = if raw_title.is_empty() {
            host.clone()
        } else {
            raw_title
        };
        hits.push(SearchHit {
            title,
            url,
            snippet: clean_fragment(&result.content, MAX_SOURCE_SNIPPET_CHARS),
            engine: clean_fragment(&result.engine, MAX_ENGINE_CHARS),
        });
    }

    hits
}

/// Validate the shape of a URL before DNS resolution. Literal loopback/private
/// addresses are rejected here; hostnames are additionally resolved and checked
/// by the command layer immediately before the request.
pub fn validate_page_url(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| "the page URL is invalid".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http and https pages can be read".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URLs with embedded credentials are not allowed".to_owned());
    }
    if url.as_str().chars().count() > MAX_URL_CHARS {
        return Err("the page URL is too long".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "the page URL has no host".to_owned())?
        .to_ascii_lowercase();
    if host.is_empty() || is_blocked_hostname(&host) {
        return Err("that host is not available to the research reader".to_owned());
    }
    if let Some(ip) = host_ip(&host) {
        if !is_public_ip(ip) {
            return Err("the page URL points to a private address".to_owned());
        }
    }
    url.set_fragment(None);
    Ok(url)
}

pub fn is_blocked_hostname(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".lan")
        || host.ends_with(".home")
        || host == "metadata.google.internal"
}

fn host_ip(host: &str) -> Option<IpAddr> {
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    host.parse::<IpAddr>().ok()
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || ip.is_unspecified()
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

/// Extract a title and readable body from HTML without running scripts or
/// preserving markup. This intentionally favours robustness over browser-grade
/// parsing; every page is untrusted input.
pub fn extract_html(html: &str, fallback_title: &str) -> (String, String) {
    let title =
        extract_title(html).unwrap_or_else(|| clean_fragment(fallback_title, MAX_TITLE_CHARS));
    let body = decode_entities(&strip_markup(html));
    let body = strip_citation_markers(&body);
    let body = clean_lines(&body);
    let body = truncate_chars(&body, MAX_PAGE_TEXT_CHARS);
    (title, body)
}

/// Plain-text responses skip the HTML parser but still get the same entity and
/// whitespace normalization as extracted article text.
pub fn clean_plain_text(text: &str) -> String {
    let text = decode_entities(text);
    let text = strip_citation_markers(&text);
    let text = clean_lines(&text);
    truncate_chars(&text, MAX_PAGE_TEXT_CHARS)
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open = lower[start..].find('>')? + start + 1;
    let close = lower[open..].find("</title>")? + open;
    let title = clean_fragment(&html[open..close], MAX_TITLE_CHARS);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn strip_markup(html: &str) -> String {
    const SKIP_CONTENT: &[&str] = &[
        "script", "style", "noscript", "svg", "template", "nav", "header", "footer", "aside",
        "form", "button", "select", "option", "iframe",
    ];
    const BLOCK_TAGS: &[&str] = &[
        "address",
        "article",
        "blockquote",
        "br",
        "dd",
        "div",
        "dl",
        "dt",
        "figcaption",
        "figure",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
        "li",
        "main",
        "ol",
        "p",
        "pre",
        "section",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "tr",
        "ul",
    ];

    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() / 2);
    let mut pos = 0usize;

    while pos < bytes.len() {
        let Some(relative) = lower[pos..].find('<') else {
            out.push_str(&html[pos..]);
            break;
        };
        let lt = pos + relative;
        if lt > pos {
            out.push_str(&html[pos..lt]);
        }

        if lower[lt..].starts_with("<!--") {
            pos = lower[lt + 4..]
                .find("-->")
                .map_or(bytes.len(), |end| lt + 4 + end + 3);
            continue;
        }
        if lower[lt..].starts_with("<![cdata[") {
            let start = lt + 9;
            let end = lower[start..]
                .find("]]>")
                .map_or(bytes.len(), |value| start + value);
            out.push_str(&html[start..end]);
            pos = (end + 3).min(bytes.len());
            continue;
        }
        if lower[lt..].starts_with("<!") || lower[lt..].starts_with("<?") {
            pos = lower[lt..]
                .find('>')
                .map_or(bytes.len(), |end| lt + end + 1);
            continue;
        }

        let Some(close_relative) = lower[lt..].find('>') else {
            break;
        };
        let gt = lt + close_relative;
        let mut tag = lower[lt + 1..gt].trim();
        let closing = tag.starts_with('/');
        if closing {
            tag = tag[1..].trim_start();
        }
        let name = tag
            .split(|ch: char| ch.is_whitespace() || ch == '/' || ch == '>')
            .next()
            .unwrap_or_default();

        if !closing && SKIP_CONTENT.contains(&name) {
            let marker = format!("</{name}");
            let search_from = gt + 1;
            let end = lower[search_from..]
                .find(&marker)
                .and_then(|relative_end| {
                    lower[search_from + relative_end..]
                        .find('>')
                        .map(|close| search_from + relative_end + close + 1)
                })
                .unwrap_or(bytes.len());
            pos = end;
            continue;
        }

        if name == "br" || BLOCK_TAGS.contains(&name) {
            out.push('\n');
        } else {
            out.push(' ');
        }
        pos = gt + 1;
    }

    out
}

fn clean_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = false;
    for line in text.lines() {
        let line = collapse_whitespace(line);
        if line.is_empty() {
            if !blank && !out.is_empty() {
                out.push('\n');
            }
            blank = true;
            continue;
        }
        if line.chars().count() < 12 || !line.chars().any(|ch| ch.is_alphanumeric()) {
            continue;
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&line);
        blank = false;
    }
    out.trim().to_owned()
}

pub fn clean_fragment(text: &str, max_chars: usize) -> String {
    let text = decode_entities(text);
    let text = strip_citation_markers(&text);
    let text = collapse_whitespace(&strip_inline_tags(&text));
    truncate_chars(&text, max_chars)
}

/// Remove bracketed numeric references copied from source pages (for example
/// Wikipedia's `[12]`) so the model cannot mistake them for our own citation
/// labels. Genuine bracketed prose is left alone.
pub fn strip_citation_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '[' {
            out.push(ch);
            continue;
        }

        let mut inside = String::new();
        let mut closed = false;
        for next in chars.by_ref() {
            if next == ']' {
                closed = true;
                break;
            }
            inside.push(next);
            if inside.chars().count() > 32 {
                break;
            }
        }

        if closed && is_citation_body(&inside) {
            continue;
        }

        out.push('[');
        out.push_str(&inside);
        if closed {
            out.push(']');
        }
    }

    out
}

fn is_citation_body(inside: &str) -> bool {
    let trimmed = inside.trim();
    if trimmed.eq_ignore_ascii_case("edit") || trimmed.eq_ignore_ascii_case("citation needed") {
        return true;
    }
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == ',' || ch.is_whitespace())
        && trimmed.chars().any(|ch| ch.is_ascii_digit())
}

fn strip_inline_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let Some(semi) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        if semi > 12 {
            out.push('&');
            rest = &tail[1..];
            continue;
        }
        let entity = &tail[1..semi];
        if let Some(decoded) = decode_entity(entity) {
            out.push(decoded);
            rest = &tail[semi + 1..];
        } else {
            out.push('&');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    if let Some(dec) = entity.strip_prefix('#') {
        return dec.parse::<u32>().ok().and_then(char::from_u32);
    }
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "mdash" => Some('—'),
        "ndash" => Some('–'),
        "hellip" => Some('…'),
        "lsquo" => Some('‘'),
        "rsquo" => Some('’'),
        "ldquo" => Some('“'),
        "rdquo" => Some('”'),
        "copy" => Some('©'),
        "reg" => Some('®'),
        "trade" => Some('™'),
        _ => None,
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    if let Some(index) = out.rfind(char::is_whitespace) {
        if out[..index].chars().count() > max_chars / 2 {
            out.truncate(index);
        }
    }
    out.trim_end().to_owned()
}

pub fn chunk_text(text: &str, max_chars: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let max_chars = max_chars.max(200);
    let mut chunks = Vec::new();
    let mut start = 0usize;

    while start < chars.len() {
        let mut end = (start + max_chars).min(chars.len());
        if end < chars.len() {
            let floor = start + max_chars.saturating_sub(220);
            let mut cut = end;
            while cut > floor {
                if chars[cut - 1].is_whitespace() {
                    end = cut;
                    break;
                }
                cut -= 1;
            }
        }
        if end <= start {
            end = (start + max_chars).min(chars.len());
        }

        let chunk: String = chars[start..end].iter().collect();
        let chunk = chunk.trim();
        if chunk.chars().count() >= 40 {
            chunks.push(chunk.to_owned());
        }
        if end >= chars.len() {
            break;
        }
        let next = end.saturating_sub(overlap);
        start = next.max(start + 1);
    }

    chunks
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter_map(|word| {
            let word = word.to_lowercase();
            if word.chars().count() < 2 || is_stop_word(&word) {
                None
            } else {
                Some(word)
            }
        })
        .collect()
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "and"
            | "for"
            | "are"
            | "but"
            | "not"
            | "you"
            | "all"
            | "can"
            | "her"
            | "was"
            | "one"
            | "our"
            | "out"
            | "his"
            | "has"
            | "had"
            | "how"
            | "its"
            | "who"
            | "did"
            | "yes"
            | "this"
            | "that"
            | "with"
            | "from"
            | "they"
            | "will"
            | "would"
            | "there"
            | "their"
            | "what"
            | "when"
            | "where"
            | "which"
            | "while"
            | "about"
            | "into"
            | "than"
            | "then"
            | "them"
            | "these"
            | "those"
            | "have"
            | "been"
            | "were"
            | "your"
            | "does"
            | "using"
            | "use"
    )
}

/// Rank chunks against the query with BM25, then cap the number of excerpts per
/// source so one long page cannot crowd out every other result.
pub fn rank_chunks(query: &str, chunks: &[Chunk], limit: usize) -> Vec<usize> {
    if chunks.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut terms: Vec<String> = tokenize(query);
    terms.sort();
    terms.dedup();
    if terms.is_empty() {
        return fallback_chunks(chunks, limit);
    }

    let documents: Vec<Vec<String>> = chunks.iter().map(|chunk| tokenize(&chunk.text)).collect();
    let average_length = documents
        .iter()
        .map(|document| document.len() as f64)
        .sum::<f64>()
        / documents.len() as f64;
    let document_count = documents.len() as f64;

    let document_frequency: HashMap<&str, usize> = terms
        .iter()
        .map(|term| {
            let count = documents
                .iter()
                .filter(|document| document.iter().any(|word| word == term))
                .count();
            (term.as_str(), count)
        })
        .collect();

    let mut scored: Vec<(usize, f64)> = documents
        .iter()
        .enumerate()
        .map(|(index, document)| {
            let length = document.len() as f64;
            let mut score = 0.0;
            for term in &terms {
                let frequency = document.iter().filter(|word| *word == term).count() as f64;
                if frequency == 0.0 {
                    continue;
                }
                let df = *document_frequency.get(term.as_str()).unwrap_or(&0) as f64;
                let idf = ((document_count - df + 0.5) / (df + 0.5) + 1.0).ln();
                let denominator = frequency + 1.2 * (1.0 - 0.75 + 0.75 * length / average_length);
                score += idf * (frequency * 2.2) / denominator;
            }
            (index, score)
        })
        .collect();

    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });

    let mut per_source: HashMap<u32, usize> = HashMap::new();
    let mut selected = Vec::new();
    for (index, score) in scored {
        if score <= 0.0 {
            continue;
        }
        let source = chunks[index].source_id;
        let count = per_source.entry(source).or_default();
        if *count >= 2 {
            continue;
        }
        *count += 1;
        selected.push(index);
        if selected.len() >= limit {
            break;
        }
    }

    if selected.is_empty() {
        fallback_chunks(chunks, limit)
    } else {
        selected
    }
}

fn fallback_chunks(chunks: &[Chunk], limit: usize) -> Vec<usize> {
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        if seen.insert(chunk.source_id) {
            selected.push(index);
            if selected.len() >= limit {
                break;
            }
        }
    }
    if selected.is_empty() {
        (0..chunks.len().min(limit)).collect()
    } else {
        selected
    }
}

pub fn build_context(sources: &[ResearchSource], chunks: &[Chunk], selected: &[usize]) -> String {
    let mut by_source: HashMap<u32, Vec<&Chunk>> = HashMap::new();
    for index in selected {
        if let Some(chunk) = chunks.get(*index) {
            by_source.entry(chunk.source_id).or_default().push(chunk);
        }
    }

    let mut context = String::new();
    for source in sources {
        context.push_str(&format!(
            "[{}] {}\nURL: {}\n",
            source.id, source.title, source.url
        ));
        if !source.snippet.is_empty() {
            context.push_str(&format!("Search snippet: {}\n", source.snippet));
        }
        if let Some(source_chunks) = by_source.get(&source.id) {
            for chunk in source_chunks {
                // The search snippet is already included above; only page
                // excerpts need a second line here.
                if chunk.kind == ChunkKind::Snippet {
                    continue;
                }
                context.push_str(&format!("Page excerpt: {}\n", chunk.text));
            }
        }
        context.push('\n');
    }
    context.trim_end().to_owned()
}

pub fn build_system_prompt(context: &str, source_count: usize) -> String {
    let labels = (1..=source_count)
        .map(|id| format!("[{id}]"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "You are a local research assistant. Answer the user's question using only the numbered sources below.\n\
         The user has already run a web search, and the numbered sources are its results. Do not say you cannot search the web, access the internet, or perform real-world actions, and do not talk about being a language model. Treat a short or imperative question (for example \"find X\") as a request for factual information about X.\n\
         There are exactly {source_count} sources. The only valid citation labels are {labels}.\n\
         Cite factual claims with a label immediately after the claim, for example [1]. You may cite several sources in one group, for example [1, 2].\n\
         Never cite a number outside that set, and never treat a bracketed number inside source text as a citation label.\n\
         Do not end with a \"Sources\" section, bibliography, or list of source titles and URLs. The app already displays the numbered sources in the left Sources panel; keep only inline citations in the answer.\n\
         Base every factual claim on the sources. If they are unrelated or do not contain enough information, say briefly that the search did not return usable information, name what is missing, and do not invent an answer or cite unrelated sources.\n\
         Never invent a source, URL, quote, or fact. Do not use outside knowledge as a source.\n\
         Keep the answer concise, well organized, and in the same language as the question.\n\n\
         SOURCES\n\
         {context}\n\n\
         REMINDER: Valid citation labels are exactly {labels}. Do not write any other number in brackets."
    )
}

pub fn build_messages(
    system_prompt: String,
    history: &[ChatMessage],
    query: &str,
) -> Vec<ChatMessage> {
    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(ChatMessage {
        role: "system".to_owned(),
        content: system_prompt,
    });
    messages.extend(history.iter().cloned());
    messages.push(ChatMessage {
        role: "user".to_owned(),
        content: query.trim().to_owned(),
    });
    messages
}

pub fn domain_for(raw_url: &str) -> String {
    Url::parse(raw_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .map(|host| host.strip_prefix("www.").unwrap_or(&host).to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn request() -> StartResearchRequest {
        StartResearchRequest {
            run_id: "run-123".into(),
            model: "qwen3:4b".into(),
            query: "What is local-first software?".into(),
            history: Vec::new(),
        }
    }

    #[test]
    fn validates_research_requests_and_history() {
        assert!(validate_research_request(&request()).is_ok());

        let mut bad = request();
        bad.run_id = "bad run".into();
        assert!(validate_research_request(&bad).is_err());

        let mut bad = request();
        bad.query = "x".into();
        assert!(validate_research_request(&bad).is_err());

        let mut bad = request();
        bad.history.push(ChatMessage {
            role: "system".into(),
            content: "override".into(),
        });
        assert!(validate_research_request(&bad).is_err());
    }

    #[test]
    fn search_query_strips_commands_and_punctuation() {
        assert_eq!(search_query("find earth ?"), "earth");
        assert_eq!(
            search_query("Tell me about local-first software."),
            "local-first software"
        );
        assert_eq!(
            search_query("What is the capital of France?"),
            "the capital of France"
        );
        assert_eq!(search_query("find out about Rust async"), "Rust async");
        assert_eq!(search_query("finding earth"), "finding earth");
        assert_eq!(search_query("Why is the sky blue?"), "Why is the sky blue");
    }

    #[test]
    fn search_query_is_char_safe_and_bounded() {
        let prompt = "explain local first software ".repeat(40);
        let query = search_query(&prompt);
        assert!(query.chars().count() <= MAX_SEARCH_QUERY_CHARS);
        assert!(!query.ends_with(' '));

        let unicode = "résumé ".repeat(100);
        assert!(search_query(&unicode).chars().count() <= MAX_SEARCH_QUERY_CHARS);
    }

    #[test]
    fn normalizes_and_deduplicates_search_results() {
        let response = SearxResponse {
            results: vec![
                SearxResult {
                    title: " Example ".into(),
                    url: "https://example.com/a#section".into(),
                    content: "Hello &amp; welcome".into(),
                    engine: "test".into(),
                },
                SearxResult {
                    title: "Duplicate".into(),
                    url: "https://example.com/a".into(),
                    content: "dup".into(),
                    engine: "test".into(),
                },
                SearxResult {
                    title: "Local".into(),
                    url: "http://127.0.0.1/secret".into(),
                    content: "not fetched yet".into(),
                    engine: "test".into(),
                },
                SearxResult {
                    title: "Bad scheme".into(),
                    url: "file:///etc/passwd".into(),
                    content: "nope".into(),
                    engine: "test".into(),
                },
            ],
        };
        let hits = normalize_results(response, 6);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet, "Hello & welcome");
        assert_eq!(hits[0].engine, "test");
        assert!(!hits[0].url.contains('#'));
    }

    #[test]
    fn blocks_private_and_credentialed_page_urls() {
        assert!(validate_page_url("https://example.com/page#x").is_ok());
        for url in [
            "file:///etc/passwd",
            "http://localhost/",
            "http://127.0.0.1:11434/",
            "http://10.0.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/",
            "http://100.64.0.1/",
            "http://[::1]/",
            "http://[fc00::1]/",
            "https://user:pass@example.com/",
            "http://metadata.google.internal/",
        ] {
            assert!(validate_page_url(url).is_err(), "{url} should be blocked");
        }
    }

    #[test]
    fn classifies_public_and_private_ips() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.1.1",
            "100.64.1.1",
            "224.0.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} should be private");
        }
        for ip in ["8.8.8.8", "1.1.1.1", "2001:4860:4860::8888"] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip} should be public");
        }
    }

    #[test]
    fn extracts_readable_text_and_title() {
        let html = r#"
            <html><head><title>Local &amp; First</title>
            <style>.hidden { display:none }</style>
            <script>alert('ignore me')</script></head>
            <body><nav>home about contact</nav>
            <article><h1>Local-first</h1>
            <p>Local-first software keeps your data on your device &amp; gives you ownership.</p>
            <p>It is not the same as offline-only software.</p></article>
            <footer>copyright</footer></body></html>
        "#;
        let (title, text) = extract_html(html, "fallback");
        assert_eq!(title, "Local & First");
        assert!(text.contains("keeps your data on your device & gives you ownership"));
        assert!(text.contains("not the same as offline-only"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("copyright"));
        assert!(!text.contains("contact"));
    }

    #[test]
    fn strips_source_citation_markers_without_losing_prose() {
        assert_eq!(
            strip_citation_markers("Local-first [1] software [12] is collaborative [edit]."),
            "Local-first  software  is collaborative ."
        );
        assert_eq!(
            strip_citation_markers("Keep [2024] planning notes and [the draft]."),
            "Keep  planning notes and [the draft]."
        );
    }

    #[test]
    fn prompt_names_only_the_available_citation_labels() {
        let prompt = build_system_prompt("[1] A\n[2] B", 2);
        assert!(prompt.contains("There are exactly 2 sources"));
        assert!(prompt.contains("[1], [2]"));
        assert!(prompt.contains("already run a web search"));
        assert!(prompt.contains("Treat a short or imperative question"));
        assert!(prompt.contains("Do not write any other number in brackets"));
        assert!(prompt.contains("Do not end with a \"Sources\" section"));

        let single = build_system_prompt("[1] A", 1);
        assert!(single.contains("The only valid citation labels are [1]."));
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(clean_fragment("caf&#233; &#x2014; ok", 100), "café — ok");
    }

    #[test]
    fn chunks_unicode_text_without_losing_the_tail() {
        let text = "naïve café ".repeat(500);
        let chunks = chunk_text(&text, 400, 80);
        assert!(chunks.len() > 2);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 400));
        assert!(chunks.last().unwrap().contains("café"));
    }

    #[test]
    fn ranks_relevant_chunks_and_limits_per_source() {
        let chunks = vec![
            Chunk {
                source_id: 1,
                text: "Local-first software keeps data on the device and supports ownership."
                    .into(),
                kind: ChunkKind::Snippet,
            },
            Chunk {
                source_id: 2,
                text: "A recipe for chocolate cake with butter and sugar.".into(),
                kind: ChunkKind::Snippet,
            },
            Chunk {
                source_id: 1,
                text: "Local-first software also works offline and syncs without a central server."
                    .into(),
                kind: ChunkKind::Page,
            },
        ];
        let ranked = rank_chunks("What is local-first software?", &chunks, 3);
        assert!(!ranked.is_empty());
        assert!(ranked.contains(&0));
        assert!(!ranked.contains(&1));
    }

    #[test]
    fn builds_context_and_messages_without_a_system_override() {
        let sources = vec![ResearchSource {
            id: 1,
            title: "Example".into(),
            url: "https://example.com".into(),
            domain: "example.com".into(),
            snippet: "A snippet".into(),
            engine: "test".into(),
            read: true,
        }];
        let chunks = vec![Chunk {
            source_id: 1,
            text: "A longer page excerpt.".into(),
            kind: ChunkKind::Page,
        }];
        let context = build_context(&sources, &chunks, &[0]);
        assert!(context.contains("[1] Example"));
        assert!(context.contains("A longer page excerpt."));
        let messages = build_messages(
            build_system_prompt(&context, sources.len()),
            &[],
            "What is it?",
        );
        assert_eq!(messages.first().unwrap().role, "system");
        assert_eq!(messages.last().unwrap().role, "user");
        assert!(messages.last().unwrap().content.contains("What is it?"));

        // The research path has its own system prompt and still goes through
        // the shared request validator before streaming.
        let request = crate::ollama::StartChatRequest {
            run_id: "run-123".into(),
            model: "qwen3:4b".into(),
            messages,
        };
        assert!(crate::ollama::validate_chat_request(&request).is_ok());
    }
}
