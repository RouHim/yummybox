use std::path::Path;

use chrono::Utc;
use rusqlite::{Connection, params};

use crate::error::AppError;
use crate::model::{Meal, MealPatch, NewMeal};

pub fn init_db(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meals (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT NOT NULL,
            ingredients TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )?;
    Ok(conn)
}

pub fn list_meals(conn: &Connection, search: Option<&str>) -> Result<Vec<Meal>, AppError> {
    let search_term = search.map(str::trim).filter(|s| !s.is_empty());
    let meals = match search_term {
        Some(term) => {
            let pattern = format!("%{}%", term.to_lowercase());
            let mut stmt = conn.prepare(
                "SELECT id, name, ingredients, created_at, updated_at
                 FROM meals
                 WHERE LOWER(name) LIKE ?1 OR LOWER(ingredients) LIKE ?2
                 ORDER BY updated_at DESC, id DESC",
            )?;
            let rows = stmt.query_map(params![pattern, pattern], |row| {
                Ok(Meal {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    ingredients: row.get(2)?,
                    created_at: rfc3339_to_dt(&row.get::<_, String>(3)?)?,
                    updated_at: rfc3339_to_dt(&row.get::<_, String>(4)?)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT id, name, ingredients, created_at, updated_at
                 FROM meals
                 ORDER BY updated_at DESC, id DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(Meal {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    ingredients: row.get(2)?,
                    created_at: rfc3339_to_dt(&row.get::<_, String>(3)?)?,
                    updated_at: rfc3339_to_dt(&row.get::<_, String>(4)?)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        }
    };
    Ok(meals)
}

pub fn find_meal(conn: &Connection, id: i64) -> Result<Meal, AppError> {
    conn.query_row(
        "SELECT id, name, ingredients, created_at, updated_at FROM meals WHERE id = ?1",
        params![id],
        |row| {
            Ok(Meal {
                id: row.get(0)?,
                name: row.get(1)?,
                ingredients: row.get(2)?,
                created_at: rfc3339_to_dt(&row.get::<_, String>(3)?)?,
                updated_at: rfc3339_to_dt(&row.get::<_, String>(4)?)?,
            })
        },
    )
    .map_err(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound,
        other => AppError::Database(other),
    })
}

pub fn insert_meal(conn: &Connection, new: NewMeal) -> Result<Meal, AppError> {
    validate_meal(&new.name, &new.ingredients)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO meals (name, ingredients, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![new.name.trim(), new.ingredients.trim(), now, now],
    )?;
    let id = conn.last_insert_rowid();
    find_meal(conn, id)
}

pub fn update_meal(conn: &Connection, id: i64, patch: MealPatch) -> Result<Meal, AppError> {
    validate_meal(&patch.name, &patch.ingredients)?;
    let now = Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE meals SET name = ?1, ingredients = ?2, updated_at = ?3 WHERE id = ?4",
        params![patch.name.trim(), patch.ingredients.trim(), now, id],
    )?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    find_meal(conn, id)
}

pub fn delete_meal(conn: &Connection, id: i64) -> Result<(), AppError> {
    let affected = conn.execute("DELETE FROM meals WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub fn validate_meal(name: &str, ingredients: &str) -> Result<(), AppError> {
    let name_trimmed = name.trim();
    if name_trimmed.is_empty() {
        return Err(AppError::Validation("name must not be empty".into()));
    }
    if name_trimmed.chars().count() > 200 {
        return Err(AppError::Validation(
            "name must be at most 200 characters".into(),
        ));
    }
    let ing_trimmed = ingredients.trim();
    if ing_trimmed.is_empty() {
        return Err(AppError::Validation("ingredients must not be empty".into()));
    }
    if ing_trimmed.chars().count() > 5000 {
        return Err(AppError::Validation(
            "ingredients must be at most 5000 characters".into(),
        ));
    }
    Ok(())
}

fn rfc3339_to_dt(s: &str) -> Result<chrono::DateTime<Utc>, rusqlite::Error> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(Into::into)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> (Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let conn = init_db(&db_path).expect("init_db");
        (conn, dir)
    }

    fn insert_test_meal(conn: &Connection, name: &str, ingredients: &str) -> Meal {
        insert_meal(
            conn,
            NewMeal {
                name: name.into(),
                ingredients: ingredients.into(),
            },
        )
        .expect("insert_test_meal")
    }

    #[test]
    fn given_empty_db_when_list_meals_then_returns_empty_vec() {
        let (conn, _dir) = setup_db();
        let meals = list_meals(&conn, None).expect("list_meals");
        assert!(meals.is_empty());
    }

    #[test]
    fn given_two_meals_when_list_meals_then_returns_both_ordered_by_updated_at_desc() {
        let (conn, _dir) = setup_db();
        let m1 = insert_test_meal(&conn, "First", "a");
        // Sleep to ensure distinct updated_at timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));
        let m2 = insert_test_meal(&conn, "Second", "b");

        let meals = list_meals(&conn, None).expect("list_meals");
        assert_eq!(meals.len(), 2);
        // Most recently updated first
        assert_eq!(meals[0].id, m2.id);
        assert_eq!(meals[1].id, m1.id);
    }

    #[test]
    fn given_meal_exists_when_find_meal_then_returns_meal() {
        let (conn, _dir) = setup_db();
        let inserted = insert_test_meal(&conn, "Test", "stuff");
        let found = find_meal(&conn, inserted.id).expect("find_meal");
        assert_eq!(found.id, inserted.id);
        assert_eq!(found.name, inserted.name);
        assert_eq!(found.ingredients, inserted.ingredients);
    }

    #[test]
    fn given_meal_exists_when_find_meal_with_wrong_id_then_returns_not_found() {
        let (conn, _dir) = setup_db();
        insert_test_meal(&conn, "Test", "stuff");
        let result = find_meal(&conn, 999);
        match result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn given_search_term_matches_name_then_returns_only_matching() {
        let (conn, _dir) = setup_db();
        insert_test_meal(&conn, "Chicken Soup", "broth");
        insert_test_meal(&conn, "Beef Stew", "meat");
        let meals = list_meals(&conn, Some("chicken")).expect("list_meals");
        assert_eq!(meals.len(), 1);
        assert_eq!(meals[0].name, "Chicken Soup");
    }

    #[test]
    fn given_search_term_matches_ingredients_then_returns_only_matching() {
        let (conn, _dir) = setup_db();
        insert_test_meal(&conn, "Chicken Soup", "broth");
        insert_test_meal(&conn, "Beef Stew", "meat");
        let meals = list_meals(&conn, Some("meat")).expect("list_meals");
        assert_eq!(meals.len(), 1);
        assert_eq!(meals[0].name, "Beef Stew");
    }

    #[test]
    fn given_search_term_is_whitespace_then_returns_all() {
        let (conn, _dir) = setup_db();
        insert_test_meal(&conn, "A", "x");
        insert_test_meal(&conn, "B", "y");
        let meals = list_meals(&conn, Some("   ")).expect("list_meals");
        assert_eq!(meals.len(), 2);
    }

    #[test]
    fn given_search_term_matches_neither_then_returns_empty() {
        let (conn, _dir) = setup_db();
        insert_test_meal(&conn, "A", "x");
        let meals = list_meals(&conn, Some("zzz")).expect("list_meals");
        assert!(meals.is_empty());
    }

    #[test]
    fn given_meal_exists_when_update_meal_then_preserves_id_and_advances_updated_at() {
        let (conn, _dir) = setup_db();
        let original = insert_test_meal(&conn, "Old Name", "Old Ingredients");
        std::thread::sleep(std::time::Duration::from_millis(10));
        let updated = update_meal(
            &conn,
            original.id,
            MealPatch {
                name: "New Name".into(),
                ingredients: "New Ingredients".into(),
            },
        )
        .expect("update_meal");
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.ingredients, "New Ingredients");
        assert!(updated.updated_at > original.updated_at);
    }

    #[test]
    fn given_meal_missing_when_update_meal_then_returns_not_found() {
        let (conn, _dir) = setup_db();
        let result = update_meal(
            &conn,
            999,
            MealPatch {
                name: "X".into(),
                ingredients: "Y".into(),
            },
        );
        match result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn given_meal_exists_when_delete_meal_then_subsequent_find_returns_not_found() {
        let (conn, _dir) = setup_db();
        let meal = insert_test_meal(&conn, "To Delete", "stuff");
        delete_meal(&conn, meal.id).expect("delete_meal");
        let result = find_meal(&conn, meal.id);
        match result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound after delete, got {other:?}"),
        }
    }

    #[test]
    fn given_meal_missing_when_delete_meal_then_returns_not_found() {
        let (conn, _dir) = setup_db();
        let result = delete_meal(&conn, 999);
        match result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn given_empty_name_when_insert_meal_then_returns_validation_error() {
        let (conn, _dir) = setup_db();
        let result = insert_meal(
            &conn,
            NewMeal {
                name: "".into(),
                ingredients: "x".into(),
            },
        );
        match result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_whitespace_only_name_when_insert_meal_then_returns_validation_error() {
        let (conn, _dir) = setup_db();
        let result = insert_meal(
            &conn,
            NewMeal {
                name: "   ".into(),
                ingredients: "x".into(),
            },
        );
        match result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_name_at_exactly_200_chars_when_insert_meal_then_succeeds() {
        let (conn, _dir) = setup_db();
        let name = "a".repeat(200);
        let result = insert_meal(
            &conn,
            NewMeal {
                name: name.clone(),
                ingredients: "x".into(),
            },
        );
        let meal = result.expect("should succeed");
        assert_eq!(meal.name, name.as_str());
    }

    #[test]
    fn given_name_at_201_chars_when_insert_meal_then_returns_validation_error() {
        let (conn, _dir) = setup_db();
        let name = "a".repeat(201);
        let result = insert_meal(
            &conn,
            NewMeal {
                name,
                ingredients: "x".into(),
            },
        );
        match result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_ingredients_at_exactly_5000_chars_when_insert_meal_then_succeeds() {
        let (conn, _dir) = setup_db();
        let ingredients = "a".repeat(5000);
        let result = insert_meal(
            &conn,
            NewMeal {
                name: "x".into(),
                ingredients: ingredients.clone(),
            },
        );
        let meal = result.expect("should succeed");
        assert_eq!(meal.ingredients, ingredients.as_str());
    }

    #[test]
    fn given_ingredients_at_5001_chars_when_insert_meal_then_returns_validation_error() {
        let (conn, _dir) = setup_db();
        let ingredients = "a".repeat(5001);
        let result = insert_meal(
            &conn,
            NewMeal {
                name: "x".into(),
                ingredients,
            },
        );
        match result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("ingredients")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_empty_ingredients_when_insert_meal_then_returns_validation_error() {
        let (conn, _dir) = setup_db();
        let result = insert_meal(
            &conn,
            NewMeal {
                name: "x".into(),
                ingredients: "".into(),
            },
        );
        match result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("ingredients")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_whitespace_only_ingredients_when_insert_meal_then_returns_validation_error() {
        let (conn, _dir) = setup_db();
        let result = insert_meal(
            &conn,
            NewMeal {
                name: "x".into(),
                ingredients: "   ".into(),
            },
        );
        match result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("ingredients")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }
}
