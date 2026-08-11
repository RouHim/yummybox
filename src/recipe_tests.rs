// Recipe parsing tests. Kept in a separate flat module so src/recipe.rs
// stays focused on production code.

use crate::error::AppError;
use crate::recipe::*;

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

// ── extract_readable_text ──────────────────────────────────────────

#[test]
fn extract_readable_text_strips_script_style() {
    let html = "<html><script>alert(1)</script><style>body{}</style><p>Hello world</p></html>";
    let result = extract_readable_text(html);
    assert_eq!(result, "Hello world");
}

#[test]
fn extract_readable_text_collapses_whitespace() {
    let html = "<p>line one\nline two</p>\n<p>  extra   spaces  </p>";
    let result = extract_readable_text(html);
    assert_eq!(result, "line one line two extra spaces");
}

#[test]
fn extract_readable_text_empty_input() {
    assert_eq!(extract_readable_text(""), "");
}

// ── extract_image_urls_from_html ──────────────────────────────────

#[test]
fn extract_image_urls_finds_og_image() {
    let html = r#"<html><head>
<meta property="og:image" content="https://example.com/cake.jpg">
</head><body></body></html>"#;
    let urls = extract_image_urls_from_html(html, "https://example.com/page");
    assert_eq!(urls, vec!["https://example.com/cake.jpg"]);
}

#[test]
fn extract_image_urls_finds_jsonld_image() {
    let html = r#"<html><head>
<script type="application/ld+json">
{"@type":"Recipe","image":"https://example.com/pie.jpg","name":"Pie","recipeIngredient":["flour"],"recipeInstructions":"Bake"}
</script>
</head><body></body></html>"#;
    let urls = extract_image_urls_from_html(html, "https://example.com/page");
    assert_eq!(urls, vec!["https://example.com/pie.jpg"]);
}

#[test]
fn extract_image_urls_finds_wp_post_image_class() {
    let html = r#"<html><body>
<img class="attachment-post-thumbnail size-post-thumbnail wp-post-image" src="https://example.com/salad.jpg">
</body></html>"#;
    let urls = extract_image_urls_from_html(html, "https://example.com/page");
    assert_eq!(urls, vec!["https://example.com/salad.jpg"]);
}

#[test]
fn extract_image_urls_deduplicates_and_preserves_priority() {
    let html = r#"<html><head>
<meta property="og:image" content="https://example.com/first.jpg">
<script type="application/ld+json">
{"@type":"Recipe","image":"https://example.com/first.jpg","name":"Pie","recipeIngredient":["flour"],"recipeInstructions":"Bake"}
</script>
</head><body>
<img class="wp-post-image" src="https://example.com/second.jpg">
</body></html>"#;
    let urls = extract_image_urls_from_html(html, "https://example.com/page");
    // og:image first (deduped), then the <img> — JSON-LD duplicate is dropped
    assert_eq!(
        urls,
        vec![
            "https://example.com/first.jpg",
            "https://example.com/second.jpg"
        ]
    );
}

#[test]
fn extract_image_urls_resolves_relative_urls() {
    let html = r#"<html><body>
<img class="wp-post-image" src="/wp-content/uploads/2024/salad.jpg">
</body></html>"#;
    let urls = extract_image_urls_from_html(html, "https://example.com/recipe/yum-yum-salat/");
    assert_eq!(
        urls,
        vec!["https://example.com/wp-content/uploads/2024/salad.jpg"]
    );
}

#[test]
fn extract_image_urls_empty_when_no_images() {
    let html = "<html><body><p>No images here</p></body></html>";
    let urls = extract_image_urls_from_html(html, "https://example.com/page");
    assert!(urls.is_empty());
}

// ── is_bare_url ────────────────────────────────────────────────────

#[test]
fn is_bare_url_detects_http_https() {
    assert!(is_bare_url("https://example.com/recipe"));
    assert!(is_bare_url("http://localhost:8080/foo"));
}

#[test]
fn is_bare_url_rejects_plain_text() {
    assert!(!is_bare_url("pasta with tomatoes"));
    assert!(!is_bare_url(""));
}

#[test]
fn is_bare_url_rejects_embedded_url() {
    assert!(!is_bare_url("see https://example.com here"));
}
