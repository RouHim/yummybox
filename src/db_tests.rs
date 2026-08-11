// Unit tests for the database layer. Kept in a separate flat module so
// src/db.rs stays focused on production code.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::db::*;
use crate::error::AppError;
use crate::model::{Meal, MealPatch, NewIngredientLine, NewMeal};

use crate::model::{NewPlanRequest, PlanPatch};

async fn setup_db() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let pool = init_db(&db_path).await.expect("init_db");
    (pool, dir)
}

async fn insert_test_meal(
    pool: &SqlitePool,
    name: &str,
    ingredients: &[(&str, Option<&str>)],
) -> Meal {
    insert_meal(
        pool,
        NewMeal {
            name: name.into(),
            ingredients: ingredients
                .iter()
                .map(|(n, q)| NewIngredientLine {
                    name: n.to_string(),
                    quantity: q.map(String::from),
                })
                .collect(),
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await
    .expect("insert_test_meal")
}

// -----------------------------------------------------------------------
// meals_count
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_empty_db_when_meals_count_then_returns_zero() {
    let (pool, _dir) = setup_db().await;
    assert_eq!(meals_count(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn given_one_meal_inserted_when_meals_count_then_returns_one() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "Test", &[("salt", None)]).await;
    assert_eq!(meals_count(&pool).await.unwrap(), 1);
}

// -----------------------------------------------------------------------
// pool resilience: cancelled begin
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_cancelled_begin_when_pool_reused_then_no_worker_crash() {
    use std::future::poll_fn;
    use std::pin::pin;
    use std::task::Poll;

    let (pool, _dir) = setup_db().await;

    // Poll the begin future once to start the BEGIN command, then drop it.
    // In sqlx 0.8.x this could leave the worker in a corrupted state
    // (Transaction guard not yet constructed, so Drop couldn't roll back).
    // In sqlx 0.9.0 the guard is created before begin(), so Drop safely rolls back.
    // Poll once then drop — cancellation exercises Transaction::Drop rollback.
    {
        let mut begin_fut = pin!(pool.begin());
        let _ = poll_fn(|cx| {
            let _ = begin_fut.as_mut().poll(cx);
            Poll::Ready(())
        })
        .await;
    } // begin_fut dropped here — cancels the pending BEGIN
    // Pool should still be usable — no WorkerCrashed
    let count = meals_count(&pool)
        .await
        .expect("pool should survive cancelled begin");
    assert_eq!(count, 0);
}

// -----------------------------------------------------------------------
// normalize_ingredient_name
// -----------------------------------------------------------------------

#[test]
fn given_name_with_mixed_case_and_whitespace_when_normalize_then_preserves_case_and_collapses_internal_whitespace()
 {
    assert_eq!(normalize_ingredient_name(" Salt "), "Salt");
    assert_eq!(
        normalize_ingredient_name("  Black   Pepper  "),
        "Black Pepper"
    );
}

#[test]
fn given_name_with_only_whitespace_when_normalize_then_returns_empty_string() {
    assert_eq!(normalize_ingredient_name("   "), "");
}

// -----------------------------------------------------------------------
// normalize_meal_name
// -----------------------------------------------------------------------

#[test]
fn given_name_with_mixed_case_and_whitespace_when_normalize_then_lowercases_and_collapses() {
    assert_eq!(normalize_meal_name("Pancakes"), "pancakes");
    assert_eq!(normalize_meal_name("  PANCAKES  "), "pancakes");
    assert_eq!(normalize_meal_name("  Pan   Cakes  "), "pan cakes");
}

#[test]
fn given_name_empty_or_whitespace_when_normalize_then_returns_empty() {
    assert_eq!(normalize_meal_name(""), "");
    assert_eq!(normalize_meal_name("   "), "");
}

#[test]
fn given_unicode_ingredient_name_when_normalize_then_preserves_case_and_collapses_whitespace() {
    assert_eq!(
        normalize_ingredient_name("  Thüringer   Rostbratwurst  "),
        "Thüringer Rostbratwurst"
    );
    assert_eq!(normalize_ingredient_name("grüne Kresse"), "grüne Kresse");
}

// -----------------------------------------------------------------------
// meal_name_exists
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_existing_meal_when_check_duplicate_name_case_insensitive_then_returns_true() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "Risotto", &[("rice", None)]).await;
    assert!(meal_name_exists(&pool, "RISOTTO", None).await.unwrap());
    assert!(meal_name_exists(&pool, "  risotto  ", None).await.unwrap());
}

#[tokio::test]
async fn given_no_meals_when_check_duplicate_name_then_returns_false() {
    let (pool, _dir) = setup_db().await;
    assert!(!meal_name_exists(&pool, "Anything", None).await.unwrap());
}

#[tokio::test]
async fn given_exclude_id_when_check_own_name_then_returns_false() {
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "Pasta", &[("noodles", None)]).await;
    // Excluding the meal itself → not a duplicate
    assert!(
        !meal_name_exists(&pool, "pasta", Some(meal.id))
            .await
            .unwrap()
    );
    // Excluding a different meal → still a duplicate
    assert!(
        meal_name_exists(&pool, "pasta", Some(meal.id + 999))
            .await
            .unwrap()
    );
}

// -----------------------------------------------------------------------
// validate_meal
// -----------------------------------------------------------------------

#[test]
fn given_no_ingredient_lines_when_insert_meal_then_validation_error() {
    let result = validate_meal("x", &[], "valid instructions", None);
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("ingredient")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[test]
fn given_ingredient_line_with_empty_trimmed_name_when_insert_meal_then_validation_error() {
    let result = validate_meal(
        "x",
        &[NewIngredientLine {
            name: "   ".into(),
            quantity: None,
        }],
        "valid instructions",
        None,
    );
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[test]
fn given_ingredient_name_above_100_chars_when_insert_meal_then_validation_error() {
    let long_name = "a".repeat(101);
    let result = validate_meal(
        "x",
        &[NewIngredientLine {
            name: long_name,
            quantity: None,
        }],
        "valid instructions",
        None,
    );
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[test]
fn given_ingredient_quantity_above_50_chars_when_insert_meal_then_validation_error() {
    let long_qty = "a".repeat(51);
    let result = validate_meal(
        "x",
        &[NewIngredientLine {
            name: "valid".into(),
            quantity: Some(long_qty),
        }],
        "valid instructions",
        None,
    );
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("quantity")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[test]
fn given_above_100_ingredient_lines_when_insert_meal_then_validation_error() {
    let lines: Vec<NewIngredientLine> = (0..101)
        .map(|i| NewIngredientLine {
            name: format!("ingredient {i}"),
            quantity: None,
        })
        .collect();
    let result = validate_meal("x", &lines, "valid instructions", None);
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("100")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[test]
fn given_empty_instructions_when_validate_meal_then_error() {
    let result = validate_meal(
        "x",
        &[NewIngredientLine {
            name: "salt".into(),
            quantity: None,
        }],
        "",
        None,
    );
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("instructions")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[test]
fn given_instructions_above_20000_chars_when_validate_meal_then_error() {
    let long_instructions = "a".repeat(20001);
    let result = validate_meal(
        "x",
        &[NewIngredientLine {
            name: "salt".into(),
            quantity: None,
        }],
        &long_instructions,
        None,
    );
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("20000")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn given_name_at_exactly_200_chars_when_insert_meal_then_succeeds() {
    let (pool, _dir) = setup_db().await;
    let name = "a".repeat(200);
    let result = insert_meal(
        &pool,
        NewMeal {
            name: name.clone(),
            ingredients: vec![NewIngredientLine {
                name: "x".into(),
                quantity: None,
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await;
    let meal = result.expect("should succeed");
    assert_eq!(meal.name, name.as_str());
}

#[tokio::test]
async fn given_name_at_201_chars_when_insert_meal_then_returns_validation_error() {
    let (pool, _dir) = setup_db().await;
    let name = "a".repeat(201);
    let result = insert_meal(
        &pool,
        NewMeal {
            name,
            ingredients: vec![NewIngredientLine {
                name: "x".into(),
                quantity: None,
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await;
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn given_empty_name_when_insert_meal_then_returns_validation_error() {
    let (pool, _dir) = setup_db().await;
    let result = insert_meal(
        &pool,
        NewMeal {
            name: "".into(),
            ingredients: vec![NewIngredientLine {
                name: "x".into(),
                quantity: None,
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await;
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn given_whitespace_only_name_when_insert_meal_then_returns_validation_error() {
    let (pool, _dir) = setup_db().await;
    let result = insert_meal(
        &pool,
        NewMeal {
            name: "   ".into(),
            ingredients: vec![NewIngredientLine {
                name: "x".into(),
                quantity: None,
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await;
    match &result {
        Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
        other => panic!("expected Validation, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// upsert_ingredients
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_new_ingredient_names_when_upsert_then_returns_inserted_rows_in_input_order() {
    let (pool, _dir) = setup_db().await;
    let names: Vec<String> = vec!["salt".into(), "pepper".into()];
    let mut conn = pool.acquire().await.unwrap();
    let result = upsert_ingredients(&mut conn, &names).await.expect("upsert");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].1, "salt");
    assert_eq!(result[1].1, "pepper");
}

#[tokio::test]
async fn given_existing_ingredient_names_when_upsert_then_returns_existing_ids_no_duplicates() {
    let (pool, _dir) = setup_db().await;
    let names: Vec<String> = vec!["salt".into()];
    let mut conn = pool.acquire().await.unwrap();
    let first = upsert_ingredients(&mut conn, &names)
        .await
        .expect("upsert 1");
    let second = upsert_ingredients(&mut conn, &names)
        .await
        .expect("upsert 2");
    assert_eq!(first[0].0, second[0].0);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingredients")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn given_empty_input_when_upsert_then_returns_empty_vec() {
    let (pool, _dir) = setup_db().await;
    let mut conn = pool.acquire().await.unwrap();
    let result = upsert_ingredients(&mut conn, &[]).await.expect("upsert");
    assert!(result.is_empty());
}

// -----------------------------------------------------------------------
// set_meal_ingredients / get_meal_ingredients
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_meal_with_existing_ingredients_when_set_meal_ingredients_then_replaces_with_new_set()
{
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "Test", &[("old", None)]).await;

    let mut conn = pool.acquire().await.unwrap();
    set_meal_ingredients(
        &mut conn,
        meal.id,
        &[NewIngredientLine {
            name: "new".into(),
            quantity: None,
        }],
    )
    .await
    .expect("set");

    let ings = get_meal_ingredients(&mut conn, meal.id).await.expect("get");
    assert_eq!(ings.len(), 1);
    assert_eq!(ings[0].name, "new");
}

#[tokio::test]
async fn given_ingredient_line_with_no_quantity_when_set_meal_ingredients_then_stores_null_quantity()
 {
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "Test", &[("x", None)]).await;

    let mut conn = pool.acquire().await.unwrap();
    set_meal_ingredients(
        &mut conn,
        meal.id,
        &[NewIngredientLine {
            name: "salt".into(),
            quantity: None,
        }],
    )
    .await
    .expect("set");

    let ings = get_meal_ingredients(&mut conn, meal.id).await.expect("get");
    assert_eq!(ings[0].quantity, None);
}

#[tokio::test]
async fn given_ingredient_line_with_blank_name_when_set_meal_ingredients_then_skips_that_line() {
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "Test", &[("x", None)]).await;

    let mut conn = pool.acquire().await.unwrap();
    set_meal_ingredients(
        &mut conn,
        meal.id,
        &[
            NewIngredientLine {
                name: "   ".into(),
                quantity: None,
            },
            NewIngredientLine {
                name: "kept".into(),
                quantity: None,
            },
        ],
    )
    .await
    .expect("set");

    let ings = get_meal_ingredients(&mut conn, meal.id).await.expect("get");
    assert_eq!(ings.len(), 1);
    assert_eq!(ings[0].name, "kept");
}

#[tokio::test]
async fn given_meal_with_ingredients_when_get_meal_ingredients_then_returns_ingredient_quantities_sorted_by_name()
 {
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "Test", &[("zucchini", None), ("apple", Some("2"))]).await;
    let mut conn = pool.acquire().await.unwrap();
    let ings = get_meal_ingredients(&mut conn, meal.id).await.expect("get");
    assert_eq!(ings[0].name, "apple");
    assert_eq!(ings[1].name, "zucchini");
    assert_eq!(ings[0].quantity.as_deref(), Some("2"));
    assert_eq!(ings[1].quantity, None);
}

#[tokio::test]
async fn given_meal_with_no_ingredients_when_get_meal_ingredients_then_returns_empty_vec() {
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "Test", &[("x", None)]).await;
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("DELETE FROM meal_ingredients WHERE meal_id = ?1")
        .bind(meal.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    let ings = get_meal_ingredients(&mut conn, meal.id).await.expect("get");
    assert!(ings.is_empty());
}

#[tokio::test]
async fn given_same_ingredient_different_case_when_insert_across_meals_then_deduplicates_preserving_first_casing()
 {
    let (pool, _dir) = setup_db().await;
    // First meal uses "Thüringer Rostbratwurst" (imported casing)
    let meal_a = insert_test_meal(
        &pool,
        "Meal A",
        &[("Thüringer Rostbratwurst", Some("200 g"))],
    )
    .await;
    // Second meal uses lowercase variant — must resolve to the SAME ingredient row
    let meal_b = insert_test_meal(
        &pool,
        "Meal B",
        &[("thüringer rostbratwurst", Some("100 g"))],
    )
    .await;

    let mut conn = pool.acquire().await.unwrap();
    let ings_a = get_meal_ingredients(&mut conn, meal_a.id)
        .await
        .expect("get");
    let ings_b = get_meal_ingredients(&mut conn, meal_b.id)
        .await
        .expect("get");

    // First-seen casing "Thüringer Rostbratwurst" is stored, not lowercased
    assert_eq!(ings_a[0].name, "Thüringer Rostbratwurst");
    // Second meal resolves to the SAME ingredient row (same casing as first-seen)
    assert_eq!(ings_b[0].name, "Thüringer Rostbratwurst");

    // Only one ingredient row exists in the table (case-insensitive dedup)
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ingredients WHERE name = 'Thüringer Rostbratwurst' COLLATE NOCASE",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

// -----------------------------------------------------------------------
// hydrate_meals
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_meals_with_ingredients_when_hydrate_then_attaches_ingredient_lists_to_each() {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "A", &[("salt", Some("1g"))]).await;
    let m2 = insert_test_meal(&pool, "B", &[("salt", Some("2g"))]).await;
    let mut meals = vec![
        Meal {
            ingredients: Vec::new(),
            ..m1.clone()
        },
        Meal {
            ingredients: Vec::new(),
            ..m2.clone()
        },
    ];
    hydrate_meals(&pool, &mut meals).await.expect("hydrate");
    assert_eq!(meals[0].ingredients.len(), 1);
    assert_eq!(meals[0].ingredients[0].name, "salt");
    assert_eq!(meals[1].ingredients.len(), 1);
    assert_eq!(meals[1].ingredients[0].name, "salt");
}

#[tokio::test]
async fn given_meals_with_no_ingredients_when_hydrate_then_attaches_empty_lists() {
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "A", &[("x", None)]).await;
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("DELETE FROM meal_ingredients WHERE meal_id = ?1")
        .bind(meal.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    let mut meals = vec![Meal {
        ingredients: Vec::new(),
        ..meal.clone()
    }];
    hydrate_meals(&pool, &mut meals).await.expect("hydrate");
    assert!(meals[0].ingredients.is_empty());
}

// -----------------------------------------------------------------------
// list_meals
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_empty_db_when_list_meals_then_returns_empty_vec() {
    let (pool, _dir) = setup_db().await;
    let meals = list_meals(&pool, None).await.expect("list_meals");
    assert!(meals.is_empty());
}

#[tokio::test]
async fn given_two_meals_when_list_meals_then_returns_both_ordered_by_updated_at_desc() {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "First", &[("a", None)]).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let m2 = insert_test_meal(&pool, "Second", &[("b", None)]).await;

    let meals = list_meals(&pool, None).await.expect("list_meals");
    assert_eq!(meals.len(), 2);
    assert_eq!(meals[0].id, m2.id);
    assert_eq!(meals[1].id, m1.id);
}

#[tokio::test]
async fn given_search_term_matches_name_then_returns_only_matching() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "Chicken Soup", &[("broth", None)]).await;
    insert_test_meal(&pool, "Beef Stew", &[("meat", None)]).await;
    let meals = list_meals(&pool, Some("chicken"))
        .await
        .expect("list_meals");
    assert_eq!(meals.len(), 1);
    assert_eq!(meals[0].name, "Chicken Soup");
}

#[tokio::test]
async fn given_search_term_matches_ingredients_then_returns_only_matching() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "Chicken Soup", &[("broth", None)]).await;
    insert_test_meal(&pool, "Beef Stew", &[("meat", None)]).await;
    let meals = list_meals(&pool, Some("meat")).await.expect("list_meals");
    assert_eq!(meals.len(), 1);
    assert_eq!(meals[0].name, "Beef Stew");
}

#[tokio::test]
async fn given_search_term_matching_ingredient_name_when_list_meals_then_returns_meals_with_that_ingredient()
 {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "A", &[("tomato", None)]).await;
    insert_test_meal(&pool, "B", &[("onion", None)]).await;
    let meals = list_meals(&pool, Some("tomato")).await.expect("list_meals");
    assert_eq!(meals.len(), 1);
    assert_eq!(meals[0].name, "A");
}

#[tokio::test]
async fn given_search_term_is_whitespace_then_returns_all() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "A", &[("x", None)]).await;
    insert_test_meal(&pool, "B", &[("y", None)]).await;
    let meals = list_meals(&pool, Some("   ")).await.expect("list_meals");
    assert_eq!(meals.len(), 2);
}

#[tokio::test]
async fn given_search_term_matches_neither_then_returns_empty() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "A", &[("x", None)]).await;
    let meals = list_meals(&pool, Some("zzz")).await.expect("list_meals");
    assert!(meals.is_empty());
}

// -----------------------------------------------------------------------
// find_meal
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_meal_exists_when_find_meal_then_returns_meal() {
    let (pool, _dir) = setup_db().await;
    let inserted = insert_test_meal(&pool, "Test", &[("stuff", None)]).await;
    let found = find_meal(&pool, inserted.id).await.expect("find_meal");
    assert_eq!(found.id, inserted.id);
    assert_eq!(found.name, inserted.name);
    assert_eq!(found.ingredients.len(), 1);
    assert_eq!(found.ingredients[0].name, "stuff");
}

#[tokio::test]
async fn given_meal_exists_when_find_meal_with_wrong_id_then_returns_not_found() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "Test", &[("stuff", None)]).await;
    let result = find_meal(&pool, 999).await;
    match &result {
        Err(AppError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// insert_meal
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_valid_meal_when_insert_meal_then_persists_with_ingredients() {
    let (pool, _dir) = setup_db().await;
    let result = insert_meal(
        &pool,
        NewMeal {
            name: "Test".into(),
            ingredients: vec![NewIngredientLine {
                name: "Salt".into(),
                quantity: Some("200g".into()),
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await;
    let meal = result.expect("insert_meal");
    assert!(meal.id > 0);
    assert_eq!(meal.ingredients.len(), 1);
    assert_eq!(meal.ingredients[0].name, "Salt");
    assert_eq!(meal.ingredients[0].quantity.as_deref(), Some("200g"));
}

// -----------------------------------------------------------------------
// update_meal
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_meal_exists_when_update_meal_then_preserves_id_and_advances_updated_at() {
    let (pool, _dir) = setup_db().await;
    let original = insert_test_meal(&pool, "Old Name", &[("Old", None)]).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let updated = update_meal(
        &pool,
        original.id,
        MealPatch {
            name: "New Name".into(),
            ingredients: vec![NewIngredientLine {
                name: "New".into(),
                quantity: None,
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await
    .expect("update_meal");
    assert_eq!(updated.id, original.id);
    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.ingredients.len(), 1);
    assert_eq!(updated.ingredients[0].name, "New");
    assert!(updated.updated_at > original.updated_at);
}

#[tokio::test]
async fn given_meal_when_update_meal_then_last_planned_at_is_unchanged() {
    let (pool, _dir) = setup_db().await;
    let original = insert_test_meal(&pool, "X", &[("y", None)]).await;
    let ts = "2025-01-01T00:00:00Z";
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE id = ?2")
        .bind(ts)
        .bind(original.id)
        .execute(&mut *conn)
        .await
        .unwrap();

    let updated = update_meal(
        &pool,
        original.id,
        MealPatch {
            name: "X2".into(),
            ingredients: vec![NewIngredientLine {
                name: "z".into(),
                quantity: None,
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await
    .expect("update_meal");
    assert_eq!(
        updated.last_planned_at,
        Some(
            DateTime::parse_from_rfc3339(ts)
                .unwrap()
                .with_timezone(&Utc)
        )
    );
}

#[tokio::test]
async fn given_meal_missing_when_update_meal_then_returns_not_found() {
    let (pool, _dir) = setup_db().await;
    let result = update_meal(
        &pool,
        999,
        MealPatch {
            name: "X".into(),
            ingredients: vec![NewIngredientLine {
                name: "y".into(),
                quantity: None,
            }],
            instructions: "test".into(),
            portions: None,
        },
        ImageChange::Keep,
    )
    .await;
    match &result {
        Err(AppError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// -----------------------------------------------------------------------
// delete_meal
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_meal_exists_when_delete_meal_then_subsequent_find_returns_not_found() {
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "To Delete", &[("stuff", None)]).await;
    delete_meal(&pool, meal.id).await.expect("delete_meal");
    let result = find_meal(&pool, meal.id).await;
    match &result {
        Err(AppError::NotFound) => {}
        other => panic!("expected NotFound after delete, got {other:?}"),
    }
}

#[tokio::test]
async fn given_meal_missing_when_delete_meal_then_returns_not_found() {
    let (pool, _dir) = setup_db().await;
    let result = delete_meal(&pool, 999).await;
    match &result {
        Err(AppError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn given_meal_only_uses_ingredient_when_delete_meal_then_ingredient_row_is_deleted_as_orphan()
{
    let (pool, _dir) = setup_db().await;
    let meal = insert_test_meal(&pool, "X", &[("unique_ing", None)]).await;
    delete_meal(&pool, meal.id).await.expect("delete");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ingredients WHERE name = 'unique_ing'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn given_meal_shares_ingredient_with_others_when_delete_meal_then_ingredient_row_remains() {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "A", &[("shared", None)]).await;
    let _m2 = insert_test_meal(&pool, "B", &[("shared", None)]).await;
    delete_meal(&pool, m1.id).await.expect("delete");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingredients WHERE name = 'shared'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// -----------------------------------------------------------------------
// select_meals_weighted
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_3_unplanned_and_3_recent_meals_when_select_3_weighted_over_100_trials_then_unplanned_chosen_at_least_twice_as_often()
 {
    let (pool, _dir) = setup_db().await;

    for i in 1..=3 {
        let m = insert_test_meal(&pool, &format!("new{i}"), &[("x", None)]).await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE meals SET last_planned_at = NULL WHERE id = ?1")
            .bind(m.id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    let recent = Utc::now();
    for i in 1..=3 {
        let m = insert_test_meal(&pool, &format!("recent{i}"), &[("x", None)]).await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE id = ?2")
            .bind(recent)
            .bind(m.id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    let mut unplanned_picks = 0usize;
    let mut recent_picks = 0usize;

    for _ in 0..100 {
        let mut conn = pool.acquire().await.unwrap();
        for i in 1..=3 {
            sqlx::query("UPDATE meals SET last_planned_at = NULL WHERE name = ?1")
                .bind(format!("new{i}"))
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        for i in 1..=3 {
            sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE name = ?2")
                .bind(recent)
                .bind(format!("recent{i}"))
                .execute(&mut *conn)
                .await
                .unwrap();
        }

        let selected = crate::plan::select_meals_weighted(&mut conn, 3)
            .await
            .expect("select");
        for meal in &selected {
            if meal.name.starts_with("new") {
                unplanned_picks += 1;
            } else {
                recent_picks += 1;
            }
        }
    }

    assert!(
        unplanned_picks >= 2 * recent_picks,
        "unplanned_picks={unplanned_picks}, recent_picks={recent_picks}"
    );
}

#[tokio::test]
async fn given_3_meals_when_select_5_weighted_then_returns_all_3_meals() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "A", &[("x", None)]).await;
    insert_test_meal(&pool, "B", &[("x", None)]).await;
    insert_test_meal(&pool, "C", &[("x", None)]).await;

    let mut conn = pool.acquire().await.unwrap();
    let selected = crate::plan::select_meals_weighted(&mut conn, 5)
        .await
        .expect("select");
    assert_eq!(selected.len(), 3);
}

#[tokio::test]
async fn given_empty_meals_table_when_select_weighted_then_returns_empty_vec() {
    let (pool, _dir) = setup_db().await;
    let mut conn = pool.acquire().await.unwrap();
    let result = crate::plan::select_meals_weighted(&mut conn, 3)
        .await
        .expect("select");
    assert!(result.is_empty());
}

// -----------------------------------------------------------------------
// aggregate_plan_ingredients
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_plan_with_two_meals_sharing_salt_200g_and_100g_when_aggregate_then_salt_numeric_total_is_300_with_unit_g()
 {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
    let m2 = insert_test_meal(&pool, "B", &[("salt", Some("100g"))]).await;

    let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    let plan_id: i64 = row.get(0);
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m1.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m2.id)
        .execute(&pool)
        .await
        .unwrap();

    let summary = crate::plan::aggregate_plan_ingredients(&pool, plan_id)
        .await
        .expect("aggregate");
    let salt = summary
        .iter()
        .find(|e| e.name == "salt")
        .expect("salt entry");
    let nt = salt.numeric_total.as_ref().expect("numeric_total");
    assert!((nt.value - 300.0).abs() < 0.001);
    assert_eq!(nt.unit.as_deref(), Some("g"));
}

#[tokio::test]
async fn given_plan_with_salt_200g_and_a_pinch_when_aggregate_then_numeric_total_is_200_with_unit_g_and_non_numeric_has_a_pinch()
 {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
    let m2 = insert_test_meal(&pool, "B", &[("salt", Some("a pinch"))]).await;

    let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    let plan_id: i64 = row.get(0);
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m1.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m2.id)
        .execute(&pool)
        .await
        .unwrap();

    let summary = crate::plan::aggregate_plan_ingredients(&pool, plan_id)
        .await
        .expect("aggregate");
    let salt = summary
        .iter()
        .find(|e| e.name == "salt")
        .expect("salt entry");
    let nt = salt.numeric_total.as_ref().expect("numeric_total");
    assert!((nt.value - 200.0).abs() < 0.001);
    assert_eq!(nt.unit.as_deref(), Some("g"));
    assert_eq!(salt.non_numeric, vec!["a pinch"]);
}

#[tokio::test]
async fn given_plan_with_salt_200g_and_cups_1_5_when_aggregate_then_numeric_total_is_201_5_with_null_unit()
 {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
    let m2 = insert_test_meal(&pool, "B", &[("salt", Some("1.5 cups"))]).await;

    let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    let plan_id: i64 = row.get(0);
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m1.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m2.id)
        .execute(&pool)
        .await
        .unwrap();

    let summary = crate::plan::aggregate_plan_ingredients(&pool, plan_id)
        .await
        .expect("aggregate");
    let salt = summary
        .iter()
        .find(|e| e.name == "salt")
        .expect("salt entry");
    let nt = salt.numeric_total.as_ref().expect("numeric_total");
    assert!((nt.value - 201.5).abs() < 0.001);
    assert_eq!(nt.unit, None);
}

#[tokio::test]
async fn given_plan_with_no_meals_when_aggregate_then_returns_empty_vec() {
    let (pool, _dir) = setup_db().await;
    let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    let plan_id: i64 = row.get(0);
    let summary = crate::plan::aggregate_plan_ingredients(&pool, plan_id)
        .await
        .expect("aggregate");
    assert!(summary.is_empty());
}

// -----------------------------------------------------------------------
// Plan CRUD
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_plan_with_meals_when_get_plan_meals_then_returns_hydrated_meals_in_id_order() {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "A", &[("x", None)]).await;
    let m2 = insert_test_meal(&pool, "B", &[("y", None)]).await;

    let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    let plan_id: i64 = row.get(0);
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m1.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m2.id)
        .execute(&pool)
        .await
        .unwrap();

    let meals = crate::plan::get_plan_meals(&pool, plan_id)
        .await
        .expect("get_plan_meals");
    assert_eq!(meals.len(), 2);
    assert!(!meals[0].ingredients.is_empty());
}

#[tokio::test]
async fn given_empty_meals_table_when_create_or_replace_plan_then_returns_bad_request() {
    let (pool, _dir) = setup_db().await;
    let result = crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 1,
            meal_count: 3,
        },
    )
    .await;
    match &result {
        Err(AppError::BadRequest(_)) => {}
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[tokio::test]
async fn given_existing_plan_when_create_or_replace_plan_then_replaces_meals_and_updates_last_planned_for_new_set_only()
 {
    let (pool, _dir) = setup_db().await;

    insert_test_meal(&pool, "M1", &[("x", None)]).await;
    insert_test_meal(&pool, "M2", &[("x", None)]).await;
    insert_test_meal(&pool, "M3", &[("x", None)]).await;

    let plan1 = crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 1,
            meal_count: 2,
        },
    )
    .await
    .expect("create plan 1");

    let _old_meal_ids: std::collections::HashSet<i64> = plan1.meals.iter().map(|m| m.id).collect();

    let plan2 = crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 1,
            meal_count: 2,
        },
    )
    .await
    .expect("create plan 2");

    assert_eq!(plan2.meals.len(), 2);
    assert!(!plan2.ingredient_summary.is_empty());
}

#[tokio::test]
async fn given_invalid_year_or_week_when_create_or_replace_plan_then_returns_bad_request() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "X", &[("y", None)]).await;
    let result = crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 99,
            meal_count: 1,
        },
    )
    .await;
    match &result {
        Err(AppError::BadRequest(_)) => {}
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[tokio::test]
async fn given_plan_exists_when_get_plan_then_returns_full_plan_with_meals_and_summary() {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
    insert_test_meal(&pool, "B", &[("salt", Some("100g"))]).await;

    let _created = crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 1,
            meal_count: 2,
        },
    )
    .await
    .expect("create");

    let plan = crate::plan::get_plan(&pool, 2026, 1)
        .await
        .expect("get_plan");
    assert_eq!(plan.meals.len(), 2);
    assert!(!plan.ingredient_summary.is_empty());
}

#[tokio::test]
async fn given_plan_missing_when_get_plan_then_returns_not_found() {
    let (pool, _dir) = setup_db().await;
    let result = crate::plan::get_plan(&pool, 2026, 99).await;
    match &result {
        Err(AppError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn given_3_plans_for_year_when_list_plans_for_year_then_returns_3_summary_items_sorted_by_week()
 {
    let (pool, _dir) = setup_db().await;
    insert_test_meal(&pool, "A", &[("x", None)]).await;
    insert_test_meal(&pool, "B", &[("x", None)]).await;

    crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 3,
            meal_count: 1,
        },
    )
    .await
    .expect("create");
    crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 1,
            meal_count: 1,
        },
    )
    .await
    .expect("create");
    crate::plan::create_or_replace_plan(
        &pool,
        NewPlanRequest {
            year: 2026,
            week_number: 2,
            meal_count: 1,
        },
    )
    .await
    .expect("create");

    let list = crate::plan::list_plans_for_year(&pool, 2026)
        .await
        .expect("list");
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].week_number, 1);
    assert_eq!(list[1].week_number, 2);
    assert_eq!(list[2].week_number, 3);
}

#[tokio::test]
async fn given_no_plans_for_year_when_list_plans_for_year_then_returns_empty_vec() {
    let (pool, _dir) = setup_db().await;
    let list = crate::plan::list_plans_for_year(&pool, 2026)
        .await
        .expect("list");
    assert!(list.is_empty());
}

#[tokio::test]
async fn given_existing_plan_when_update_plan_meals_then_returns_plan_with_new_meal_list_and_does_not_touch_any_last_planned_at()
 {
    let (pool, _dir) = setup_db().await;

    let m1 = insert_test_meal(&pool, "M1", &[("x", None)]).await;
    let m2 = insert_test_meal(&pool, "M2", &[("x", None)]).await;
    let m3 = insert_test_meal(&pool, "M3", &[("x", None)]).await;

    let ts = "2025-06-01T00:00:00Z";
    let mut conn = pool.acquire().await.unwrap();
    for id in &[m1.id, m2.id, m3.id] {
        sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE id = ?2")
            .bind(ts)
            .bind(*id)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
    let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m1.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m2.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    let plan = crate::plan::update_plan_meals(
        &pool,
        2026,
        1,
        PlanPatch {
            meal_ids: vec![m1.id, m3.id],
        },
    )
    .await
    .expect("update");

    assert_eq!(plan.meals.len(), 2);
    let meal_ids: Vec<i64> = plan.meals.iter().map(|m| m.id).collect();
    assert!(meal_ids.contains(&m1.id));
    assert!(meal_ids.contains(&m3.id));

    let m1_fresh = find_meal(&pool, m1.id).await.unwrap();
    let m3_fresh = find_meal(&pool, m3.id).await.unwrap();
    let expected = Some(
        DateTime::parse_from_rfc3339(ts)
            .unwrap()
            .with_timezone(&Utc),
    );
    assert_eq!(m1_fresh.last_planned_at, expected);
    assert_eq!(m3_fresh.last_planned_at, expected);
}

#[tokio::test]
async fn given_plan_missing_when_update_plan_meals_then_returns_not_found() {
    let (pool, _dir) = setup_db().await;
    let m = insert_test_meal(&pool, "X", &[("y", None)]).await;
    let result = crate::plan::update_plan_meals(
        &pool,
        2026,
        99,
        PlanPatch {
            meal_ids: vec![m.id],
        },
    )
    .await;
    match &result {
        Err(AppError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn given_meal_id_not_in_meals_table_when_update_plan_meals_then_returns_not_found() {
    let (pool, _dir) = setup_db().await;
    let m = insert_test_meal(&pool, "X", &[("y", None)]).await;

    let mut conn = pool.acquire().await.unwrap();
    sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
    let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    let result = crate::plan::update_plan_meals(
        &pool,
        2026,
        1,
        PlanPatch {
            meal_ids: vec![99999],
        },
    )
    .await;
    match &result {
        Err(AppError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn given_existing_plan_when_delete_plan_then_returns_ok_and_plan_meals_rows_cascade_away() {
    let (pool, _dir) = setup_db().await;
    let m = insert_test_meal(&pool, "A", &[("x", None)]).await;

    let mut conn = pool.acquire().await.unwrap();
    sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
    let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    crate::plan::delete_plan(&pool, 2026, 1)
        .await
        .expect("delete");

    let pm_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plan_meals WHERE plan_id = ?1")
        .bind(plan_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pm_count, 0);

    let _meal = find_meal(&pool, m.id).await.expect("meal should exist");
}

#[tokio::test]
async fn given_meal_in_plan_when_delete_meal_then_plan_meals_row_removed_and_plan_remains() {
    let (pool, _dir) = setup_db().await;
    let m1 = insert_test_meal(&pool, "M1", &[("x", None)]).await;
    let m2 = insert_test_meal(&pool, "M2", &[("x", None)]).await;

    let mut conn = pool.acquire().await.unwrap();
    sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
    let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m1.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
        .bind(plan_id)
        .bind(m2.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    delete_meal(&pool, m1.id).await.expect("delete");

    let wp_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM week_plans WHERE id = ?1")
        .bind(plan_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(wp_count, 1);

    let rows = sqlx::query("SELECT meal_id FROM plan_meals WHERE plan_id = ?1")
        .bind(plan_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    let pm_meals: Vec<i64> = rows.iter().map(|r| r.get(0)).collect();
    assert_eq!(pm_meals, vec![m2.id]);
}
