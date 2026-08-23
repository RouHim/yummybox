use std::collections::HashSet;
use std::time::Duration;

use base64::Engine;
use recipe_scraper::Extract;
use recipe_scraper::SchemaOrgEntry;

use crate::error::AppError;
use crate::model::NewIngredientLine;

/// Output of a recipe parse — a meal-shaped draft, not persisted.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDraft {
    pub name: String,
    pub ingredients: Vec<NewIngredientLine>,
    pub instructions: String,
    /// Base64-encoded JPEG bytes if an image was found and downloaded; None otherwise.
    /// Only populated by `fetch_and_parse` (URL mode). Always `None` for `parse_recipe` (paste mode).
    pub image_base64: Option<String>,
    #[serde(default)]
    pub portions: Option<i32>,
}

/// Parse a recipe from raw HTML or JSON-LD text. No network fetch.
/// `image_base64` is always `None` in the returned draft (paste mode cannot download).
pub fn parse_recipe(text: &str) -> Result<ImportDraft, AppError> {
    let (draft, _image_url) = parse_recipe_with_image_url(text)?;
    Ok(ImportDraft {
        name: draft.name,
        ingredients: draft.ingredients,
        instructions: draft.instructions,
        image_base64: None,
        portions: draft.portions,
    })
}

/// Fetch a URL and return the HTML body as a `String`. Used by both
/// [`fetch_and_parse`] (recipe-scraper path) and [`import_from_llm`]
/// (LLM URL expansion path).
pub async fn fetch_page_html(url: &str) -> Result<String, AppError> {
    let parsed_url =
        reqwest::Url::parse(url).map_err(|_| AppError::BadRequest("invalid URL".into()))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))?;

    let resp = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to fetch page: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "fetch returned HTTP {}",
            resp.status()
        )));
    }

    if let Some(len) = resp.content_length() {
        if len > 2_000_000 {
            return Err(AppError::BadRequest("page too large (max 2MB)".into()));
        }
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to read page body: {e}")))?;

    std::str::from_utf8(&bytes)
        .map(|s| s.to_string())
        .map_err(|_| AppError::BadRequest("page is not valid UTF-8".into()))
}
/// Fetch a URL server-side, then parse. Image download is best-effort.
pub async fn fetch_and_parse(url: &str) -> Result<ImportDraft, AppError> {
    let html = fetch_page_html(url).await?;
    let (mut draft, image_url) = parse_recipe_with_image_url(&html)?;

    // Image download (best-effort) — needs its own client
    if let Some(img_url) = image_url {
        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .build()
        {
            if let Some(jpeg_bytes) = try_download_image(&client, &img_url).await {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
                draft.image_base64 = Some(b64);
            }
        }
    }

    Ok(draft)
}

/// Parse recipe and return the draft plus the raw image URL (if found in JSON-LD).
/// `fetch_and_parse` uses the image URL to download; `parse_recipe` discards it.
fn parse_recipe_with_image_url(text: &str) -> Result<(ImportDraft, Option<String>), AppError> {
    // Use scraper to find all JSON-LD script blocks
    let document = scraper::Html::parse_document(text);
    let selector =
        scraper::Selector::parse(r#"script[type="application/ld+json"]"#).expect("static selector");

    // Collect (raw_json_value, schema_entry) pairs for blocks that parse successfully
    let mut pairs: Vec<(serde_json::Value, SchemaOrgEntry)> = Vec::new();

    for element in document.select(&selector) {
        let block_text = element.text().collect::<String>();
        // Try serde_json parse for image extraction
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&block_text) {
            // Try recipe_scraper parse for recipe extraction
            if let Ok(entry) = SchemaOrgEntry::from_json_str(&block_text) {
                pairs.push((json_value, entry));
            }
        }
    }

    // Fallback: if no script blocks found, try parsing the text directly as raw JSON-LD
    if pairs.is_empty() {
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(text) {
            if let Ok(entry) = SchemaOrgEntry::from_json_str(text) {
                pairs.push((json_value, entry));
            }
        }
    }

    // Extract the first Recipe from all SchemaEntry objects
    for (json_value, entry) in &pairs {
        let recipes: Vec<_> = entry.extract_recipes();
        if let Some(recipe) = recipes.into_iter().next() {
            let name = recipe.name().to_string();
            let ingredients = recipe
                .ingredients()
                .clone()
                .into_iter()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(split_ingredient_line(trimmed))
                    }
                })
                .collect();
            let raw_instructions = recipe
                .directions()
                .as_ref()
                .map_or_else(String::new, |list| {
                    if let Some(dirs) = list.directions() {
                        dirs.iter()
                            .map(|d| d.to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if let Some(sections) = list.sections() {
                        sections
                            .cloned()
                            .flat_map(|s| s.into_iter().map(|d| d.to_string()).collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        String::new()
                    }
                });
            let instructions = sanitize_instructions(&raw_instructions);

            let image_url = extract_image_url(json_value);

            let portions = recipe.yields().as_ref().and_then(|y| {
                let s = y.to_string();
                let num_str: String = s
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                num_str.parse::<i32>().ok()
            });

            return Ok((
                ImportDraft {
                    name,
                    ingredients,
                    instructions,
                    image_base64: None,
                    portions,
                },
                image_url,
            ));
        }
    }

    Err(AppError::BadRequest(
        "no schema.org Recipe found in the provided content".into(),
    ))
}

/// Unit words recognized as a quantity prefix in ingredient lines.
const UNITS: &[&str] = &[
    "cup",
    "cups",
    "tbsp",
    "tablespoon",
    "tablespoons",
    "tsp",
    "teaspoon",
    "teaspoons",
    "g",
    "gram",
    "grams",
    "kg",
    "kilogram",
    "kilograms",
    "ml",
    "milliliter",
    "milliliters",
    "l",
    "liter",
    "liters",
    "oz",
    "ounce",
    "ounces",
    "lb",
    "lbs",
    "pound",
    "pounds",
    "clove",
    "cloves",
    "slice",
    "slices",
    "piece",
    "pieces",
    "pinch",
    "dash",
    "quart",
    "quarts",
    "pint",
    "pints",
    "gallon",
    "gallons",
    "stick",
    "sticks",
    "bunch",
    "bunches",
    "handful",
    "handfuls",
    "can",
    "cans",
];

/// Split an ingredient line into name and optional quantity.
/// Best-effort: if the line starts with a quantity prefix (number + unit word),
/// the prefix is the quantity and the rest is the name. Otherwise the whole line is the name.
pub(crate) fn split_ingredient_line(line: &str) -> NewIngredientLine {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return NewIngredientLine {
            name: truncate(line.trim(), 100),
            quantity: None,
        };
    }

    // Check if first token starts with a digit or is a fraction (1/2, 1½, etc.)
    let starts_with_number = tokens[0]
        .chars()
        .next()
        .map(|c| c.is_ascii_digit() || c == '½' || c == '⅓' || c == '⅔' || c == '¼' || c == '¾')
        .unwrap_or(false);

    if starts_with_number && tokens.len() >= 2 {
        // Check if the second token (or sometimes third) is a unit word
        let unit_idx = tokens.iter().skip(1).take(2).position(|t| {
            UNITS.contains(&t.to_lowercase().trim_end_matches(',').trim_end_matches('.'))
        });

        if let Some(rel_idx) = unit_idx {
            let unit_end = 1 + rel_idx + 1; // number + unit
            let quantity = tokens[..unit_end].join(" ");
            let name = tokens[unit_end..].join(" ");
            if !name.is_empty() {
                return NewIngredientLine {
                    name: truncate(name.trim(), 100),
                    quantity: Some(truncate(quantity.trim(), 50)),
                };
            }
        }
    }

    NewIngredientLine {
        name: truncate(line.trim(), 100),
        quantity: None,
    }
}

/// Truncate a string to `max` chars, appending `…` if truncated.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        let truncated = &s[..max.saturating_sub(1)];
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}

/// Extract the image URL from a raw JSON-LD value.
/// The `image` field can be: a URL string, an array of URL strings,
/// a single `ImageObject` with a `url` field, or an array of `ImageObject`s.
pub(crate) fn extract_image_url(json: &serde_json::Value) -> Option<String> {
    // If @graph array, find the first element with @type containing "Recipe"
    if let Some(graph) = json.get("@graph").and_then(|g| g.as_array()) {
        for item in graph {
            if is_recipe_type(item) {
                if let Some(url) = extract_image_url(item) {
                    return Some(url);
                }
            }
        }
    }

    let img = json.get("image")?;
    match img {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => arr.first().and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(o) => o.get("url").and_then(|u| u.as_str()).map(String::from),
            _ => None,
        }),
        serde_json::Value::Object(o) => o.get("url").and_then(|u| u.as_str()).map(String::from),
        _ => None,
    }
}

/// Check if a JSON-LD value has `@type` containing "Recipe".
fn is_recipe_type(json: &serde_json::Value) -> bool {
    match json.get("@type") {
        Some(serde_json::Value::String(s)) => s == "Recipe",
        Some(serde_json::Value::Array(arr)) => arr.iter().any(|t| t.as_str() == Some("Recipe")),
        _ => false,
    }
}

/// Download an image URL and convert to JPEG bytes. Best-effort: returns None on any failure.
pub(crate) async fn try_download_image(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    let jpeg = crate::image::convert_to_jpeg(&bytes).ok()?;
    Some(jpeg)
}

/// Download an image URL and convert to JPEG bytes via the standard pipeline
/// (`convert_to_jpeg`, q82, 3840px max long edge). Returns a structured error
/// on any failure so callers can surface actionable messages to the user.
pub(crate) async fn download_image_from_url(url: &str) -> Result<Vec<u8>, AppError> {
    let parsed_url =
        reqwest::Url::parse(url).map_err(|_| AppError::BadRequest("invalid image URL".into()))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))?;

    let resp = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("image URL unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "image URL returned HTTP {}",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to download image: {e}")))?;

    let jpeg = crate::image::convert_to_jpeg(&bytes)?;
    Ok(jpeg)
}

/// Sanitize HTML in imported instructions to a safe whitelist.
/// Allows only: p, br, strong, em, b, i, ul, ol, li. Strips all attributes.
/// Drops the *content* of script/style tags. Plain text passes through.
/// Returns "" if the result is empty/whitespace-only.
pub fn sanitize_instructions(html: &str) -> String {
    let tags: HashSet<&str> =
        HashSet::from(["p", "br", "strong", "em", "b", "i", "ul", "ol", "li"]);
    let clean_content: HashSet<&str> = HashSet::from(["script", "style"]);
    let sanitized = ammonia::Builder::empty()
        .add_tags(&tags)
        .clean_content_tags(clean_content)
        .clean(html)
        .to_string();
    if sanitized.trim().is_empty() {
        String::new()
    } else {
        sanitized
    }
}

/// Strip all HTML tags and return the plain text content with whitespace
/// collapsed to single spaces. Uses `ammonia` with no allowed tags, plus
/// `clean_content_tags` to drop script/style/noscript/nav/footer/header
/// elements entirely.
pub fn extract_readable_text(html: &str) -> String {
    let clean_content: HashSet<&str> =
        HashSet::from(["script", "style", "noscript", "nav", "footer", "header"]);
    ammonia::Builder::empty()
        .clean_content_tags(clean_content)
        .clean(html)
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve a possibly-relative URL against a base URL. Returns None if parsing fails.
fn resolve_url(url: &str, base_url: &str) -> Option<String> {
    let base = reqwest::Url::parse(base_url).ok()?;
    reqwest::Url::options()
        .base_url(Some(&base))
        .parse(url.trim())
        .ok()
        .map(|u| u.to_string())
}

/// Extract candidate image URLs from raw HTML.
/// Checks, in priority order: OpenGraph `og:image`, JSON-LD `image`,
/// and `<img>` tags with recipe-relevant classes. Returns de-duplicated, absolute URLs.
/// Returns an empty Vec (not an error) if no image URLs are found.
pub fn extract_image_urls_from_html(html: &str, base_url: &str) -> Vec<String> {
    let document = scraper::Html::parse_document(html);
    let mut urls: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1. OpenGraph og:image
    if let Ok(sel) = scraper::Selector::parse(r#"meta[property="og:image"]"#) {
        for el in document.select(&sel) {
            if let Some(content) = el.value().attr("content") {
                if let Some(abs) = resolve_url(content, base_url) {
                    if seen.insert(abs.clone()) {
                        urls.push(abs);
                    }
                }
            }
        }
    }

    // 2. JSON-LD image (reuse existing extract_image_url)
    if let Ok(sel) = scraper::Selector::parse(r#"script[type="application/ld+json"]"#) {
        for el in document.select(&sel) {
            let block = el.text().collect::<String>();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&block) {
                if let Some(img) = extract_image_url(&json) {
                    if let Some(abs) = resolve_url(&img, base_url) {
                        if seen.insert(abs.clone()) {
                            urls.push(abs);
                        }
                    }
                }
            }
        }
    }

    // 3. <img> tags with recipe-relevant classes
    if let Ok(sel) = scraper::Selector::parse("img") {
        for el in document.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let class = el.value().attr("class").unwrap_or("");
                let is_relevant = class.split_whitespace().any(|c| {
                    matches!(
                        c,
                        "wp-post-image"
                            | "attachment-post-thumbnail"
                            | "size-post-thumbnail"
                            | "recipe-image"
                            | "featured-image"
                    )
                });
                if is_relevant {
                    if let Some(abs) = resolve_url(src, base_url) {
                        if seen.insert(abs.clone()) {
                            urls.push(abs);
                        }
                    }
                }
            }
        }
    }

    urls
}

/// Returns `true` when `s` is a bare `http://` or `https://` URL with no
/// surrounding whitespace — i.e. the entire trimmed string is a single URL
/// token.
pub fn is_bare_url(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return false;
    }
    match reqwest::Url::parse(s) {
        Ok(u) => u.scheme() == "http" || u.scheme() == "https",
        Err(_) => false,
    }
}
