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
    })
}

/// Fetch a URL server-side, then parse. Image download is best-effort.
pub async fn fetch_and_parse(url: &str) -> Result<ImportDraft, AppError> {
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

    // Check content-length; reject pages > 2MB
    if let Some(len) = resp.content_length() {
        if len > 2_000_000 {
            return Err(AppError::BadRequest("page too large (max 2MB)".into()));
        }
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to read page body: {e}")))?;

    let html = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::BadRequest("page is not valid UTF-8".into()))?;

    let (mut draft, image_url) = parse_recipe_with_image_url(html)?;

    // Image download (best-effort)
    if let Some(img_url) = image_url {
        if let Some(jpeg_bytes) = try_download_image(&client, &img_url).await {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
            draft.image_base64 = Some(b64);
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

            return Ok((
                ImportDraft {
                    name,
                    ingredients,
                    instructions,
                    image_base64: None,
                },
                image_url,
            ));
        }
    }

    Err(AppError::BadRequest(
        "no schema.org Recipe found in the provided content".into(),
    ))
}

/// Split an ingredient line into name and optional quantity.
/// Best-effort: if the line starts with a quantity prefix (number + unit word),
/// the prefix is the quantity and the rest is the name. Otherwise the whole line is the name.
fn split_ingredient_line(line: &str) -> NewIngredientLine {
    let units = [
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
            units.contains(&t.to_lowercase().trim_end_matches(',').trim_end_matches('.'))
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
fn truncate(s: &str, max: usize) -> String {
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
fn extract_image_url(json: &serde_json::Value) -> Option<String> {
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
async fn try_download_image(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    let jpeg = crate::image::convert_to_jpeg(&bytes).ok()?;
    Some(jpeg)
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

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Fixture: Google's official Recipe JSON-LD example (Pina Colada)
    const HTML_WITH_JSONLD: &str = r#"<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "author": "John Smith",
  "cookTime": "PT1H",
  "datePublished": "2009-05-08",
  "description": "A delicious pina colada recipe.",
  "image": "https://example.com/pina-colada.jpg",
  "recipeIngredient": [
    "2 cups pineapple juice",
    "1/2 cup cream of coconut",
    "1 cup ice",
    "salt"
  ],
  "recipeInstructions": [
    {"@type": "HowToStep", "text": "Blend all ingredients until smooth."},
    {"@type": "HowToStep", "text": "Pour into a glass and serve."}
  ],
  "name": "Pina Colada",
  "nutrition": {"calories": "240 calories"},
  "recipeYield": "1 serving"
}
</script>
</head><body></body></html>"#;

    // Fixture: raw JSON-LD object (not wrapped in HTML)
    const RAW_JSONLD: &str = r#"{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Simple Toast",
  "description": "A simple toast recipe.",
  "recipeIngredient": ["1 slice bread", "butter"],
  "recipeInstructions": "Toast the bread and spread butter."
}"#;

    // Fixture: instructions as plain text string
    const HTML_PLAIN_TEXT_INSTRUCTIONS: &str = r#"<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Boiled Egg",
  "description": "How to boil an egg.",
  "recipeIngredient": ["1 egg", "water"],
  "recipeInstructions": "Put egg in boiling water for 7 minutes."
}
</script>
</head><body></body></html>"#;

    // Fixture: HowToStep array instructions
    const HTML_HOWTOSTEP: &str = r#"<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Pancakes",
  "description": "Fluffy pancakes.",
  "recipeIngredient": ["2 cups flour", "1 cup milk"],
  "recipeInstructions": [
    {"@type": "HowToStep", "text": "Mix dry ingredients."},
    {"@type": "HowToStep", "text": "Add milk and stir."}
  ]
}
</script>
</head><body></body></html>"#;

    // Fixture: HowToSection array instructions
    const HTML_HOWTOSECTION: &str = r#"<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Cake",
  "description": "A layered cake.",
  "recipeIngredient": ["2 cups flour", "1 cup sugar"],
  "recipeInstructions": [
    {
      "@type": "HowToSection",
      "name": "Prep",
      "itemListElement": [
        {"@type": "HowToStep", "text": "Preheat oven to 180C."},
        {"@type": "HowToStep", "text": "Grease the pan."}
      ]
    },
    {
      "@type": "HowToSection",
      "name": "Bake",
      "itemListElement": [
        {"@type": "HowToStep", "text": "Pour batter into pan."},
        {"@type": "HowToStep", "text": "Bake for 30 minutes."}
      ]
    }
  ]
}
</script>
</head><body></body></html>"#;

    #[test]
    fn given_valid_html_with_jsonld_when_parse_recipe_then_returns_draft() {
        let draft = parse_recipe(HTML_WITH_JSONLD).expect("should parse");
        assert_eq!(draft.name, "Pina Colada");
        assert_eq!(draft.ingredients.len(), 4);
        assert_eq!(
            draft.instructions,
            "Blend all ingredients until smooth.\nPour into a glass and serve."
        );
        assert!(draft.image_base64.is_none());
    }

    #[test]
    fn given_raw_jsonld_string_when_parse_recipe_then_returns_draft() {
        let draft = parse_recipe(RAW_JSONLD).expect("should parse");
        assert_eq!(draft.name, "Simple Toast");
        assert_eq!(draft.ingredients.len(), 2);
        assert_eq!(draft.instructions, "Toast the bread and spread butter.");
    }

    #[test]
    fn given_instructions_as_plain_text_when_parse_then_joined() {
        let draft = parse_recipe(HTML_PLAIN_TEXT_INSTRUCTIONS).expect("should parse");
        assert_eq!(
            draft.instructions,
            "Put egg in boiling water for 7 minutes."
        );
    }

    #[test]
    fn given_instructions_as_howtostep_array_when_parse_then_joined() {
        let draft = parse_recipe(HTML_HOWTOSTEP).expect("should parse");
        assert_eq!(
            draft.instructions,
            "Mix dry ingredients.\nAdd milk and stir."
        );
    }

    #[test]
    fn given_instructions_as_howtosection_array_when_parse_then_joined() {
        let draft = parse_recipe(HTML_HOWTOSECTION).expect("should parse");
        assert_eq!(
            draft.instructions,
            "Preheat oven to 180C.\nGrease the pan.\nPour batter into pan.\nBake for 30 minutes."
        );
    }

    #[test]
    fn given_ingredient_non_splittable_when_parse_then_name_only() {
        let draft = parse_recipe(HTML_WITH_JSONLD).expect("should parse");
        let salt = draft
            .ingredients
            .iter()
            .find(|i| i.name == "salt")
            .expect("should have salt ingredient");
        assert!(salt.quantity.is_none());
    }

    #[test]
    fn given_ingredient_with_quantity_when_parse_then_split() {
        let draft = parse_recipe(HTML_WITH_JSONLD).expect("should parse");
        let juice = draft
            .ingredients
            .iter()
            .find(|i| i.name == "pineapple juice")
            .expect("should have pineapple juice");
        assert_eq!(juice.quantity.as_deref(), Some("2 cups"));
    }

    #[test]
    fn given_html_without_recipe_when_parse_then_error() {
        let html = r#"<html><head>
<script type="application/ld+json">
{"@context": "https://schema.org", "@type": "Article", "name": "Not a recipe"}
</script>
</head><body></body></html>"#;
        let result = parse_recipe(html);
        assert!(result.is_err());
        match result {
            Err(AppError::BadRequest(msg)) => assert!(msg.contains("Recipe")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn given_html_without_jsonld_when_parse_then_error() {
        let html = "<html><body><p>No recipe here</p></body></html>";
        let result = parse_recipe(html);
        assert!(result.is_err());
    }

    #[test]
    fn given_malformed_jsonld_when_parse_then_error() {
        let html = r#"<html><head>
<script type="application/ld+json">{invalid json}</script>
</head><body></body></html>"#;
        let result = parse_recipe(html);
        assert!(result.is_err());
    }

    #[test]
    fn given_image_as_object_when_extract_image_url_then_returns_url() {
        let json: serde_json::Value = serde_json::json!({
            "@type": "Recipe",
            "image": {
                "@type": "ImageObject",
                "url": "https://example.com/photo.jpg"
            }
        });
        let result = extract_image_url(&json);
        assert_eq!(result.as_deref(), Some("https://example.com/photo.jpg"));
    }

    #[test]
    fn given_image_as_string_array_when_extract_image_url_then_returns_first() {
        let json: serde_json::Value = serde_json::json!({
            "@type": "Recipe",
            "image": ["https://example.com/img1.jpg", "https://example.com/img2.jpg"]
        });
        let result = extract_image_url(&json);
        assert_eq!(result.as_deref(), Some("https://example.com/img1.jpg"));
    }

    // -----------------------------------------------------------------------
    // sanitize_instructions tests
    // -----------------------------------------------------------------------

    #[test]
    fn given_html_with_dir_attribute_when_sanitize_then_attribute_stripped() {
        let input = "<p dir=ltr>Step 1</p>";
        let result = sanitize_instructions(input);
        assert_eq!(result, "<p>Step 1</p>");
    }

    #[test]
    fn given_script_tag_when_sanitize_then_content_dropped() {
        let input = "<script>alert(1)</script>";
        let result = sanitize_instructions(input);
        assert_eq!(result, "");
    }

    #[test]
    fn given_non_whitelisted_tags_when_sanitize_then_stripped() {
        let input = "<div><span>x</span></div>";
        let result = sanitize_instructions(input);
        assert_eq!(result, "x");
    }

    #[test]
    fn given_whitelisted_nested_tags_when_sanitize_then_preserved() {
        let input = "<p><strong><em>x</em></strong></p>";
        let result = sanitize_instructions(input);
        assert_eq!(result, "<p><strong><em>x</em></strong></p>");
    }

    #[test]
    fn given_br_self_closing_when_sanitize_then_normalized() {
        let input = "a<br/>b";
        let result = sanitize_instructions(input);
        assert_eq!(result, "a<br>b");
    }

    #[test]
    fn given_plain_text_when_sanitize_then_unchanged() {
        let input = "Step 1\nStep 2";
        let result = sanitize_instructions(input);
        assert_eq!(result, "Step 1\nStep 2");
    }

    #[test]
    fn given_whitespace_only_after_sanitize_then_empty_string() {
        let input = "   ";
        let result = sanitize_instructions(input);
        assert_eq!(result, "");
    }

    #[test]
    fn given_strong_and_br_when_sanitize_then_preserved() {
        let input = "<strong>important</strong><br>";
        let result = sanitize_instructions(input);
        assert_eq!(result, "<strong>important</strong><br>");
    }
}
