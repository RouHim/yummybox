use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meal {
    pub id: i64,
    pub name: String,
    pub ingredients: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewMeal {
    pub name: String,
    pub ingredients: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MealPatch {
    pub name: String,
    pub ingredients: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_valid_json_when_deserialize_meal_then_fields_match() {
        let json = r#"{"id":1,"name":"x","ingredients":"y","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#;
        let meal: Meal = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(meal.id, 1);
        assert_eq!(meal.name, "x");
        assert_eq!(meal.ingredients, "y");
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
            ingredients: "noodles, sauce".into(),
            created_at: DateTime::parse_from_rfc3339("2026-06-13T12:00:00Z")
                .unwrap()
                .into(),
            updated_at: DateTime::parse_from_rfc3339("2026-06-13T12:00:00Z")
                .unwrap()
                .into(),
        };
        let json = serde_json::to_string(&meal).expect("should serialize");
        let roundtripped: Meal = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(meal, roundtripped);
    }

    #[test]
    fn given_payload_when_deserialize_new_meal_then_fields_match() {
        let json = r#"{"name":"a","ingredients":"b"}"#;
        let new_meal: NewMeal = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(new_meal.name, "a");
        assert_eq!(new_meal.ingredients, "b");
    }
}
