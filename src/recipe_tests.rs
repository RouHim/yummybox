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
fn given_german_metric_units_when_parse_then_split() {
    let cases = [
        ("400 g Putenschnitzel", "400 g", "Putenschnitzel"),
        ("30 g Mehl (Type 405)", "30 g", "Mehl (Type 405)"),
        ("300 ml Gemüsebrühe", "300 ml", "Gemüsebrühe"),
        ("80 g Sahne", "80 g", "Sahne"),
        ("1 kg Kartoffeln", "1 kg", "Kartoffeln"),
        ("500 ml Milch", "500 ml", "Milch"),
    ];
    for (line, qty, name) in cases {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, name, "line: {line}");
        assert_eq!(ingredient.quantity.as_deref(), Some(qty), "line: {line}");
    }
}

#[test]
fn given_german_spoon_units_when_parse_then_split() {
    let cases = [
        ("4 EL Sonnenblumenöl", "4 EL", "Sonnenblumenöl"),
        ("3 EL Olivenöl", "3 EL", "Olivenöl"),
        (
            "1 EL Paprikapulver (edelsüß)",
            "1 EL",
            "Paprikapulver (edelsüß)",
        ),
        ("2 EL Tomatenmark", "2 EL", "Tomatenmark"),
        ("2 TL Salz", "2 TL", "Salz"),
        ("1 Prise Pfeffer", "1 Prise", "Pfeffer"),
    ];
    for (line, qty, name) in cases {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, name, "line: {line}");
        assert_eq!(ingredient.quantity.as_deref(), Some(qty), "line: {line}");
    }
}

#[test]
fn given_bare_count_without_unit_when_parse_then_split() {
    let cases = [
        ("2 rote Paprika", "2", "rote Paprika"),
        ("1 Zwiebel", "1", "Zwiebel"),
        ("3 Eier", "3", "Eier"),
        ("1/2 Zitrone", "1/2", "Zitrone"),
    ];
    for (line, qty, name) in cases {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, name, "line: {line}");
        assert_eq!(ingredient.quantity.as_deref(), Some(qty), "line: {line}");
    }
}

#[test]
fn given_no_leading_number_when_parse_then_name_only() {
    let ingredient = crate::recipe::split_ingredient_line("etwas Salz");
    assert_eq!(ingredient.name, "etwas Salz");
    assert!(ingredient.quantity.is_none());
}

#[test]
fn given_glued_unit_when_parse_then_split() {
    let cases = [
        ("400g Putenschnitzel", "400 g", "Putenschnitzel"),
        ("2EL Öl", "2 EL", "Öl"),
    ];
    for (line, qty, name) in cases {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, name, "line: {line}");
        assert_eq!(ingredient.quantity.as_deref(), Some(qty), "line: {line}");
    }
}

#[test]
fn given_mixed_token_and_unknown_suffix_when_parse_then_name_only() {
    for line in ["7-Kräuter-Mischung", "400xyz Mehl"] {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, line, "line: {line}");
        assert!(ingredient.quantity.is_none(), "line: {line}");
    }
}

#[test]
fn given_spelled_out_german_units_when_parse_then_split() {
    let cases = [
        ("200 Gramm Mehl", "200 Gramm", "Mehl"),
        ("1 Kilogramm Kartoffeln", "1 Kilogramm", "Kartoffeln"),
    ];
    for (line, qty, name) in cases {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, name, "line: {line}");
        assert_eq!(ingredient.quantity.as_deref(), Some(qty), "line: {line}");
    }
}

#[test]
fn given_range_or_pack_size_when_parse_then_name_only() {
    for line in ["1 - 2 Zwiebeln", "1 x 400g Dose Tomaten", "2 400g Dosen"] {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, line, "line: {line}");
        assert!(ingredient.quantity.is_none(), "line: {line}");
    }
}

#[test]
fn given_separator_only_first_token_when_parse_then_name_only() {
    for line in ["/ Eier", "1. Zwiebel"] {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.name, line, "line: {line}");
        assert!(ingredient.quantity.is_none(), "line: {line}");
    }
}

#[test]
fn given_glued_unit_with_trailing_punctuation_when_parse_then_split() {
    let ingredient = crate::recipe::split_ingredient_line("400g, Mehl");
    assert_eq!(ingredient.name, "Mehl");
    assert_eq!(ingredient.quantity.as_deref(), Some("400 g"));
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
        Err(AppError::Import(msg, code)) => {
            assert!(msg.contains("no machine-readable recipe"));
            assert_eq!(code, "import_no_recipe");
        }
        other => panic!("expected Import, got {other:?}"),
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
// Fixture: mixed HowToStep + trailing HowToSection (WPRM shape, e.g.
// madamecuisine.de/kartoffel-bohnen-pfanne with a "Zum Servieren" section).
const HTML_MIXED_STEPS_AND_SECTION: &str = r#"<html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@graph": [
    {"@type": "Article", "@id": "https://example.com/pan/#article", "name": "Article"},
    {
      "@type": "Recipe",
      "@id": "https://example.com/pan/#recipe",
      "name": "Kartoffel-Bohnen-Pfanne",
      "description": "Wuerzige Pfanne.",
      "image": "https://example.com/pan.jpg",
      "recipeYield": ["2", "2 Personen"],
      "recipeIngredient": ["400 g Kartoffeln", "Salz"],
      "recipeInstructions": [
        {"@type": "HowToStep", "text": "Kartoffeln schneiden."},
        {"@type": "HowToStep", "text": "Bohnen kochen."},
        {
          "@type": "HowToSection",
          "name": "Zum Servieren",
          "itemListElement": [
            {"@type": "HowToStep", "text": "Joghurt ruehren und servieren."}
          ]
        }
      ]
    }
  ]
}
</script>
</head><body></body></html>"#;

#[test]
fn given_mixed_steps_and_section_when_parse_then_all_steps_joined() {
    let draft =
        parse_recipe(HTML_MIXED_STEPS_AND_SECTION).expect("should parse mixed instructions");
    assert_eq!(draft.name, "Kartoffel-Bohnen-Pfanne");
    assert_eq!(
        draft.instructions,
        "Kartoffeln schneiden.\nBohnen kochen.\nJoghurt ruehren und servieren."
    );
    assert_eq!(draft.portions, Some(2));
}

// -----------------------------------------------------------------------
// Recipe parser fixes: entities, object-form ingredients, image shapes
// -----------------------------------------------------------------------

#[test]
fn given_top_level_array_when_extract_image_url_then_returns_recipe_image() {
    let json: serde_json::Value = serde_json::json!([
        {
            "@type": "Recipe",
            "name": "Array-Rezept",
            "image": ["https://example.com/array.jpg"],
            "recipeIngredient": ["1 Zwiebel"],
            "recipeInstructions": ["Schneiden."]
        },
        {"@type": "BreadcrumbList", "itemListElement": []}
    ]);
    let result = extract_image_url(&json);
    assert_eq!(result.as_deref(), Some("https://example.com/array.jpg"));
}

#[test]
fn given_object_form_ingredients_when_parse_then_numeric_key_order() {
    let html = r#"<html><head><script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Objekt-Kuchen",
  "recipeIngredient": {"0": "1 Mehl", "2": "2 Butter", "10": "1 Salz"},
  "recipeInstructions": ["Backen."]
}
</script></head><body></body></html>"#;
    let draft = parse_recipe(html).expect("should parse object-form ingredients");
    let names: Vec<&str> = draft.ingredients.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["Mehl", "Butter", "Salz"]);
    assert_eq!(
        draft.ingredients[0].quantity.as_deref(),
        Some("1"),
        "bare count without unit word"
    );
}

#[test]
fn given_html_entities_when_split_then_decoded_quantities() {
    let becher = crate::recipe::split_ingredient_line("&frac12; Becher Kräuterfrischkäse");
    assert_eq!(becher.name, "Kräuterfrischkäse");
    assert_eq!(becher.quantity.as_deref(), Some("½ Becher"));

    let spoon = crate::recipe::split_ingredient_line("1&nbsp;EL Zucker");
    assert_eq!(spoon.name, "Zucker");
    assert_eq!(spoon.quantity.as_deref(), Some("1 EL"));

    let numeric = crate::recipe::split_ingredient_line("&#189; Tasse Milch");
    assert_eq!(numeric.quantity.as_deref(), Some("½ Tasse"));

    let unknown = crate::recipe::decode_html_entities("1 &bogus; Mehl");
    assert_eq!(unknown, "1 &bogus; Mehl");
}

#[test]
fn given_entity_in_instructions_when_parse_then_plain_space() {
    let html = r#"<html><head><script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Recipe",
  "name": "Entitäten-Topf",
  "recipeIngredient": ["1 Zwiebel"],
  "recipeInstructions": ["Teig&nbsp;ruhen lassen."]
}
</script></head><body></body></html>"#;
    let draft = parse_recipe(html).expect("should parse");
    assert_eq!(draft.instructions, "Teig ruhen lassen.");
    assert!(!draft.instructions.contains("&nbsp;"));
}

#[test]
fn given_at_id_image_with_sibling_image_object_when_parse_then_sibling_url() {
    let html = r#"<html><head><script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@graph": [
    {
      "@type": "Recipe",
      "@id": "https://example.com/rezept/#recipe",
      "name": "Chefkoch-Topf",
      "image": {"@id": "https://example.com/rezept/#primaryimage"},
      "recipeIngredient": ["1 Zwiebel"],
      "recipeInstructions": ["Kochen."]
    },
    {
      "@type": "ImageObject",
      "@id": "https://example.com/rezept/#primaryimage",
      "url": "https://example.com/chefkoch.jpg"
    }
  ]
}
</script></head><body></body></html>"#;
    let block: serde_json::Value = {
        let document = scraper::Html::parse_document(html);
        let selector = scraper::Selector::parse(r#"script[type="application/ld+json"]"#)
            .expect("static selector");
        let text = document
            .select(&selector)
            .next()
            .expect("one block")
            .text()
            .collect::<String>();
        serde_json::from_str(&text).expect("valid JSON")
    };
    // The Recipe node itself has no usable URL; the block-level helper
    // recovers the sibling ImageObject URL.
    let recipe_node = block
        .get("@graph")
        .and_then(|g| g.as_array())
        .and_then(|g| {
            g.iter()
                .find(|n| n.get("@type").and_then(|t| t.as_str()) == Some("Recipe"))
        })
        .expect("recipe node");
    assert!(extract_image_url(recipe_node).is_none());
    let urls = extract_image_urls_from_html(html, "https://example.com/rezept/");
    assert!(
        urls.iter().any(|u| u == "https://example.com/chefkoch.jpg"),
        "sibling ImageObject URL found, got: {urls:?}"
    );
}

#[test]
fn given_dl_cl_units_when_split_then_quantity_includes_unit() {
    for (line, qty) in [("1 dl Weisswein", "1 dl"), ("2 cl Essig", "2 cl")] {
        let ingredient = crate::recipe::split_ingredient_line(line);
        assert_eq!(ingredient.quantity.as_deref(), Some(qty), "line: {line}");
    }
}

// ---------------------------------------------------------------------------
// Cookidoo: public recipe pages omit the preparation steps for anonymous
// visitors (login-gated). The mobile API serves the full recipe without
// authentication, so cookidoo URLs are imported from there.
// ---------------------------------------------------------------------------

const COOKIDOO_PAGE_URL: &str = "https://cookidoo.de/recipes/recipe/de-DE/r47931";

// Shape mirrors a real `de.tmmobile.vorwerk-digital.com` recipe payload:
// ingredients carry quantity/unit/preparation/optional flags, steps contain
// <nobr> setting annotations and HTML entities, and image assets use a
// {transformation} slot.
const COOKIDOO_API_JSON: &str = r#"{
  "title": "Gewürzpaste für Gemüsebrühe",
  "servingSize": {"quantity": {"value": 650}, "unitNotation": "g"},
  "recipeIngredientGroups": [
    {
      "title": "",
      "recipeIngredients": [
        {"quantity": {"value": 50}, "unitNotation": "g", "ingredientNotation": "Parmesan", "preparation": "in Stücken (3 cm)", "optional": true},
        {"quantity": {"value": 200}, "unitNotation": "g", "ingredientNotation": "Staudensellerie", "preparation": "in Stücken (3 cm), siehe Tipp", "optional": false},
        {"quantity": {"value": 1}, "unitNotation": "", "ingredientNotation": "Knoblauchzehe", "preparation": "", "optional": false},
        {"quantity": {"value": 1}, "unitNotation": "EL", "ingredientNotation": "Olivenöl", "preparation": "", "optional": false},
        {"quantity": {"value": 40}, "unitNotation": "g", "ingredientNotation": "Butter", "preparation": ", in Stücken", "optional": false},
        {"ingredientNotation": "Salz zum Abschmecken", "preparation": "", "optional": false}
      ]
    }
  ],
  "recipeStepGroups": [
    {"title": "", "recipeSteps": [
      {"title": "1", "formattedText": "Parmesan in den Mixtopf geben, <nobr>Pürieren /10 Sek.</nobr> und umfüllen."},
      {"title": "2", "formattedText": "Gemüse <NOBR>10 Sek./Stufe 5</NOBR> zerkleinern."},
      {"title": "3", "formattedText": "Mit &frac12; TL Salz abschmecken."}
    ]}
  ],
  "descriptiveAssets": [
    {"type": "image",
     "square": "https://assets.tmecosys.com/image/upload/{transformation}/img/recipe/ras/Assets/abc/Derivates/def-square",
     "landscape": "https://assets.tmecosys.com/image/upload/{transformation}/img/recipe/ras/Assets/abc/Derivates/def"}
  ]
}"#;

#[test]
fn given_cookidoo_de_recipe_url_when_resolving_api_url_then_maps_to_mobile_api() {
    assert_eq!(
        cookidoo_api_url(COOKIDOO_PAGE_URL),
        Some("https://de.tmmobile.vorwerk-digital.com/recipes/recipe/de-DE/r47931".to_string())
    );
}

#[test]
fn given_cookidoo_url_with_query_and_fragment_when_resolving_api_url_then_both_dropped() {
    let url = "https://cookidoo.de/recipes/recipe/de-DE/r47931?utm_source=x#steps";
    assert_eq!(
        cookidoo_api_url(url),
        Some("https://de.tmmobile.vorwerk-digital.com/recipes/recipe/de-DE/r47931".to_string())
    );
}

#[test]
fn given_non_cookidoo_host_when_resolving_api_url_then_none() {
    assert!(cookidoo_api_url("https://example.com/recipes/recipe/de-DE/r47931").is_none());
}

#[test]
fn given_cookidoo_multi_label_market_url_when_resolving_api_url_then_maps_to_market_api_label() {
    for (page_url, api_url) in [
        (
            "https://cookidoo.co.uk/recipes/recipe/en-GB/r351240",
            "https://gb.tmmobile.vorwerk-digital.com/recipes/recipe/en-GB/r351240",
        ),
        (
            "https://cookidoo.com.au/recipes/recipe/en-AU/r351240",
            "https://au.tmmobile.vorwerk-digital.com/recipes/recipe/en-AU/r351240",
        ),
        (
            "https://cookidoo.com.tr/recipes/recipe/tr-TR/r351240",
            "https://tr.tmmobile.vorwerk-digital.com/recipes/recipe/tr-TR/r351240",
        ),
    ] {
        assert_eq!(
            cookidoo_api_url(page_url),
            Some(api_url.to_string()),
            "page url: {page_url}"
        );
    }
}

#[test]
fn given_unknown_multi_label_cookidoo_host_when_resolving_api_url_then_none() {
    for page_url in [
        "https://cookidoo.co.jp/recipes/recipe/ja-JP/r351240",
        "https://cookidoo.com.mx/recipes/recipe/es-MX/r351240",
        "https://cookidoo.co.nz/recipes/recipe/en-NZ/r351240",
    ] {
        assert!(cookidoo_api_url(page_url).is_none(), "page url: {page_url}");
    }
}

#[test]
fn given_cookidoo_thermomix_com_url_when_resolving_api_url_then_maps_to_us_mobile_api() {
    // `https://cookidoo.us/...` 307-redirects here with the path preserved
    // (driver-verified: en-US r219904), and the page's JSON-LD carries no
    // instructions, so the US market must resolve to the mobile API rather
    // than fall back to HTML parsing.
    assert_eq!(
        cookidoo_api_url("https://cookidoo.thermomix.com/recipes/recipe/en-US/r219904"),
        Some("https://us.tmmobile.vorwerk-digital.com/recipes/recipe/en-US/r219904".to_string())
    );
}

#[test]
fn given_cookidoo_thermomix_vn_url_when_resolving_api_url_then_maps_to_vn_mobile_api() {
    // The Vietnamese page lives on `cookidoo.thermomix.vn` and its JSON-LD
    // Recipe carries ingredients but no instructions (page 200 for vi-VN
    // r939387); the `vn` mobile API serves 4 steps and 9 ingredients for the
    // same recipe, so it must win over HTML parsing.
    assert_eq!(
        cookidoo_api_url("https://cookidoo.thermomix.vn/recipes/recipe/vi-VN/r939387"),
        Some("https://vn.tmmobile.vorwerk-digital.com/recipes/recipe/vi-VN/r939387".to_string())
    );
}

#[test]
fn given_cookidoo_host_without_working_mobile_api_when_resolving_api_url_then_none() {
    // Both pages exist on the web, but `cn.tmmobile...` and
    // `international.tmmobile...` answer 502, so neither host may resolve to
    // an API URL; both must fall back to generic HTML parsing.
    for page_url in [
        "https://cookidoo.com.cn/recipes/recipe/zh-CN/r100",
        "https://cookidoo.international/recipes/recipe/en-US/r219904",
    ] {
        assert!(cookidoo_api_url(page_url).is_none(), "page url: {page_url}");
    }
}

#[test]
fn given_cookidoo_page_without_recipe_id_when_resolving_api_url_then_none() {
    assert!(cookidoo_api_url("https://cookidoo.de/recipes/search?q=apfel").is_none());
    assert!(cookidoo_api_url("https://cookidoo.de/recipes/recipe/de-DE/custom-abc").is_none());
}

#[test]
fn given_cookidoo_api_payload_when_building_draft_then_maps_recipe_fields() {
    let json: serde_json::Value = serde_json::from_str(COOKIDOO_API_JSON).expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    assert_eq!(draft.name, "Gewürzpaste für Gemüsebrühe");
    // The fixture's servingSize is 650 g, a mass — not a serving count.
    assert_eq!(draft.portions, None);
    assert_eq!(draft.ingredients.len(), 6);

    assert_eq!(draft.ingredients[0].quantity.as_deref(), Some("50 g"));
    assert_eq!(
        draft.ingredients[0].name,
        "Parmesan (optional), in Stücken (3 cm)"
    );
    assert_eq!(draft.ingredients[1].quantity.as_deref(), Some("200 g"));
    assert_eq!(
        draft.ingredients[1].name,
        "Staudensellerie, in Stücken (3 cm), siehe Tipp"
    );
    assert_eq!(draft.ingredients[2].quantity.as_deref(), Some("1"));
    assert_eq!(draft.ingredients[2].name, "Knoblauchzehe");
    assert_eq!(draft.ingredients[3].quantity.as_deref(), Some("1 EL"));
    assert_eq!(draft.ingredients[3].name, "Olivenöl");
    assert_eq!(draft.ingredients[4].quantity.as_deref(), Some("40 g"));
    assert_eq!(draft.ingredients[4].name, "Butter, in Stücken");
    assert_eq!(draft.ingredients[5].quantity, None);
    assert_eq!(draft.ingredients[5].name, "Salz zum Abschmecken");

    assert_eq!(
        draft.instructions,
        "Parmesan in den Mixtopf geben, Pürieren /10 Sek. und umfüllen.\nGemüse 10 Sek./Stufe 5 zerkleinern.\nMit ½ TL Salz abschmecken."
    );
}

#[test]
fn given_cookidoo_api_payload_without_title_when_building_draft_then_none() {
    let json: serde_json::Value =
        serde_json::from_str(r#"{"recipeStepGroups": []}"#).expect("fixture parses");
    assert!(draft_from_cookidoo_json(&json).is_none());
}

// `servingSize.unitNotation` decides whether the `quantity.value` is a serving
// count or a size: a gram/container size must not import as "Serves 650".
#[test]
fn given_cookidoo_portion_unit_when_building_draft_then_portions_kept() {
    for (unit, portions) in [
        ("Portionen", 4),
        ("μερίδες", 3),
        ("raciones", 6),
        ("Port.", 2),
        // Live count/portion units the singular-only allow-list dropped:
        // fr-FR r375093/r816989/r790212 and de-DE r51900/r47908.
        ("portions", 4),
        ("parts", 6),
        ("pièces", 20),
        ("Stück", 15),
        ("Stücke", 8),
        ("Scheiben", 20),
        ("porzioni", 6),
        ("人份", 8),
        // Live count nouns no allow-list can enumerate (mobile API,
        // 2026-10-14): pl-PL r253592/r617817/r782476 ("kromek"), pl-PL
        // r484907/r617819 ("kromki"), pl-PL r63596 ("kawałków"), el-GR
        // r241401/r241419/r951552/r951554/r951556 ("κομμάτια"), el-GR
        // r80434/r907188/r917115/r929888 ("φέτες"), nl-BE r944398
        // ("sneetjes").
        ("kromek", 28),
        ("kromki", 24),
        ("kawałków", 12),
        ("κομμάτια", 12),
        ("φέτες", 40),
        ("sneetjes", 15),
    ] {
        let json: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"title": "Test", "servingSize": {{"quantity": {{"value": {portions}}}, "unitNotation": "{unit}"}}}}"#
        ))
        .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");
        assert_eq!(draft.portions, Some(portions), "unit: {unit}");
    }
}

// `unitNotation` is a serving count unless it names a measure or a container:
// count nouns are an open class across markets, so no allow-list can enumerate
// them and an unlisted — even invented — noun still imports as a count.
#[test]
fn given_unlisted_count_noun_when_building_draft_then_portions_kept() {
    for (language, unit, portions) in [
        ("de", "Portion", 4),
        ("sv", "portioner", 8), // sv r812711 (first pass) / be r903936
        ("nl", "porties", 2),   // at r922115
        ("nl", "stuks", 6),     // pt r786236
        ("da", "skiver", 12),   // es r749057
        ("no", "porsjoner", 6), // ch r585415
        ("is", "sneiðar", 16),  // ch r699389
        ("is", "skammtar", 4),  // fr r590724
        ("hu", "adag", 24),     // be r801835
        ("hu", "darab", 45),    // pt r698096
        ("ro", "bucăți", 12),   // ch r811456
        ("ro", "felii", 30),    // us r695252
        ("cs", "kusů", 12),     // at r785634
        ("cs", "plátků", 17),   // pl r760712
        ("id", "potongan", 12), // ca r774097
        ("vi", "khẩu phần", 6), // fr r938266
        ("tr", "dilim", 8),     // tr r169319
        ("pl", "porcję", 1),    // de r827735
        ("es", "unidades", 20), // at r268169
        ("en", "slices", 20),   // at r968488
        // Invented noun: nothing lists it, and it is still a count.
        ("xx", "kuswiatki", 7),
    ] {
        let json: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"title": "Test", "language": "{language}", "servingSize": {{"quantity": {{"value": {portions}}}, "unitNotation": "{unit}"}}}}"#
        ))
        .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");
        assert_eq!(draft.portions, Some(portions), "unit: {unit} ({language})");
    }
}

// Live `servingSize.unitNotation` count nouns that the singular-only allow
// list and its plural endings dropped; each recipe id was checked against the
// mobile API on 2026-10-14.
#[test]
fn given_cookidoo_live_count_unit_when_building_draft_then_portions_kept() {
    for (unit, language, portions) in [
        ("adet", "tr", 30),                   // tr r171339
        ("porties", "nl", 8),                 // nl r75400
        ("stuks", "nl", 4),                   // nl r75344
        ("porsi", "id", 3),                   // id r751291
        ("个", "zh-Hans", 40),                // zh-Hans r98275
        ("個", "zh-Hant", 10),                // zh-Hant r147280
        ("fette", "it", 12),                  // it r94998
        ("morceaux", "fr", 18),               // fr r54178
        ("morceau/part", "fr", 16),           // fr r928254
        ("stykki", "is", 18),                 // is r590664
        ("bahagian", "ms", 10),               // ms r919528
        ("rebanada/loncha/rodaja", "es", 24), // es r824675
        ("rebanadas", "es", 24),              // es r500172
        ("sztuki", "pl", 2),                  // pl r608581
        // Live count nouns sampled from the mobile API on 2026-10-14; the
        // "<locale> complete recipe" phrasings carry the whole-recipe count.
        ("kromek", "pl", 28),           // pl-PL r253592
        ("kromki", "pl", 24),           // pl-PL r484907
        ("kawałków", "pl", 12),         // pl-PL r63596
        ("κομμάτια", "el", 12),         // el-GR r241401
        ("φέτες", "el", 40),            // el-GR r80434
        ("sneetjes", "nl", 15),         // nl-BE r944398
        ("unidades", "es", 20),         // at(de-AT) r268169
        ("slices", "en", 16),           // at(de-AT) r968488
        ("stykker", "da", 14),          // ch(de-CH) r798146
        ("bitar", "sv", 12),            // mx(es-MX) r619221
        ("potong", "id", 12),           // be(nl-BE) r790048
        ("bucăți", "ro", 20),           // gr(el-GR) r780797
        ("plátků", "cs", 17),           // pl(pl-PL) r760712
        ("dilim", "tr", 8),             // tr(tr-TR) r169319
        ("komplettes Rezept", "de", 1), // at(de-AT) r771615
        ("recette complète", "fr", 1),  // be(fr-BE) r536346
        ("Ricetta completa", "it", 1),  // be(fr-BE) r792558
        ("完整食谱", "zh-Hans", 1),     // ch(de-CH) r104625
        ("整份食譜", "zh-Hant", 1),     // tr(tr-TR) r773982
        ("Cały przepis", "pl", 1),      // us(en-US) r102301
        ("Complete recipe", "en", 1),   // be(nl-BE) r689585
        // Count nouns from the 2026-10-14 sweep (~99 000 requests, 3 349+
        // payloads): the widened measure/container matching must not reject
        // them. Each id is a live payload checked against the mobile API.
        ("порции", "bg", 6),   // vi-VN r951015
        ("порций", "ru", 6),   // de-CH r74888
        ("ชิ้น", "th", 100),     // el-GR r921637
        ("piece", "en", 16),   // de-DE r901970
        ("kousků", "cs", 10),  // de-DE r730678
        ("片", "zh-Hant", 16), // de-CH r56508
        ("φέτα", "el", 6),     // nl-BE r907751
        ("skiver", "no", 16),  // de-AT r585510
        ("顆", "zh-Hant", 20), // zh-TW r950559
    ] {
        let json: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"title": "Test", "language": "{language}", "servingSize": {{"quantity": {{"value": {portions}}}, "unitNotation": "{unit}"}}}}"#
        ))
        .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");
        assert_eq!(draft.portions, Some(portions), "unit: {unit} ({language})");
    }
}

// "dose"/"doses" is the Portuguese serving noun; it is accepted only when the
// payload language starts with "pt", so the German container "Dose"/"Dosen"
// stays rejected.
#[test]
fn given_portuguese_language_when_building_draft_then_dose_stays_portions() {
    for unit in ["dose", "doses"] {
        for fields in [
            r#""language": "pt","#,
            r#""locale": "pt-BR","#,
            r#""language": "", "locale": "pt-BR","#,
        ] {
            let json: serde_json::Value = serde_json::from_str(&format!(
                r#"{{"title": "Test", {fields} "servingSize": {{"quantity": {{"value": 12}}, "unitNotation": "{unit}"}}}}"#
            ))
            .expect("fixture parses");
            let draft = draft_from_cookidoo_json(&json).expect("should build draft");
            assert_eq!(draft.portions, Some(12), "unit: {unit} ({fields})");
        }
    }
}

#[test]
fn given_german_language_when_building_draft_then_dose_stays_none() {
    for unit in ["Dose", "Dosen"] {
        let json: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"title": "Test", "language": "de", "servingSize": {{"quantity": {{"value": 2}}, "unitNotation": "{unit}"}}}}"#
        ))
        .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");
        assert_eq!(draft.portions, None, "unit: {unit}");
    }
}

#[test]
fn given_missing_or_unknown_language_when_building_draft_then_dose_stays_none() {
    for unit in ["dose", "doses"] {
        for language_field in ["", r#""language": "xx","#, r#""language": "","#] {
            let json: serde_json::Value = serde_json::from_str(&format!(
                r#"{{"title": "Test", {language_field} "servingSize": {{"quantity": {{"value": 12}}, "unitNotation": "{unit}"}}}}"#
            ))
            .expect("fixture parses");
            let draft = draft_from_cookidoo_json(&json).expect("should build draft");
            assert_eq!(
                draft.portions, None,
                "unit: {unit} (field: {language_field})"
            );
        }
    }
}

#[test]
fn given_cookidoo_serving_size_without_unit_when_building_draft_then_portions_kept() {
    // A missing, blank or whitespace-only unit means the value *is* the count.
    for serving_size in [
        r#""servingSize": {"quantity": {"value": 2}}"#,
        r#""servingSize": {"quantity": {"value": 2}, "unitNotation": ""}"#,
        r#""servingSize": {"quantity": {"value": 2}, "unitNotation": "   "}"#,
        r#""servingSize": {"quantity": {"value": 2}, "unitNotation": null}"#,
    ] {
        let json: serde_json::Value =
            serde_json::from_str(&format!(r#"{{"title": "Test", {serving_size}}}"#))
                .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");
        assert_eq!(draft.portions, Some(2), "fixture: {serving_size}");
    }
}

#[test]
fn given_cookidoo_serving_size_with_container_unit_when_building_draft_then_portions_none() {
    // Real masses, volumes and containers seen in live payloads; none may
    // import as "Serves 650".
    for unit in [
        "g",
        "kg",
        "mg",
        "ml",
        "l",
        "cl",
        "dl",
        "oz",
        "Gramm",
        "gram",
        "gramo",
        "gramů",
        "litro",
        "Flasche",
        "Flaschen",
        "Schraubgläser",
        "Gläser",
        "vasetti",
        "ποτήρια",
        "Becher",
        "Packung",
        "Prise",
        "Prisen",
        // German container word — ambiguous with the Portuguese portion noun
        // "dose", so it must not import as a serving count.
        "Dose",
        "Dosen",
        // Containers from the other languages in the live sample; none may
        // become a serving count either.
        "flöskur",
        "szklanki",
        "verres",
        "bicchieri",
        "glazen",
        "pot",
        "botol",
        "gelas",
        "EL",
        "TL",
        "cm",
        // Measures and containers from the 2026-10-14 sweep of the mobile
        // API (recipe ids named in `COOKIDOO_MEASURE_OR_CONTAINER_UNITS`),
        // including qualified and multi-word container notations.
        "gam",                    // vi r939387
        "γρ.",                    // el r640582
        "grammi",                 // it r953193
        "gramos",                 // es r624650
        "gramas",                 // ca r969343
        "grames",                 // be r1036420
        "grammes",                // be r1036420
        "gramů",                  // be r143208
        "oz",                     // ca r544109
        "cups",                   // fr r420330
        "杯",                     // ca r163872
        "cốc",                    // it r666070
        "chávenas",               // pl r762669
        "bardak",                 // pt r505651
        "jar",                    // at r981728
        "jars",                   // de r1010643
        "glass",                  // en (container)
        "glasses",                // ch r985866
        "verres",                 // be r972459
        "vasos",                  // mx r123375
        "copo",                   // pt r761346
        "copos",                  // ca r1104320
        "tarros",                 // gr r13136
        "bocaux",                 // tr r77681
        "frasco de",              // ch r774143
        "frascos de",             // it r759493
        "słoik",                  // vn r5858
        "słoiki",                 // at r10919
        "słoików",                // pt r970948
        "butelka",                // pt r798279
        "butelki",                // it r994886
        "bouteilles",             // gr r991971
        "lahve",                  // tr r931981
        "szklanki",               // mx r985610
        "szklanek",               // pl r970422
        "sklenic",                // gb r679692
        "zavařovací sklenice",    // be r802206
        "Flaschen (jede 0,75 l)", // us r779014
        "罐",                     // tw r591880
        "bicchiere",              // ch r434774
        "vasetto",                // ch r383883
        // Second, larger sweep (2026-10-14, 54 400 requests / 3 958 payloads)
        // found these measures and containers still unterminated.
        "克",            // zh-Hans gr r611564 (350 克)
        "瓶",            // zh-Hans be r626265
        "банка",         // ru be r934106
        "Einmachgläser", // de be r902111
        "Schraubglas",   // de pl r439366
        "Tassen",        // de at r1104340
        "tasses",        // fr es r381739
        "tazze",         // it ca r796584
        "bottles",       // en ch r994881
        "bottiglie",     // it tr r4771
        "garrafa",       // pt at r1084517
        "garrafas",      // pt us r1285742
        "krukke",        // da es r911763
        "láhve",         // cs vn r6925
        "glös",          // is at r797841
        "ly",            // vi ca r938802
        "pahare",        // ro ca r779969
        "βαζάκια",       // el de r907097
        // Third sweep (2026-10-14, ~84 000 requests; ids named in
        // `COOKIDOO_MEASURE_OR_CONTAINER_UNITS`) found these measures and
        // containers still accepted as serving counts.
        "litres",           // pl-PL r1402688 (3 litres)
        "φλιτζάνια",        // el-GR r808945 (2 φλιτζάνια)
        "kopper",           // el-GR r208280 (4 kopper)
        "flaske",           // nl-BE r808585 (1 flaske)
        "kavanoz",          // tr-TR r904525 (2 kavanoz)
        "botella (750 ml)", // es-MX r140353 (4 botella (750 ml))
        "szklanka",         // pl-PL r933775 (1 szklanka)
        "chávena",          // de-DE r955901 (2 chávena)
        // Fourth, larger sweep (2026-10-14, ~99 000 requests, 3 349+ payloads;
        // ids named in `COOKIDOO_MEASURE_OR_CONTAINER_UNITS`).
        "borcane",       // de-DE r154084 (2 borcane, ro)
        "tazas",         // es-MX r25572 (6 tazas)
        "taza",          // singular of the above
        "tazza",         // it-IT r796587 (2 tazza)
        "fincan",        // tr-TR r825712 (4 fincan)
        "litra",         // pl-PL r244089 (1 litra)
        "šálky",         // de-DE r6919 (4 šálky, cs)
        "căni",          // nl-BE r778876 (6 căni, ro)
        "стъклени чаши", // vi-VN r930039 (4 стъклени чаши, bg)
        "грам, гр.",     // de-DE r931831 (1000 грам, гр., bg)
        // Full sweep (2026-10-14, 141 500 requests, 5 537 payloads; ids named
        // in `COOKIDOO_MEASURE_OR_CONTAINER_UNITS`).
        "βαζάκι",       // zh-TW r764166 (1 βαζάκι, el)
        "sticlă",       // fr-FR r809005 (1 sticlă, ro)
        "pohár",        // pl-PL r611882 (4 pohár, hu)
        "filiżanki",    // fr-FR r959713 (4 filiżanki, pl)
        "láhev",        // pl-PL r6924 (1 láhev, cs)
        "杯（玻璃杯）", // it-IT r904487 (4 杯（玻璃杯）, zh-Hans)
        // Fifth sweep (2026-10-14: 160 011 requests across the 19 answering
        // hosts, plus focused Greek and rare-language sweeps; ids named in
        // `COOKIDOO_MEASURE_OR_CONTAINER_UNITS`): containers still accepted,
        // the spelled-out Greek grams, and every inflection the configured
        // plural endings cannot bridge.
        "bottiglia",   // it-IT r1005382 (1 bottiglia)
        "sticle",      // ro-RO r806852 (3 sticle)
        "üveg",        // hu r790666 (1 befőttes üveg)
        "üvegek",      // plural of "üveg"
        "γραμμάρια",   // el-GR r934408 (200 γραμμάρια)
        "γραμμάριο",   // el singular of the live-verified "γραμμάρια"
        "μπουκάλια",   // el-GR r750285 (6 μπουκάλια)
        "μπουκάλι",    // el singular of "μπουκάλια"
        "φλυτζάνια",   // el r80192 (4 φλυτζάνια)
        "φλυτζάνι",    // el singular of "φλυτζάνια"
        "πρέζα",       // el r782201 (ingredient measure, a pinch)
        "πρέζες",      // el r818491 (ingredient measure)
        "grammo",      // it singular of "grammi"
        "gramy",       // pl/cs plural of "gram"
        "litry",       // pl plural of "litra"
        "litrów",      // pl genitive plural of "litra"
        "litr",        // pl masculine of "litra"
        "bardaklar",   // tr plural of "bardak"
        "łyżek",       // pl genitive plural of "łyżka"
        "ποτήρι",      // el singular of "ποτήρια"
        "φλιτζάνι",    // el singular of "φλιτζάνια"
        "lahví",       // cs genitive plural of "láhve"
        "šálek",       // cs singular of "šálky"
        "šálků",       // cs genitive plural of "šálky"
        "lžic",        // cs genitive plural of "lžíce"
        "butelek",     // pl genitive plural of "butelki"
        "flaska",      // is singular of "flöskur"
        "einmachglas", // de singular of "Einmachgläser"
        "банки",       // ru plural of "банка"
        "банок",       // ru genitive plural of "банка"
        "kop",         // da/no singular of "kopper"
        "kavanozlar",  // tr plural of "kavanoz"
        "fincanlar",   // tr plural of "fincan"
        "cană",        // ro singular of "căni"
        "чаша",        // bg singular of "чаши"
        "грама",       // bg plural of "грам"
        "poharak",     // hu plural of "pohár"
        "filiżanka",   // pl singular of "filiżanki"
        "filiżanek",   // pl genitive plural of "filiżanki"
        // The sweep's accepted serving units; each is a live payload.
        "litri",     // it (de-AT r1109692, 2 litri)
        "ounce",     // fr (pt-PT r904426, 5 ounce)
        "copas",     // es (en-AU r910927, 6 copas)
        "beker",     // nl (es-ES r75454, 1 beker)
        "potjes",    // nl (es-MX r75455, 2 potjes)
        "potten",    // nl (pt-PT r75453, 4 potten)
        "krukka",    // is (pl-PL r783696, 1 krukka)
        "krukkur",   // is plural of "krukka"
        "koppar",    // sv (en-GB r798807, 6 koppar)
        "cangkir",   // id (de-CH r781140, 6 cangkir)
        "ถ้วย",       // th (zh-TW r926293, 6 ถ้วย)
        "แก้ว",       // th (pt-PT r931435, 4 แก้ว)
        "hũ",        // vi (en-AU r668301, 1 hũ)
        "كوب زجاجي", // ar (nl-BE r944955, 6 كوب زجاجي)
        "brd",       // tr (pt-PT r260849, 4 brd)
    ] {
        let json: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"title": "Test", "servingSize": {{"quantity": {{"value": 650}}, "unitNotation": "{unit}"}}}}"#
        ))
        .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");
        assert_eq!(draft.portions, None, "unit: {unit}");
    }
}

// A payload unit and a rejection entry are matched symmetrically: either side
// may carry the inflected form, so a listed plural rejects its singular payload
// word (`grammes` -> `gramme`) and a listed singular rejects the plural
// (`flasche` -> `Flaschen`). The old lookup stripped endings off the payload
// word only, so a singular payload never reached a listed plural.
#[test]
fn given_cookidoo_serving_unit_inflected_form_when_building_draft_then_portions_none() {
    for (unit, language) in [
        // Singular payload of a listed plural.
        ("gramme", "fr"),   // singular of the listed "grammes" (be r1036420)
        ("litre", "fr"),    // singular of "litres" (pl-PL r1402688)
        ("botella", "es"),  // singular of "botellas" (es-MX r140353)
        ("szklanka", "pl"), // pl-PL r933775 (listed "szklanki")
        ("chávena", "pt"),  // de-DE r955901 (listed "chávenas")
        ("butelka", "pl"),  // listed singular of "butelki"
        // Plural payload of a listed singular.
        ("grammes", "fr"),
        ("litres", "fr"),
        ("botellas", "es"),
        ("szklanki", "pl"),
        ("chávenas", "pt"),
        ("butelki", "pl"),
        ("flaske", "da"), // singular container, listed
        ("flasker", "da"),
        ("taza", "es"), // singular of the listed "tazas" (es-MX r25572)
        ("tazas", "es"),
        ("tazza", "it"), // it-IT r796587 (listed "tazze")
        ("tazze", "it"),
        // Fifth sweep singular/plural pairs, both directions, for the entries
        // added there (ids in `COOKIDOO_MEASURE_OR_CONTAINER_UNITS`).
        ("bottiglia", "it"), // it-IT r1005382 (listed "bottiglie")
        ("bottiglie", "it"),
        ("sticle", "ro"), // ro-RO r806852 (listed "sticlă")
        ("sticlă", "ro"),
        ("üveg", "hu"), // hu r790666 (1 befőttes üveg)
        ("üvegek", "hu"),
        ("grammo", "it"), // singular of the listed "grammi"
        ("grammi", "it"),
        ("litry", "pl"), // plural of the listed "litra" (pl-PL r244089)
        ("litrów", "pl"),
        ("litra", "pl"),
        ("ποτήρι", "el"), // singular of the listed "ποτήρια"
        ("ποτήρια", "el"),
        ("φλιτζάνι", "el"), // singular of the listed "φλιτζάνια"
        ("φλιτζάνια", "el"),
        ("šálek", "cs"), // singular of the listed "šálky" (de-DE r6919)
        ("šálky", "cs"),
        ("šálků", "cs"),
        ("банки", "ru"), // plural of the listed "банка" (be r934106)
        ("банок", "ru"),
        ("банка", "ru"),
    ] {
        let json: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"title": "Test", "language": "{language}", "servingSize": {{"quantity": {{"value": 2}}, "unitNotation": "{unit}"}}}}"#
        ))
        .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");
        assert_eq!(draft.portions, None, "unit: {unit} ({language})");
    }
}

#[test]
fn given_cookidoo_api_payload_when_resolving_image_url_then_landscape_without_placeholder() {
    let json: serde_json::Value = serde_json::from_str(COOKIDOO_API_JSON).expect("fixture parses");
    assert_eq!(
        cookidoo_image_url(&json),
        Some(
            "https://assets.tmecosys.com/image/upload/t_web_rdp_recipe_584x480_1_5x/img/recipe/ras/Assets/abc/Derivates/def.jpg"
                .to_string()
        )
    );
}

// Range quantities: the mobile API serves `{"from": x, "to": y}` without a
// `value`, which used to drop both amount and unit.
const COOKIDOO_RANGE_JSON: &str = r#"{
  "title": "Apfel-Ingwer-Shot",
  "recipeIngredientGroups": [
    {
      "recipeIngredients": [
        {"ingredientNotation": "Ingwer, frisch", "quantity": {"to": 100, "from": 80}, "unitNotation": "g", "preparation": "in dünnen Scheiben"},
        {"ingredientNotation": "Zitrone", "quantity": {"from": 2}, "unitNotation": ""},
        {"ingredientNotation": "Honig", "quantity": {"to": 3}, "unitNotation": "EL"}
      ]
    }
  ]
}"#;

#[test]
fn given_cookidoo_range_quantity_when_building_draft_then_range_and_unit_kept_in_name() {
    let json: serde_json::Value =
        serde_json::from_str(COOKIDOO_RANGE_JSON).expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    // A two-bound range stays name-only: downstream consumers read just the
    // leading number, so "80-100 g" as a quantity would render as "80 -100 g".
    assert_eq!(draft.ingredients[0].quantity, None);
    assert_eq!(
        draft.ingredients[0].name,
        "80 - 100 g Ingwer, frisch, in dünnen Scheiben"
    );
    // A single bound still yields an amount.
    assert_eq!(draft.ingredients[1].quantity.as_deref(), Some("2"));
    assert_eq!(draft.ingredients[1].name, "Zitrone");
    assert_eq!(draft.ingredients[2].quantity.as_deref(), Some("3 EL"));
    assert_eq!(draft.ingredients[2].name, "Honig");
}

#[test]
fn given_cookidoo_range_quantity_when_building_draft_then_quantity_is_not_digit_leading_range() {
    let json: serde_json::Value =
        serde_json::from_str(COOKIDOO_RANGE_JSON).expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    for line in &draft.ingredients {
        let Some(quantity) = &line.quantity else {
            continue;
        };
        // Downstream `parse_numeric_quantity` splits on the first non-digit, so a
        // "80-100 g" quantity would leave "-100 g" as the unit.
        let (_, unit) = crate::plan::parse_numeric_quantity(quantity)
            .unwrap_or_else(|| panic!("quantity must stay parse-safe: {quantity}"));
        assert!(
            !unit.starts_with('-'),
            "quantity must not be a digit-leading range: {quantity}"
        );
    }
}

// Cookidoo encodes "no amount given" as `{"value": 0}`; `cookidoo_amount_text`
// rendered the number verbatim, so the meal quantity became "0" — or "0
// quelques" with a qualitative unit. The 2026-10-14 sweep saw the shape in
// 6 181 of 23 945 payloads (9 038 ingredient lines), e.g. de-AT r1017650 "sal".
// The public page renders those lines without an amount and keeps the
// qualitative word next to the ingredient (" quelques brins de coriandre
// fraîche", fr-FR r819463; " λίγο ελαιόλαδο", el-GR r749573).
#[test]
fn given_cookidoo_zero_quantity_when_building_draft_then_quantity_none() {
    let json: serde_json::Value = serde_json::from_str(
        r#"{"title": "Test", "language": "es", "recipeIngredientGroups": [{"recipeIngredients": [
            {"quantity": {"value": 0}, "unitNotation": "", "ingredientNotation": "sal"},
            {"quantity": {"value": 0}, "unitNotation": "g", "ingredientNotation": "pimienta molida"}
        ]}]}"#,
    )
    .expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    // de-AT r1017650 carries both lines: a bare zero and a zero with a measure
    // unit. Neither may import as "0" (or "0 g"). A measure carries no amount
    // information on its own (the public page prints " g di farina di forza",
    // ca r65261), so the word disappears with the amount and the name stays bare.
    assert_eq!(draft.ingredients[0].quantity, None);
    assert_eq!(draft.ingredients[0].name, "sal");
    assert_eq!(draft.ingredients[1].quantity, None);
    assert_eq!(draft.ingredients[1].name, "pimienta molida");
}

#[test]
fn given_cookidoo_zero_quantity_with_qualitative_unit_when_building_draft_then_unit_kept_in_name() {
    // A zero amount with a qualitative unit word: the amount is dropped but the
    // word stays visible, as the public page renders it. Each word was observed
    // on a live zero line of the 2026-10-14 sweep.
    for (language, unit, ingredient, expected_name) in [
        (
            "fr",
            "quelques",
            "brins de coriandre fraîche",
            "quelques brins de coriandre fraîche",
        ), // fr-FR r819463
        ("zh-Hans", "适量", "盐", "适量 盐"), // it-IT r250249
        ("el", "λίγο", "ελαιόλαδο", "λίγο ελαιόλαδο"), // el-GR r749573
        ("de", "etwas", "Pfeffer", "etwas Pfeffer"), // de r163751
        ("pt", "q.b.", "sal", "q.b. sal"),    // pt r111309
        ("de", "Blatt", "Petersilie", "Blatt Petersilie"),
    ] {
        let json: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"title": "Test", "language": "{language}", "recipeIngredientGroups": [{{"recipeIngredients": [
                {{"quantity": {{"value": 0}}, "unitNotation": "{unit}", "ingredientNotation": "{ingredient}"}}
            ]}}]}}"#
        ))
        .expect("fixture parses");
        let draft = draft_from_cookidoo_json(&json).expect("should build draft");

        assert_eq!(draft.ingredients[0].quantity, None, "unit: {unit}");
        assert_eq!(draft.ingredients[0].name, expected_name, "unit: {unit}");
    }
}

#[test]
fn given_cookidoo_nonzero_quantity_when_building_draft_then_quantity_unchanged() {
    // Scalar and single-bound amounts keep their text, and a two-bound range
    // stays name-only, exactly as before the zero handling.
    let json: serde_json::Value = serde_json::from_str(
        r#"{"title": "Test", "recipeIngredientGroups": [{"recipeIngredients": [
            {"quantity": {"value": 200}, "unitNotation": "g", "ingredientNotation": "Mehl"},
            {"quantity": {"value": 1}, "unitNotation": "EL", "ingredientNotation": "Öl"},
            {"quantity": {"from": 80, "to": 100}, "unitNotation": "g", "ingredientNotation": "Ingwer"},
            {"quantity": {"from": 2}, "unitNotation": "", "ingredientNotation": "Zitrone"}
        ]}]}"#,
    )
    .expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    assert_eq!(draft.ingredients[0].quantity.as_deref(), Some("200 g"));
    assert_eq!(draft.ingredients[0].name, "Mehl");
    assert_eq!(draft.ingredients[1].quantity.as_deref(), Some("1 EL"));
    assert_eq!(draft.ingredients[2].quantity, None);
    assert_eq!(draft.ingredients[2].name, "80 - 100 g Ingwer");
    assert_eq!(draft.ingredients[3].quantity.as_deref(), Some("2"));
}

// Multibyte ingredient text: the Cookidoo name/quantity are truncated by byte
// limit, so cutting mid-character used to panic (and abort the release server).
#[test]
fn given_cookidoo_ingredient_with_multibyte_name_past_limit_when_building_draft_then_cuts_on_char_boundary()
 {
    // 60 Greek letters = 120 bytes, so the 100-byte cut lands inside a character.
    let notation = "α".repeat(60);
    let json: serde_json::Value = serde_json::from_str(&format!(
        r#"{{"title": "Test", "recipeIngredientGroups": [{{"recipeIngredients": [
            {{"ingredientNotation": "{notation}", "quantity": {{"value": 1}}, "unitNotation": "g"}}
        ]}}]}}"#
    ))
    .expect("fixture parses");

    let draft = draft_from_cookidoo_json(&json).expect("should build draft");
    let name = &draft.ingredients[0].name;
    assert_eq!(name, &format!("{}…", "α".repeat(48)));
    assert!(name.len() <= 100, "got {} bytes", name.len());
}

#[test]
fn given_cookidoo_ingredient_with_multibyte_quantity_past_limit_when_building_draft_then_cuts_on_char_boundary()
 {
    // "1 " plus 30 Greek letters = 62 bytes, so the 50-byte cut lands inside a character.
    let unit = "β".repeat(30);
    let json: serde_json::Value = serde_json::from_str(&format!(
        r#"{{"title": "Test", "recipeIngredientGroups": [{{"recipeIngredients": [
            {{"ingredientNotation": "Test-Öl", "quantity": {{"value": 1}}, "unitNotation": "{unit}"}}
        ]}}]}}"#
    ))
    .expect("fixture parses");

    let draft = draft_from_cookidoo_json(&json).expect("should build draft");
    let quantity = draft.ingredients[0]
        .quantity
        .clone()
        .expect("quantity kept");
    assert_eq!(quantity, format!("1 {}…", "β".repeat(22)));
    assert!(quantity.len() <= 50, "got {} bytes", quantity.len());
    assert_eq!(draft.ingredients[0].name, "Test-Öl");
}

#[test]
fn given_multibyte_string_past_limit_when_truncating_then_cuts_on_char_boundary() {
    // 100-byte budget minus the 3-byte ellipsis leaves 97 bytes, floored to the
    // 96-byte char boundary: 48 two-byte characters plus the ellipsis (99 bytes).
    let long_alpha = truncate(&"α".repeat(60), 100);
    assert_eq!(long_alpha, format!("{}…", "α".repeat(48)));
    assert!(long_alpha.len() <= 100, "got {} bytes", long_alpha.len());
    // 50-byte budget: 47 bytes floored to 46 — 23 characters plus ellipsis.
    let long_beta = truncate(&"β".repeat(60), 50);
    assert_eq!(long_beta, format!("{}…", "β".repeat(23)));
    assert!(long_beta.len() <= 50, "got {} bytes", long_beta.len());
}

#[test]
fn given_ascii_string_when_truncating_then_behavior_unchanged() {
    assert_eq!(truncate("short", 100), "short");
    // The ellipsis is counted inside the budget: 97 'a's + "…" is exactly 100 bytes.
    let long_ascii = truncate(&"a".repeat(120), 100);
    assert_eq!(long_ascii, format!("{}…", "a".repeat(97)));
    assert_eq!(long_ascii.len(), 100);
}

// ---------------------------------------------------------------------------
// Cookidoo repeats an ingredient across groups: de-DE r51900 (Hefeschnecken)
// lists "Milch" 220 g in "Hefeteig" and 240 g in "Füllung und Fertigstellung",
// de-DE r47908 lists "Salz" in "Teig" and in "Füllung". `db::set_meal_ingredients`
// inserts one `meal_ingredients` row per line under
// `PRIMARY KEY (meal_id, ingredient_id)`, so the second insert raised a
// uniqueness error, the transaction rolled back and saving the import failed
// with HTTP 500. Both amounts must survive under distinct names.
// ---------------------------------------------------------------------------

const COOKIDOO_DUPLICATE_INGREDIENTS_JSON: &str = r#"{
  "title": "Hefeschnecken",
  "servingSize": {"quantity": {"value": 15}, "unitNotation": "Stück"},
  "recipeIngredientGroups": [
    {"title": "Hefeteig", "recipeIngredients": [
      {"quantity": {"value": 220}, "unitNotation": "g", "ingredientNotation": "Milch"},
      {"quantity": {"value": 100}, "unitNotation": "g", "ingredientNotation": "Zucker"}
    ]},
    {"title": "Füllung und Fertigstellung", "recipeIngredients": [
      {"quantity": {"value": 240}, "unitNotation": "g", "ingredientNotation": "Milch"},
      {"quantity": {"value": 20}, "unitNotation": "g", "ingredientNotation": "Zucker"}
    ]}
  ]
}"#;

/// The identity `db::set_meal_ingredients` keys rows by: normalized name,
/// matched case-insensitively (`COLLATE NOCASE`).
fn ingredient_identity(name: &str) -> String {
    crate::db::normalize_ingredient_name(name).to_lowercase()
}

fn assert_ingredient_identities_unique(ingredients: &[crate::model::NewIngredientLine]) {
    let mut identities: Vec<String> = ingredients
        .iter()
        .map(|line| ingredient_identity(&line.name))
        .collect();
    identities.sort();
    identities.dedup();
    assert_eq!(
        identities.len(),
        ingredients.len(),
        "duplicate ingredient identities would break the meal_ingredients primary key"
    );
}

#[test]
fn given_cookidoo_ingredient_repeated_in_two_groups_when_building_draft_then_keeps_both_amounts_under_distinct_names()
 {
    let json: serde_json::Value =
        serde_json::from_str(COOKIDOO_DUPLICATE_INGREDIENTS_JSON).expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    // The later duplicate is named after its group; no line is dropped and each
    // keeps its own quantity.
    let names: Vec<&str> = draft
        .ingredients
        .iter()
        .map(|line| line.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Milch",
            "Zucker",
            "Milch (Füllung und Fertigstellung)",
            "Zucker (Füllung und Fertigstellung)",
        ]
    );
    let quantities: Vec<Option<&str>> = draft
        .ingredients
        .iter()
        .map(|line| line.quantity.as_deref())
        .collect();
    assert_eq!(
        quantities,
        [Some("220 g"), Some("100 g"), Some("240 g"), Some("20 g")]
    );
    assert_ingredient_identities_unique(&draft.ingredients);
}

#[test]
fn given_cookidoo_ingredient_repeated_in_untitled_group_when_building_draft_then_numeric_suffixes()
{
    // An empty/missing group title cannot distinguish the lines, so the numeric
    // suffix takes over — and keeps counting for a third occurrence.
    let json: serde_json::Value = serde_json::from_str(
        r#"{"title": "Test", "recipeIngredientGroups": [
            {"recipeIngredients": [
                {"quantity": {"value": 200}, "unitNotation": "g", "ingredientNotation": "Mehl"},
                {"quantity": {"value": 50}, "unitNotation": "g", "ingredientNotation": "Mehl"},
                {"quantity": {"value": 30}, "unitNotation": "g", "ingredientNotation": "Mehl"}
            ]}
        ]}"#,
    )
    .expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    let names: Vec<&str> = draft
        .ingredients
        .iter()
        .map(|line| line.name.as_str())
        .collect();
    assert_eq!(names, ["Mehl", "Mehl (2)", "Mehl (3)"]);
    let quantities: Vec<Option<&str>> = draft
        .ingredients
        .iter()
        .map(|line| line.quantity.as_deref())
        .collect();
    assert_eq!(quantities, [Some("200 g"), Some("50 g"), Some("30 g")]);
    assert_ingredient_identities_unique(&draft.ingredients);
}

#[test]
fn given_cookidoo_duplicate_group_title_already_used_when_building_draft_then_numeric_suffix() {
    // The second occurrence claims the group title, so the third must fall back
    // to a number instead of colliding with it.
    let json: serde_json::Value = serde_json::from_str(
        r#"{"title": "Test", "recipeIngredientGroups": [
            {"title": "Glasur", "recipeIngredients": [
                {"quantity": {"value": 100}, "unitNotation": "g", "ingredientNotation": "Puderzucker"},
                {"quantity": {"value": 50}, "unitNotation": "g", "ingredientNotation": "Puderzucker"},
                {"quantity": {"value": 20}, "unitNotation": "g", "ingredientNotation": "Puderzucker"}
            ]}
        ]}"#,
    )
    .expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    let names: Vec<&str> = draft
        .ingredients
        .iter()
        .map(|line| line.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Puderzucker", "Puderzucker (Glasur)", "Puderzucker (2)"]
    );
    assert_ingredient_identities_unique(&draft.ingredients);
}

#[test]
fn given_cookidoo_ingredient_repeated_in_different_case_when_building_draft_then_second_renamed() {
    // `upsert_ingredients` looks names up with `COLLATE NOCASE`, so case-only
    // variants share one `ingredients` row and would collide just the same.
    let json: serde_json::Value = serde_json::from_str(
        r#"{"title": "Test", "recipeIngredientGroups": [
            {"title": "Teig", "recipeIngredients": [
                {"quantity": {"value": 150}, "unitNotation": "g", "ingredientNotation": "Mehl"},
                {"quantity": {"value": 50}, "unitNotation": "g", "ingredientNotation": "MEHL"}
            ]}
        ]}"#,
    )
    .expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    let names: Vec<&str> = draft
        .ingredients
        .iter()
        .map(|line| line.name.as_str())
        .collect();
    assert_eq!(names, ["Mehl", "MEHL (Teig)"]);
    assert_ingredient_identities_unique(&draft.ingredients);
}

#[test]
fn given_cookidoo_distinct_ingredients_when_building_draft_then_names_unchanged() {
    let json: serde_json::Value = serde_json::from_str(
        r#"{"title": "Test", "recipeIngredientGroups": [
            {"title": "Teig", "recipeIngredients": [
                {"quantity": {"value": 150}, "unitNotation": "g", "ingredientNotation": "Mehl"}
            ]},
            {"title": "Füllung", "recipeIngredients": [
                {"quantity": {"value": 50}, "unitNotation": "g", "ingredientNotation": "Milch"}
            ]}
        ]}"#,
    )
    .expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    let names: Vec<&str> = draft
        .ingredients
        .iter()
        .map(|line| line.name.as_str())
        .collect();
    assert_eq!(names, ["Mehl", "Milch"]);
}

#[test]
fn given_cookidoo_duplicate_at_name_limit_when_building_draft_then_suffix_within_limit_and_unique()
{
    // Names already at the 100-byte validation limit: the suffix must be
    // reserved inside the budget rather than truncated away with the name.
    let notation = "a".repeat(100);
    let json: serde_json::Value = serde_json::from_str(&format!(
        r#"{{"title": "Test", "recipeIngredientGroups": [
            {{"title": "Gruppe", "recipeIngredients": [
                {{"quantity": {{"value": 1}}, "unitNotation": "g", "ingredientNotation": "{notation}"}},
                {{"quantity": {{"value": 2}}, "unitNotation": "g", "ingredientNotation": "{notation}"}}
            ]}}
        ]}}"#
    ))
    .expect("fixture parses");
    let draft = draft_from_cookidoo_json(&json).expect("should build draft");

    assert_eq!(draft.ingredients.len(), 2);
    for line in &draft.ingredients {
        assert!(line.name.len() <= 100, "got {} bytes", line.name.len());
    }
    assert!(draft.ingredients[0].name.ends_with(&"a".repeat(100)));
    assert!(draft.ingredients[1].name.ends_with("(Gruppe)"));
    assert_ingredient_identities_unique(&draft.ingredients);
}
