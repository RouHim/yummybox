use crate::model::{IngredientQuantity, Meal};

/// Schema.org JSON-LD context URI.
pub const SCHEMA_CONTEXT: &str = "https://schema.org";

/// How a Recipe names its image.
///
/// The schema.org mapping is identical for the API and the ZIP export; only
/// the image reference differs, so it is the one value the caller supplies.
#[derive(Debug, Clone, Copy)]
pub enum ImageRef<'a> {
    /// Absolute URL served by this instance: `<base>/api/meals/<id>/image`.
    Absolute(&'a str),
    /// Relative path inside an export archive: `<prefix>/<id>.jpg`.
    Relative(&'a str),
    /// Leave the image out, even when the meal has one.
    Omit,
}

impl<'a> ImageRef<'a> {
    /// Absolute image URLs derived from the request's base URL, or `Omit`
    /// when the request carries no host header.
    pub fn from_base(base: Option<&'a str>) -> Self {
        match base {
            Some(base) => Self::Absolute(base),
            None => Self::Omit,
        }
    }

    /// The `image` value for this meal, or `None` when it has no image or the
    /// caller asked for it to be omitted.
    fn url_for(self, meal: &Meal) -> Option<String> {
        if !meal.has_image {
            return None;
        }
        match self {
            Self::Absolute(base) => Some(format!("{base}/api/meals/{}/image", meal.id)),
            Self::Relative(prefix) => Some(format!("{prefix}/{}.jpg", meal.id)),
            Self::Omit => None,
        }
    }
}

/// Format an ingredient line: `"{quantity} {name}"` when quantity is Some and
/// non-empty after trimming, otherwise just `"{name}"`.
fn ingredient_line(ing: &IngredientQuantity) -> String {
    match &ing.quantity {
        Some(q) if !q.trim().is_empty() => format!("{} {}", q.trim(), ing.name),
        _ => ing.name.clone(),
    }
}

/// Convert a single [`Meal`] into a schema.org `Recipe` JSON-LD object.
///
/// The image is emitted only when the meal has one and `image` names it.
pub fn meal_to_recipe(meal: &Meal, image: ImageRef<'_>) -> serde_json::Value {
    use serde_json::{Map, Value};
    let mut obj = Map::new();

    obj.insert("@context".into(), Value::String(SCHEMA_CONTEXT.into()));
    obj.insert("@type".into(), Value::String("Recipe".into()));
    obj.insert("name".into(), Value::String(meal.name.clone()));

    let ingredients: Vec<Value> = meal
        .ingredients
        .iter()
        .map(|i| Value::String(ingredient_line(i)))
        .collect();
    obj.insert("recipeIngredient".into(), Value::Array(ingredients));

    // Always emit recipeInstructions, even empty string (per Edge Case)
    obj.insert(
        "recipeInstructions".into(),
        Value::String(meal.instructions.clone()),
    );

    obj.insert(
        "datePublished".into(),
        Value::String(meal.created_at.to_rfc3339()),
    );
    obj.insert(
        "dateModified".into(),
        Value::String(meal.updated_at.to_rfc3339()),
    );

    if let Some(p) = meal.portions {
        obj.insert("recipeYield".into(), Value::String(p.to_string()));
    }

    if let Some(url) = &meal.source_url {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            obj.insert("url".into(), Value::String(trimmed.to_string()));
        }
    }

    if let Some(image_url) = image.url_for(meal) {
        obj.insert("image".into(), Value::String(image_url));
    }

    Value::Object(obj)
}

/// Convert a slice of meals into a JSON-LD `@graph` array response.
///
/// Each node in the graph is a self-contained `Recipe` with its own
/// `@context` and `@type` so that a single node extracted from the array
/// carries all required metadata.
pub fn meals_to_graph(meals: &[Meal], image: ImageRef<'_>) -> serde_json::Value {
    serde_json::json!({
        "@context": SCHEMA_CONTEXT,
        "@graph": meals
            .iter()
            .map(|m| meal_to_recipe(m, image))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_meal(
        id: i64,
        name: &str,
        ingredients: Vec<IngredientQuantity>,
        instructions: &str,
        has_image: bool,
    ) -> Meal {
        let ts = Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0).unwrap();
        Meal {
            id,
            name: name.into(),
            ingredients,
            instructions: instructions.into(),
            last_planned_at: None,
            created_at: ts,
            updated_at: ts,
            has_image,
            portions: None,
            source_url: None,
        }
    }

    fn iq(name: &str, quantity: Option<&str>) -> IngredientQuantity {
        IngredientQuantity {
            name: name.into(),
            quantity: quantity.map(String::from),
        }
    }

    // --- ingredient_line ---------------------------------------------------

    #[test]
    fn given_ingredient_with_quantity_when_ingredient_line_then_quantity_space_name() {
        let ing = iq("flour", Some("2 cups"));
        assert_eq!(ingredient_line(&ing), "2 cups flour");
    }

    #[test]
    fn given_ingredient_with_padded_quantity_when_ingredient_line_then_quantity_is_trimmed() {
        let ing = iq("flour", Some("  2 cups "));
        assert_eq!(ingredient_line(&ing), "2 cups flour");
    }

    #[test]
    fn given_ingredient_with_empty_quantity_when_ingredient_line_then_name_only() {
        let ing = iq("salt", Some(""));
        assert_eq!(ingredient_line(&ing), "salt");
    }

    #[test]
    fn given_ingredient_with_whitespace_quantity_when_ingredient_line_then_name_only() {
        let ing = iq("sugar", Some("   "));
        assert_eq!(ingredient_line(&ing), "sugar");
    }

    #[test]
    fn given_ingredient_without_quantity_when_ingredient_line_then_name_only() {
        let ing = iq("eggs", None);
        assert_eq!(ingredient_line(&ing), "eggs");
    }

    // --- meal_to_recipe ----------------------------------------------------

    #[test]
    fn given_meal_when_to_recipe_then_has_context_and_type() {
        let meal = make_meal(1, "Test", vec![iq("x", None)], "", false);
        let recipe = meal_to_recipe(&meal, ImageRef::Omit);
        let obj = recipe.as_object().unwrap();
        assert_eq!(obj["@context"], "https://schema.org");
        assert_eq!(obj["@type"], "Recipe");
    }

    #[test]
    fn given_meal_with_image_when_to_recipe_then_image_is_absolute_url() {
        let meal = make_meal(42, "Photo Meal", vec![], "", true);
        let recipe = meal_to_recipe(&meal, ImageRef::Absolute("http://127.0.0.1:11341"));
        assert_eq!(recipe["image"], "http://127.0.0.1:11341/api/meals/42/image");
    }

    #[test]
    fn given_meal_with_image_when_to_recipe_with_relative_ref_then_image_is_archive_path() {
        let meal = make_meal(42, "Photo Meal", vec![], "", true);
        let recipe = meal_to_recipe(&meal, ImageRef::Relative("images"));
        assert_eq!(recipe["image"], "images/42.jpg");
    }

    #[test]
    fn given_meal_without_image_when_to_recipe_then_no_image_field() {
        let meal = make_meal(1, "NoImg", vec![], "", false);
        let recipe = meal_to_recipe(&meal, ImageRef::Absolute("http://127.0.0.1:11341"));
        assert!(
            !recipe.as_object().unwrap().contains_key("image"),
            "image field should be absent when has_image is false"
        );
    }

    #[test]
    fn given_none_base_url_when_to_recipe_then_no_image_field() {
        let meal = make_meal(1, "ImgNoBase", vec![], "", true);
        let recipe = meal_to_recipe(&meal, ImageRef::from_base(None));
        assert!(
            !recipe.as_object().unwrap().contains_key("image"),
            "image field should be absent when base_url is None"
        );
    }

    #[test]
    fn given_meal_when_to_recipe_then_ingredients_formatted() {
        let meal = make_meal(
            1,
            "Pancakes",
            vec![
                iq("flour", Some("2 cups")),
                iq("egg", Some("1")),
                iq("salt", None),
            ],
            "",
            false,
        );
        let recipe = meal_to_recipe(&meal, ImageRef::Omit);
        let ings = recipe["recipeIngredient"].as_array().unwrap();
        assert_eq!(ings.len(), 3);
        assert_eq!(ings[0], "2 cups flour");
        assert_eq!(ings[1], "1 egg");
        assert_eq!(ings[2], "salt");
    }

    #[test]
    fn given_empty_instructions_when_to_recipe_then_recipe_instructions_empty_string() {
        let meal = make_meal(1, "Minimal", vec![], "", false);
        let recipe = meal_to_recipe(&meal, ImageRef::Omit);
        assert_eq!(recipe["recipeInstructions"], "");
    }

    #[test]
    fn given_meal_when_to_recipe_then_dates_iso8601() {
        let meal = make_meal(1, "Dated", vec![], "", false);
        let recipe = meal_to_recipe(&meal, ImageRef::Omit);
        assert_eq!(recipe["datePublished"], "2026-06-29T12:00:00+00:00");
        assert_eq!(recipe["dateModified"], "2026-06-29T12:00:00+00:00");
    }

    // --- meals_to_graph ----------------------------------------------------

    #[test]
    fn given_meals_when_to_graph_then_context_and_graph_array() {
        let meals = vec![
            make_meal(1, "A", vec![], "", false),
            make_meal(2, "B", vec![], "", false),
        ];
        let graph = meals_to_graph(&meals, ImageRef::Omit);
        let obj = graph.as_object().unwrap();
        assert_eq!(obj["@context"], "https://schema.org");
        let nodes = obj["@graph"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        for node in nodes {
            assert_eq!(node["@type"], "Recipe");
            assert!(node.as_object().unwrap().contains_key("@context"));
        }
    }

    #[test]
    fn given_no_meals_when_to_graph_then_empty_graph() {
        let graph = meals_to_graph(&[], ImageRef::Omit);
        let obj = graph.as_object().unwrap();
        assert_eq!(obj["@context"], "https://schema.org");
        let nodes = obj["@graph"].as_array().unwrap();
        assert!(nodes.is_empty());
    }
}
