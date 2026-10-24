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
/// Results requested from each planned sub-query.
pub const MAX_SEARCH_RESULTS: usize = 6;
/// Unique candidate sources kept after merging the sub-query result sets.
pub const MAX_CANDIDATE_SOURCES: usize = 12;
pub const MAX_FETCHED_PAGES: usize = 6;
/// Extra pages read only when the first evidence pass is weak.
pub const MAX_RETRY_PAGES: usize = 2;
pub const MAX_PAGE_BYTES: usize = 2_500_000;
pub const MAX_PAGE_TEXT_CHARS: usize = 180_000;
pub const MAX_SOURCE_SNIPPET_CHARS: usize = 420;
pub const MAX_CONTEXT_CHUNKS: usize = 8;
pub const CHUNK_CHARS: usize = 1_400;
pub const CHUNK_OVERLAP: usize = 180;
pub const MAX_RESEARCH_HISTORY_MESSAGES: usize = 16;
pub const MAX_REDIRECTS: usize = 4;
pub const MAX_PLANNED_QUERIES: usize = 3;

/// SearXNG engines used for ordinary web research. The instance is still free
/// to add or remove engines; a restricted query falls back to the general
/// category when the route returns nothing.
const GENERAL_ENGINES: [&str; 7] = [
    "brave",
    "duckduckgo",
    "mwmbl",
    "wiby",
    "wikidata",
    "wikipedia",
    "yep",
];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryIntent {
    Factual,
    Current,
    HowTo,
    Comparison,
    Technical,
    General,
}

impl QueryIntent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Factual => "factual",
            Self::Current => "current",
            Self::HowTo => "how-to",
            Self::Comparison => "comparison",
            Self::Technical => "technical",
            Self::General => "general",
        }
    }
}

/// One planned search request. The command layer sends only these strings to
/// the fixed local SearXNG endpoint; the webview cannot supply engines or time
/// ranges.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedQuery {
    pub query: String,
    pub engines: Vec<String>,
    pub purpose: String,
    pub time_range: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResearchPlan {
    pub intent: QueryIntent,
    pub queries: Vec<PlannedQuery>,
}

/// A local, deterministic assessment of whether the retrieved text actually
/// covers the question and comes from more than one independent domain.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceReport {
    pub source_count: usize,
    pub chunk_count: usize,
    pub read_count: usize,
    pub domain_count: usize,
    pub coverage: f64,
}

impl EvidenceReport {
    pub fn coverage_percent(&self) -> u32 {
        let percent = (self.coverage * 100.0).round();
        if percent.is_finite() {
            percent.clamp(0.0, 100.0) as u32
        } else {
            0
        }
    }

    /// Thin evidence is not fatal: the answer prompt is told to qualify it.
    /// It is, however, enough to justify one broader retrieval pass.
    pub fn is_weak(&self) -> bool {
        self.source_count == 0
            || self.chunk_count == 0
            || self.read_count == 0
            || self.domain_count < 2
            || self.coverage < 0.30
    }

    pub fn summary(&self) -> String {
        format!(
            "{}/{} pages · {} domains · {}% coverage",
            self.read_count,
            self.source_count,
            self.domain_count,
            self.coverage_percent()
        )
    }
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

/// Build a small, local research plan. This is deliberately deterministic:
/// classification is cheap, works without a second model call, and keeps the
/// retrieval stage testable. The plan still includes several searches and a
/// different engine route for each kind of question.
pub fn plan_research(prompt: &str) -> ResearchPlan {
    let intent = detect_intent(prompt);
    let base = search_query(prompt);
    let mut plan = ResearchPlan {
        intent,
        queries: Vec::new(),
    };
    if base.is_empty() {
        return plan;
    }

    push_planned(
        &mut plan,
        &base,
        engines_for(intent, "primary"),
        "primary",
        time_range_for(intent, "primary"),
    );

    let focus = focus_query(prompt);
    if !focus.is_empty() {
        push_planned(
            &mut plan,
            &focus,
            engines_for(intent, "focus"),
            "focus",
            None,
        );
    }

    if plan.queries.len() < MAX_PLANNED_QUERIES {
        let (variant, purpose) = variant_query(&base, intent);
        if !variant.is_empty() {
            push_planned(
                &mut plan,
                &variant,
                engines_for(intent, purpose),
                purpose,
                time_range_for(intent, purpose),
            );
        }
    }

    // Very short questions (for example "earth") do not produce a useful focus
    // query. Add one broader variant so even those get a second retrieval path.
    if plan.queries.len() == 1 {
        let fallback = format!("{base} overview");
        push_planned(
            &mut plan,
            &fallback,
            engines_for(intent, "variant"),
            "overview",
            None,
        );
    }

    plan
}

fn detect_intent(prompt: &str) -> QueryIntent {
    let lower = prompt.to_lowercase();
    let words = words(&lower);
    let has = |term: &str| words.iter().any(|word| word == term);
    let has_any = |terms: &[&str]| terms.iter().any(|term| has(term));
    let phrase = |needle: &str| lower.contains(needle);

    if has_any(&[
        "vs",
        "versus",
        "compare",
        "comparison",
        "difference",
        "differences",
        "better",
        "worse",
        "pros",
        "cons",
    ]) || phrase("compared to")
    {
        QueryIntent::Comparison
    } else if has_any(&[
        "latest",
        "today",
        "current",
        "currently",
        "news",
        "recent",
        "recently",
        "now",
        "price",
        "prices",
        "weather",
        "stock",
        "release",
    ]) {
        QueryIntent::Current
    } else if phrase("how to")
        || has_any(&[
            "guide",
            "tutorial",
            "install",
            "setup",
            "configure",
            "fix",
            "steps",
            "step",
        ])
    {
        QueryIntent::HowTo
    } else if has_any(&[
        "what",
        "who",
        "when",
        "where",
        "why",
        "which",
        "define",
        "definition",
        "meaning",
        "history",
        "capital",
        "population",
        "explain",
    ]) {
        QueryIntent::Factual
    } else if has_any(&[
        "rust",
        "python",
        "typescript",
        "javascript",
        "api",
        "code",
        "cargo",
        "npm",
        "server",
        "database",
        "sql",
        "linux",
        "windows",
        "macos",
        "docker",
        "git",
        "http",
        "json",
        "algorithm",
        "programming",
        "software",
        "framework",
        "library",
    ]) {
        QueryIntent::Technical
    } else {
        QueryIntent::General
    }
}

fn words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn focus_query(prompt: &str) -> String {
    let mut selected: Vec<String> = Vec::new();
    for term in tokenize(prompt) {
        if selected.iter().any(|existing| existing == &term) {
            continue;
        }
        selected.push(term);
        if selected.len() >= 8 {
            break;
        }
    }

    if selected.len() < 3 {
        String::new()
    } else {
        selected.join(" ")
    }
}

fn variant_query(base: &str, intent: QueryIntent) -> (String, &'static str) {
    let lower = base.to_lowercase();
    match intent {
        QueryIntent::Factual if !lower.contains("definition") && !lower.contains("meaning") => {
            (format!("{base} definition"), "variant")
        }
        QueryIntent::Current if !lower.contains("latest") && !lower.contains("news") => {
            (format!("{base} latest"), "variant")
        }
        QueryIntent::HowTo if !lower.contains("guide") && !lower.contains("tutorial") => {
            (format!("{base} guide"), "variant")
        }
        QueryIntent::Comparison if !lower.contains("comparison") => {
            (format!("{base} comparison"), "variant")
        }
        QueryIntent::Technical if !lower.contains("documentation") && !lower.contains("docs") => {
            (format!("{base} documentation"), "variant")
        }
        QueryIntent::General if !lower.contains("overview") => {
            (format!("{base} overview"), "variant")
        }
        _ => (String::new(), "variant"),
    }
}

fn engines_for(intent: QueryIntent, purpose: &str) -> Vec<String> {
    let names: &[&str] = match (intent, purpose) {
        (QueryIntent::Factual, "variant") => &["wikipedia", "wikidata", "brave"],
        (QueryIntent::Current, "variant") => &["brave", "duckduckgo", "yep"],
        (QueryIntent::HowTo, "variant") => &["duckduckgo", "brave", "mwmbl"],
        (QueryIntent::Comparison, "variant") => &["brave", "duckduckgo", "wikipedia", "yep"],
        (QueryIntent::Technical, "variant") => &["duckduckgo", "brave", "mwmbl", "wikipedia"],
        _ => match intent {
            QueryIntent::Factual => &["wikipedia", "wikidata", "brave", "duckduckgo"],
            QueryIntent::Current => &["brave", "duckduckgo", "yep", "wikipedia"],
            QueryIntent::HowTo => &["duckduckgo", "brave", "mwmbl", "wikipedia"],
            QueryIntent::Comparison => &["brave", "duckduckgo", "wikipedia", "yep"],
            QueryIntent::Technical => &["duckduckgo", "brave", "mwmbl", "wikipedia"],
            QueryIntent::General => &GENERAL_ENGINES[..],
        },
    };

    names.iter().map(|name| (*name).to_owned()).collect()
}

fn time_range_for(intent: QueryIntent, purpose: &str) -> Option<String> {
    match (intent, purpose) {
        (QueryIntent::Current, "primary") => Some("year".to_owned()),
        (QueryIntent::Current, "variant") => Some("month".to_owned()),
        _ => None,
    }
}

fn push_planned(
    plan: &mut ResearchPlan,
    query: &str,
    engines: Vec<String>,
    purpose: &str,
    time_range: Option<String>,
) {
    if plan.queries.len() >= MAX_PLANNED_QUERIES {
        return;
    }
    let query = query.trim();
    let chars = query.chars().count();
    if !(2..=MAX_SEARCH_QUERY_CHARS).contains(&chars) {
        return;
    }
    let key = query_key(query);
    if key.is_empty()
        || plan
            .queries
            .iter()
            .any(|existing| query_key(&existing.query) == key)
    {
        return;
    }

    plan.queries.push(PlannedQuery {
        query: query.to_owned(),
        engines,
        purpose: purpose.to_owned(),
        time_range,
    });
}

fn query_key(query: &str) -> String {
    let mut terms = tokenize(query);
    terms.sort();
    terms.dedup();
    if terms.is_empty() {
        query.trim().to_lowercase()
    } else {
        terms.join(" ")
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

/// Readable characters kept from one built-in search result page.
pub const MAX_SEARCH_HTML_CHARS: usize = 1_500_000;

/// Parse DuckDuckGo's HTML endpoint into search results.
///
/// The built-in search exists so research works without a separately installed
/// SearXNG. It reads the same public result page a browser would, so the markup
/// is parsed defensively: anything unrecognised is skipped rather than failing
/// the run.
pub fn parse_duckduckgo_html(html: &str) -> Vec<SearxResult> {
    let mut results = Vec::new();
    for block in html.split("class=\"result results_links\"").skip(1) {
        let Some((href, title)) = parse_anchor(block, "class=\"result__a\"") else {
            continue;
        };
        let url = unwrap_duckduckgo_link(&href);
        if url.is_empty() {
            continue;
        }
        let content = parse_snippet(block, "class=\"result__snippet\"", "</a>")
            .or_else(|| parse_snippet(block, "class=\"result__snippet\"", "</div>"))
            .unwrap_or_default();
        results.push(SearxResult {
            title,
            url,
            content,
            engine: "duckduckgo".to_owned(),
        });
    }
    results
}

/// Parse Bing's organic result list. Used when DuckDuckGo throttles the request.
pub fn parse_bing_html(html: &str) -> Vec<SearxResult> {
    let mut results = Vec::new();
    for block in html.split("<li class=\"b_algo\"").skip(1) {
        let Some((href, title)) = parse_anchor(block, "<h2") else {
            continue;
        };
        if href.is_empty() {
            continue;
        }
        let content = parse_snippet(block, "<p", "</p>").unwrap_or_default();
        results.push(SearxResult {
            title,
            url: href,
            content,
            engine: "bing".to_owned(),
        });
    }
    results
}

/// Return `(href, text)` for the first anchor after `marker`.
fn parse_anchor(chunk: &str, marker: &str) -> Option<(String, String)> {
    let start = chunk.find(marker)?;
    let window = &chunk[start..];
    let href_at = window.find("href=\"")?;
    let after_href = &window[href_at + "href=\"".len()..];
    let href_end = after_href.find('"')?;
    let href = decode_entities(&after_href[..href_end]);
    let after_open = &after_href[href_end..];
    let text_start = after_open.find('>')? + 1;
    let text = &after_open[text_start..];
    let text_end = text.find("</a>")?;
    Some((href, strip_tags(&text[..text_end])))
}

/// Return the visible text that follows `marker` up to `terminator`.
fn parse_snippet(chunk: &str, marker: &str, terminator: &str) -> Option<String> {
    let start = chunk.find(marker)?;
    let window = &chunk[start..];
    let text_start = window.find('>')? + 1;
    let text = &window[text_start..];
    let text_end = text.find(terminator)?;
    let snippet = strip_tags(&text[..text_end]);
    if snippet.is_empty() {
        None
    } else {
        Some(snippet)
    }
}

/// DuckDuckGo wraps outbound links in `/l/?uddg=<percent-encoded target>`.
fn unwrap_duckduckgo_link(href: &str) -> String {
    if let Some(index) = href.find("uddg=") {
        let rest = &href[index + "uddg=".len()..];
        let end = rest.find('&').unwrap_or(rest.len());
        return percent_decode(&rest[..end]);
    }
    if let Some(stripped) = href.strip_prefix("//") {
        return format!("https://{stripped}");
    }
    href.to_owned()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let entity_end = tail.find(';').filter(|end| *end <= 10);
        let Some(end) = entity_end else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" | "#x27" | "#X27" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|dec| dec.parse::<u32>().ok())
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_tags(input: &str) -> String {
    let mut text = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    decode_entities(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A stable key for duplicate detection. Tracking parameters, fragments, a
/// leading `www.`, a trailing slash, and the http/https choice do not change the
/// key, so the same article found by several engines is read once.
pub fn canonical_url_key(raw: &str) -> String {
    let Ok(url) = Url::parse(raw.trim()) else {
        return String::new();
    };
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    if host.is_empty() {
        return String::new();
    }

    let port = url
        .port()
        .map(|value| format!(":{value}"))
        .unwrap_or_default();
    let path = url.path().trim_end_matches('/');
    let mut key = format!("{host}{port}{path}");

    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(name, _)| !is_tracking_parameter(name))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.into_owned()))
        .collect();
    pairs.sort();
    if !pairs.is_empty() {
        key.push('?');
        for (index, (name, value)) in pairs.iter().enumerate() {
            if index > 0 {
                key.push('&');
            }
            key.push_str(name);
            key.push('=');
            key.push_str(value);
        }
    }

    key
}

fn is_tracking_parameter(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("utm_")
        || matches!(
            name.as_str(),
            "fbclid"
                | "gclid"
                | "dclid"
                | "msclkid"
                | "mc_cid"
                | "mc_eid"
                | "ref"
                | "ref_src"
                | "ref_url"
                | "source"
                | "spm"
                | "igshid"
                | "yclid"
                | "mkt_tok"
        )
}

pub fn dedupe_hits(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut unique: Vec<SearchHit> = Vec::new();

    for hit in hits {
        let key = canonical_url_key(&hit.url);
        if key.is_empty() {
            continue;
        }
        if let Some(index) = seen.get(&key).copied() {
            if let Some(existing) = unique.get_mut(index) {
                if existing.snippet.chars().count() < hit.snippet.chars().count() {
                    existing.snippet = hit.snippet;
                }
                if existing.title == domain_for(&existing.url) && !hit.title.is_empty() {
                    existing.title = hit.title;
                }
                if existing.url.starts_with("http://") && hit.url.starts_with("https://") {
                    existing.url = hit.url;
                }
                if existing.engine.is_empty() {
                    existing.engine = hit.engine;
                }
            }
            continue;
        }
        seen.insert(key, unique.len());
        unique.push(hit);
    }

    unique
}

/// Merge and rank candidate sources. Relevance is weighted by title, snippet,
/// URL, engine trust and a small set of domain-quality signals. Selection is
/// then diversified by domain and engine so one site cannot fill the panel.
pub fn rank_hits(query: &str, hits: Vec<SearchHit>, limit: usize) -> Vec<SearchHit> {
    if hits.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut terms = tokenize(query);
    terms.sort();
    terms.dedup();

    let deduped = dedupe_hits(hits);
    let required_terms = terms.len().min(2);
    let mut filtered: Vec<SearchHit> = deduped
        .iter()
        .filter(|hit| hit_term_matches(&terms, hit) >= required_terms)
        .cloned()
        .collect();
    // Do not throw away a small result set just because every result is a weak
    // lexical match; the answer prompt will still be told to qualify it.
    if filtered.is_empty() {
        filtered = deduped;
    }

    let mut remaining: Vec<(SearchHit, f64)> = filtered
        .into_iter()
        .map(|hit| {
            let mut score = hit_relevance(&terms, &hit)
                + engine_trust(&hit.engine) * 0.15
                + domain_quality(&hit.url);
            if !hit.snippet.is_empty() {
                score += 0.05;
            }
            (hit, score)
        })
        .collect();
    remaining.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.url.cmp(&right.0.url))
    });

    let limit = limit.min(remaining.len());
    let mut selected = Vec::with_capacity(limit);
    let mut domain_counts: HashMap<String, usize> = HashMap::new();
    let mut engine_counts: HashMap<String, usize> = HashMap::new();

    while selected.len() < limit && !remaining.is_empty() {
        let mut best_index: Option<usize> = None;
        let mut best_score = f64::NEG_INFINITY;

        for (index, (hit, base_score)) in remaining.iter().enumerate() {
            let domain = domain_for(&hit.url);
            let domain_count = *domain_counts.get(&domain).unwrap_or(&0);
            if domain_count >= 2 {
                continue;
            }
            let engine = hit.engine.to_ascii_lowercase();
            let engine_count = *engine_counts.get(&engine).unwrap_or(&0);
            let diversity = if domain_count == 0 { 0.18 } else { 0.0 }
                + if engine_count == 0 { 0.06 } else { 0.0 };
            let score = *base_score + diversity;
            if best_index.is_none() || score > best_score {
                best_index = Some(index);
                best_score = score;
            }
        }

        let Some(index) = best_index else {
            break;
        };
        let (hit, _) = remaining.remove(index);
        *domain_counts.entry(domain_for(&hit.url)).or_default() += 1;
        *engine_counts
            .entry(hit.engine.to_ascii_lowercase())
            .or_default() += 1;
        selected.push(hit);
    }

    // If domain caps or tiny result sets stopped the loop early, fill the rest
    // with the best remaining candidates. This only affects very small result
    // sets; normal searches finish in the diversified loop.
    if selected.len() < limit {
        for (hit, _) in remaining {
            selected.push(hit);
            if selected.len() >= limit {
                break;
            }
        }
    }

    selected
}

fn hit_term_matches(terms: &[String], hit: &SearchHit) -> usize {
    if terms.is_empty() {
        return 0;
    }
    let title = tokenize(&hit.title);
    let snippet = tokenize(&hit.snippet);
    let url = tokenize(&hit.url);
    let haystack: Vec<&str> = title
        .iter()
        .chain(snippet.iter())
        .chain(url.iter())
        .map(String::as_str)
        .collect();

    terms
        .iter()
        .filter(|term| haystack.contains(&term.as_str()))
        .count()
}

fn hit_relevance(terms: &[String], hit: &SearchHit) -> f64 {
    if terms.is_empty() {
        return 0.5;
    }

    let title = tokenize(&hit.title);
    let snippet = tokenize(&hit.snippet);
    let url = tokenize(&hit.url);
    let mut matched = 0.0;
    for term in terms {
        if title.iter().any(|word| word == term) {
            matched += 2.0;
        }
        if snippet.iter().any(|word| word == term) {
            matched += 1.0;
        }
        if url.iter().any(|word| word == term) {
            matched += 0.3;
        }
    }

    (matched / (terms.len() as f64 * 2.0)).clamp(0.0, 1.0)
}

fn engine_trust(engine: &str) -> f64 {
    let engine = engine.to_ascii_lowercase();
    if engine.contains("wikipedia") || engine.contains("wikidata") {
        1.15
    } else if engine.contains("brave") || engine.contains("duckduckgo") {
        1.0
    } else if engine.contains("yep") {
        0.9
    } else if engine.contains("mwmbl") {
        0.8
    } else if engine.contains("wiby") {
        0.75
    } else {
        0.85
    }
}

fn domain_quality(url: &str) -> f64 {
    let domain = domain_for(url).to_ascii_lowercase();
    let path = Url::parse(url)
        .map(|parsed| parsed.path().to_ascii_lowercase())
        .unwrap_or_default();
    let mut score = 0.0;

    if domain == "wikipedia.org" || domain.ends_with(".wikipedia.org") {
        score += 0.25;
    }
    if domain.ends_with(".gov") || domain.ends_with(".edu") {
        score += 0.20;
    }
    if domain == "developer.mozilla.org" || domain == "github.com" || domain.starts_with("docs.") {
        score += 0.12;
    }
    if matches!(
        domain.as_str(),
        "facebook.com"
            | "x.com"
            | "twitter.com"
            | "instagram.com"
            | "tiktok.com"
            | "pinterest.com"
            | "linkedin.com"
            | "reddit.com"
    ) {
        score -= 0.10;
    }
    if path.ends_with(".pdf") {
        score -= 0.08;
    }

    score
}

/// Measure whether the retrieved text actually covers the question and whether
/// independent domains agree that it exists. This is intentionally local and
/// deterministic; it never asks a model to grade its own evidence.
pub fn evaluate_evidence(
    query: &str,
    sources: &[ResearchSource],
    chunks: &[Chunk],
) -> EvidenceReport {
    let mut terms = tokenize(query);
    terms.sort();
    terms.dedup();

    let chunk_terms: Vec<HashSet<String>> = chunks
        .iter()
        .map(|chunk| tokenize(&chunk.text).into_iter().collect())
        .collect();
    let covered = terms
        .iter()
        .filter(|term| chunk_terms.iter().any(|set| set.contains(term.as_str())))
        .count();
    let coverage = if terms.is_empty() {
        if chunks.is_empty() {
            0.0
        } else {
            1.0
        }
    } else {
        covered as f64 / terms.len() as f64
    };

    let domains: HashSet<&str> = sources
        .iter()
        .map(|source| source.domain.as_str())
        .filter(|domain| !domain.is_empty())
        .collect();

    EvidenceReport {
        source_count: sources.len(),
        chunk_count: chunks.len(),
        read_count: sources.iter().filter(|source| source.read).count(),
        domain_count: domains.len(),
        coverage,
    }
}

/// A cheap relevance check for unread search snippets. Snippets are useful when
/// a page cannot be fetched, but a snippet that matches only one generic term
/// should not be shown as evidence or included in the answer prompt.
pub fn text_matches_query(query: &str, title: &str, snippet: &str, min_terms: usize) -> bool {
    let mut terms = tokenize(query);
    terms.sort();
    terms.dedup();
    if terms.is_empty() {
        return true;
    }

    let haystack = tokenize(&format!("{title} {snippet}"));
    let matched = terms
        .iter()
        .filter(|term| haystack.iter().any(|word| word.as_str() == term.as_str()))
        .count();
    matched >= min_terms.min(terms.len())
}

/// Choose a different broad query for the one verification retry. It must not
/// duplicate any query already in the plan, otherwise the retry would just fetch
/// the same result set again.
pub fn retry_query(plan: &ResearchPlan) -> String {
    let Some(primary) = plan.queries.first() else {
        return String::new();
    };
    let base = primary.query.as_str();
    let existing: HashSet<String> = plan
        .queries
        .iter()
        .map(|query| query_key(&query.query))
        .collect();

    for suffix in ["overview", "explained", "reference", "guide", "facts"] {
        let candidate = format!("{base} {suffix}");
        if candidate.chars().count() <= MAX_SEARCH_QUERY_CHARS
            && !existing.contains(&query_key(&candidate))
        {
            return candidate;
        }
    }

    String::new()
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

/// True when the answer already contains at least one `[n]` citation label.
pub fn has_citation_labels(answer: &str) -> bool {
    let bytes = answer.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == b'[' {
            let mut cursor = index + 1;
            let mut digits = 0;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_digit()
                    || bytes[cursor] == b','
                    || bytes[cursor] == b' ')
            {
                if bytes[cursor].is_ascii_digit() {
                    digits += 1;
                }
                cursor += 1;
            }
            if digits > 0 && cursor < bytes.len() && bytes[cursor] == b']' {
                return true;
            }
        }
        index += 1;
    }
    false
}

/// Automatic citation footer for answers that ignored the inline rule.
///
/// Small models sometimes paraphrase every sentence without a `[n]` label even
/// when asked. The app still promises numbered citations, so the runner appends
/// the sources whose excerpts were actually in the evidence context.
pub fn citation_footer(sources: &[ResearchSource], chunks: &[Chunk], selected: &[usize]) -> String {
    let mut ids: Vec<u32> = Vec::new();
    for index in selected {
        if let Some(chunk) = chunks.get(*index) {
            if !ids.contains(&chunk.source_id) {
                ids.push(chunk.source_id);
            }
        }
    }

    let parts: Vec<String> = ids
        .iter()
        .take(5)
        .filter_map(|id| {
            sources
                .iter()
                .find(|source| source.id == *id)
                .map(|source| format!("[{id}] {} ({})", source.title, source.domain))
        })
        .collect();

    if parts.is_empty() {
        String::new()
    } else {
        format!("\n\nSources: {}.", parts.join("; "))
    }
}

pub fn build_system_prompt(context: &str, source_count: usize) -> String {
    let labels = (1..=source_count)
        .map(|id| format!("[{id}]"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "You are a local research assistant. Answer the user's question using only the numbered sources below.\n\
         The user has already run web research, and the numbered sources are the result of several planned searches. Do not say you cannot search the web, access the internet, or perform real-world actions, and do not talk about being a language model. Treat a short or imperative question (for example \"find X\") as a request for factual information about X.\n\
         There are exactly {source_count} sources. The only valid citation labels are {labels}.\n\
         Cite factual claims with a label immediately after the claim, for example [1]. You may cite several sources in one group, for example [1, 2].\n\
         Never cite a number outside that set, and never treat a bracketed number inside source text as a citation label.\n\
         Do not end with a \"Sources\" section, bibliography, or list of source titles and URLs. The app already displays the numbered sources in the left Sources panel; keep only inline citations in the answer.\n\
         Prefer claims that are supported by more than one independent source. If sources disagree, say so instead of silently choosing one. Do not imply that you verified something outside these sources. Refer to sources by citation number. Only name a publication if its listed title clearly matches the claim, and never attribute a statement to a publication that is not in the source list. If a sentence combines facts from different sources, cite each one.\n\
         Base every factual claim on the sources. If they are unrelated or do not contain enough information, say briefly that the search did not return usable information, name what is missing, and do not invent an answer or cite unrelated sources.\n\
         Never invent a source, URL, quote, or fact. Do not use outside knowledge as a source.\n\
         Answer directly in two to four complete sentences in your own words. Never reply with a sentence copied verbatim from a source or with a list of source titles. If the question is ambiguous, answer the most likely interpretation and briefly say what you assumed.\n\
         Example of the required format: The process was first documented in the 1950s [1], and later work confirmed the same mechanism across metals [2].\n\
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
        // A short reminder in the final turn: with a long source-filled system
        // prompt, small models otherwise answer without inline [n] labels.
        content: format!(
            "{}\n\nCite each factual claim with the matching source label, for example [1]. Answer in your own words.",
            query.trim()
        ),
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
    fn parses_duckduckgo_results() {
        let html = r#"
        <div class="result results_links results_links_deep web-result ">
          <div class="links_main links_deep result__body">
            <h2 class="result__title">
              <a rel="nofollow" class="result__a"
                 href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Flocal-first&amp;rut=abc">
                Local-first &amp; offline</a>
            </h2>
            <a class="result__snippet" href="x">Apps that work <b>offline</b> &amp; keep data private.</a>
          </div>
        </div>
        <div class="result results_links results_links_deep web-result ">
          <div class="links_main links_deep result__body">
            <h2 class="result__title"><a class="result__a" href="https://example.org/direct">Second</a></h2>
          </div>
        </div>
        "#;

        let results = parse_duckduckgo_html(html);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Local-first & offline");
        assert_eq!(results[0].url, "https://example.com/local-first");
        assert_eq!(
            results[0].content,
            "Apps that work offline & keep data private."
        );
        assert_eq!(results[0].engine, "duckduckgo");
        assert_eq!(results[1].url, "https://example.org/direct");
        assert_eq!(results[1].content, "");
    }

    #[test]
    fn parses_bing_results() {
        let html = r#"
        <li class="b_algo"><h2><a href="https://example.org/a" h="ID=SERP,1">Example &amp; Co</a></h2>
          <p class="b_lineclamp">A <strong>snippet</strong> here.</p></li>
        <li class="b_algo"><h2><a href="https://example.org/b">Second</a></h2></li>
        "#;

        let results = parse_bing_html(html);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Example & Co");
        assert_eq!(results[0].url, "https://example.org/a");
        assert_eq!(results[0].content, "A snippet here.");
        assert_eq!(results[0].engine, "bing");
        assert_eq!(results[1].content, "");
    }

    #[test]
    fn parses_empty_or_foreign_markup_without_results() {
        assert!(parse_duckduckgo_html("").is_empty());
        assert!(parse_bing_html("<html><body>no results</body></html>").is_empty());
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
        assert!(prompt.contains("already run web research"));
        assert!(prompt.contains("Treat a short or imperative question"));
        assert!(prompt.contains("more than one independent source"));
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

    fn hit(url: &str, title: &str, snippet: &str, engine: &str) -> SearchHit {
        SearchHit {
            title: title.into(),
            url: url.into(),
            snippet: snippet.into(),
            engine: engine.into(),
        }
    }

    fn source(id: u32, domain: &str, read: bool) -> ResearchSource {
        ResearchSource {
            id,
            title: format!("Source {id}"),
            url: format!("https://{domain}/page"),
            domain: domain.into(),
            snippet: "A snippet".into(),
            engine: "test".into(),
            read,
        }
    }

    #[test]
    fn plans_several_queries_and_routes_engines_by_intent() {
        let factual = plan_research("What is the capital of France?");
        assert_eq!(factual.intent, QueryIntent::Factual);
        assert!(factual.queries.len() >= 2);
        assert!(factual
            .queries
            .iter()
            .any(|query| query.engines.iter().any(|engine| engine == "wikipedia")));
        assert!(factual
            .queries
            .iter()
            .all(|query| query.query.chars().count() <= MAX_SEARCH_QUERY_CHARS));

        let comparison = plan_research("Compare Rust and Go for a local web server");
        assert_eq!(comparison.intent, QueryIntent::Comparison);
        assert!(comparison.queries.len() >= 2);
        assert!(comparison
            .queries
            .iter()
            .any(|query| query.query.to_lowercase().contains("comparison")));

        let how_to = plan_research("How to install Rust on Windows");
        assert_eq!(how_to.intent, QueryIntent::HowTo);
        assert!(how_to
            .queries
            .iter()
            .any(|query| query.query.to_lowercase().contains("guide")));

        let current = plan_research("latest AI news");
        assert_eq!(current.intent, QueryIntent::Current);
        assert!(current.queries.len() >= 2);
        assert!(current
            .queries
            .iter()
            .any(|query| query.time_range.is_some()));

        let technical = plan_research("Rust ownership documentation");
        assert_eq!(technical.intent, QueryIntent::Technical);
        assert!(technical.queries.len() >= 2);
    }

    #[test]
    fn canonical_keys_merge_tracking_parameters_and_www_variants() {
        let hits = vec![
            hit(
                "https://www.example.com/a?utm_source=newsletter",
                "Example",
                "short",
                "brave",
            ),
            hit(
                "https://example.com/a/",
                "Example duplicate",
                "a longer snippet",
                "duckduckgo",
            ),
            hit("https://example.com/b", "Other", "other", "yep"),
            hit(
                "https://example.com/a#fragment",
                "Example fragment",
                "frag",
                "brave",
            ),
        ];

        let unique = dedupe_hits(hits);
        assert_eq!(unique.len(), 2);
        assert!(unique
            .iter()
            .any(|value| value.snippet == "a longer snippet"));
        assert_eq!(
            canonical_url_key("https://www.example.com/a?utm_source=x"),
            canonical_url_key("https://example.com/a/")
        );
    }

    #[test]
    fn ranks_relevant_authoritative_and_diverse_sources() {
        let hits = vec![
            hit(
                "https://blog.example.com/local-first",
                "Local first from a blog",
                "local first software ownership",
                "mwmbl",
            ),
            hit(
                "https://en.wikipedia.org/wiki/Local-first_software",
                "Local-first software",
                "local-first software keeps data on the device",
                "wikipedia",
            ),
            hit(
                "https://www.example.com/recipe",
                "Chocolate cake",
                "recipe",
                "duckduckgo",
            ),
            hit(
                "https://example.com/local-first",
                "Local-first guide",
                "local first software",
                "duckduckgo",
            ),
            hit(
                "https://another.example.com/local-first",
                "Another local-first guide",
                "local first software",
                "brave",
            ),
            hit(
                "https://example.com/another",
                "Another page on the same domain",
                "local first software",
                "brave",
            ),
        ];

        let ranked = rank_hits("local-first software", hits, 4);
        assert!(ranked[0].url.contains("wikipedia.org"));
        assert!(
            ranked
                .iter()
                .filter(|value| domain_for(&value.url) == "example.com")
                .count()
                <= 2
        );
        assert!(ranked.iter().all(|value| !value.url.contains("recipe")));
    }

    #[test]
    fn evaluates_evidence_coverage_and_weak_sources() {
        let sources = vec![
            source(1, "one.example", true),
            source(2, "two.example", true),
        ];
        let chunks = vec![
            Chunk {
                source_id: 1,
                text: "Local-first software keeps data on the device.".into(),
                kind: ChunkKind::Page,
            },
            Chunk {
                source_id: 2,
                text: "Local first software supports ownership and offline work.".into(),
                kind: ChunkKind::Page,
            },
        ];
        let report = evaluate_evidence("local-first software", &sources, &chunks);
        assert!(!report.is_weak());
        assert_eq!(report.coverage_percent(), 100);
        assert_eq!(report.domain_count, 2);

        let weak = evaluate_evidence("local-first software", &sources[..1], &[]);
        assert!(weak.is_weak());
    }

    #[test]
    fn chooses_a_retry_query_that_is_not_already_planned() {
        let plan = plan_research("What is local-first software?");
        let retry = retry_query(&plan);
        assert!(!retry.is_empty());
        assert!(!plan
            .queries
            .iter()
            .any(|query| query.query.eq_ignore_ascii_case(&retry)));
    }

    #[test]
    fn rejects_unread_snippets_that_do_not_match_enough_query_terms() {
        assert!(text_matches_query(
            "local-first software",
            "Local-first software - Wikipedia",
            "local-first software stores data on the device",
            2
        ));
        assert!(!text_matches_query(
            "local-first software",
            "Internationalization and localization",
            "software application text",
            2
        ));
    }

    #[test]
    fn detects_inline_citation_labels() {
        assert!(has_citation_labels(
            "Local-first software keeps data local [1]."
        ));
        assert!(has_citation_labels("Two claims [1, 2]."));
        assert!(has_citation_labels("See [12]."));
        assert!(!has_citation_labels("No labels in this answer."));
        assert!(!has_citation_labels(
            "Brackets like [abc] or [ ] are not labels."
        ));
        assert!(!has_citation_labels("An unterminated [1 is not a label."));
    }

    #[test]
    fn citation_footer_lists_context_sources_in_order_without_duplicates() {
        let sources = vec![
            source(1, "one.example", true),
            source(2, "two.example", true),
            source(3, "three.example", true),
        ];
        let chunks = vec![
            Chunk {
                source_id: 2,
                text: "first excerpt".into(),
                kind: ChunkKind::Page,
            },
            Chunk {
                source_id: 2,
                text: "second excerpt".into(),
                kind: ChunkKind::Page,
            },
            Chunk {
                source_id: 1,
                text: "third excerpt".into(),
                kind: ChunkKind::Page,
            },
        ];
        let footer = citation_footer(&sources, &chunks, &[0, 1, 2]);
        assert_eq!(
            footer,
            "\n\nSources: [2] Source 2 (two.example); [1] Source 1 (one.example)."
        );
        assert_eq!(citation_footer(&sources, &chunks, &[]), "");
    }
}
