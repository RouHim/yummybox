use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meal {
    pub id: i64,
    pub name: String,
    pub ingredients: Vec<IngredientQuantity>,
    #[serde(default)]
    pub instructions: String,
    pub last_planned_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub has_image: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct IngredientQuantity {
    pub name: String,
    pub quantity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewMeal {
    pub name: String,
    pub ingredients: Vec<NewIngredientLine>,
    #[serde(default)]
    pub instructions: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewIngredientLine {
    pub name: String,
    pub quantity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MealPatch {
    pub name: String,
    pub ingredients: Vec<NewIngredientLine>,
    #[serde(default)]
    pub instructions: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: i64,
    pub year: i32,
    pub week_number: i32,
    pub created_at: DateTime<Utc>,
    pub meals: Vec<Meal>,
    pub ingredient_summary: Vec<IngredientSummaryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlanSummaryItem {
    pub year: i32,
    pub week_number: i32,
    pub id: i64,
    pub meal_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngredientSummaryEntry {
    pub name: String,
    pub numeric_total: Option<NumericTotal>,
    pub non_numeric: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericTotal {
    pub value: f64,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPlanRequest {
    pub year: i32,
    pub week_number: i32,
    pub meal_count: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanPatch {
    pub meal_ids: Vec<i64>,
}

// ---------------------------------------------------------------------------
// Bulk import types
// ---------------------------------------------------------------------------

/// Request body for `POST /api/import/bulk`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkImportRequest {
    pub urls: Vec<String>,
}

/// A single failed URL entry in a bulk import response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkImportFailure {
    pub url: String,
    pub reason: String,
}

/// Response body for `POST /api/import/bulk`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkImportResult {
    pub created: Vec<Meal>,
    pub failed: Vec<BulkImportFailure>,
}

/// Application version exposed via GET /api/version.
#[derive(Debug, Clone, Serialize)]
pub struct AppVersion {
    pub version: &'static str,
}
mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[test]
    fn given_valid_meal_json_when_deserialize_then_fields_match() {
        let json = r#"{"id":1,"name":"x","ingredients":[{"name":"y","quantity":null}],"instructions":"cook it","last_planned_at":null,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","has_image":false}"#;
        let meal: Meal = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(meal.id, 1);
        assert_eq!(meal.name, "x");
        assert_eq!(meal.ingredients.len(), 1);
        assert_eq!(meal.ingredients[0].name, "y");
        assert_eq!(meal.ingredients[0].quantity, None);
        assert_eq!(meal.instructions, "cook it");
        assert_eq!(meal.last_planned_at, None);
        let expected_dt =
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").expect("valid datetime");
        assert_eq!(meal.created_at, expected_dt);
        assert_eq!(meal.updated_at, expected_dt);
    }

    #[test]
    fn given_meal_when_serialize_then_round_trips() {
        let meal = Meal {
            id: 42,
            name: "Pasta".into(),
            ingredients: vec![IngredientQuantity {
                name: "noodles".into(),
                quantity: None,
            }],
            instructions: String::new(),
            last_planned_at: None,
            created_at: DateTime::parse_from_rfc3339("2026-06-13T12:00:00Z")
                .unwrap()
                .into(),
            updated_at: DateTime::parse_from_rfc3339("2026-06-13T12:00:00Z")
                .unwrap()
                .into(),
            has_image: false,
        };
        let json = serde_json::to_string(&meal).expect("should serialize");
        let roundtripped: Meal = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(meal, roundtripped);
    }

    #[test]
    fn given_payload_json_when_deserialize_new_meal_then_fields_match() {
        let json =
            r#"{"name":"a","ingredients":[{"name":"b","quantity":null}],"instructions":"cook"}"#;
        let new_meal: NewMeal = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(new_meal.name, "a");
        assert_eq!(new_meal.ingredients.len(), 1);
        assert_eq!(new_meal.ingredients[0].name, "b");
        assert_eq!(new_meal.ingredients[0].quantity, None);
        assert_eq!(new_meal.instructions, "cook");
    }

    #[test]
    fn given_meal_json_without_has_image_field_when_deserialize_then_defaults_to_false() {
        // Older clients or persisted snapshots may omit has_image.
        let json = r#"{"id":9,"name":"old","ingredients":[],"last_planned_at":null,"created_at":"2025-01-01T00:00:00Z","updated_at":"2025-01-01T00:00:00Z"}"#;
        let meal: Meal = serde_json::from_str(json).expect("should deserialize");
        assert!(!meal.has_image);
    }
}
