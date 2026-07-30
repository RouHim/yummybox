use rand::RngExt;
use rand::SeedableRng;
use rand::seq::SliceRandom;

use crate::db;
use crate::error::AppError;
use crate::model::{NewIngredientLine, NewMeal};

/// Hardcoded seed for deterministic meal generation.
pub const SEED_RNG_SEED: u64 = 0xC0FFEE_BEEF5EA5;

/// Pool of plausible meal names (at least 30, per spec).
const MEAL_NAME_POOL: &[&str] = &[
    "Spaghetti Bolognese",
    "Chicken Tikka Masala",
    "Margherita Pizza",
    "Beef Stroganoff",
    "Pad Thai",
    "Fish and Chips",
    "Chicken Caesar Salad",
    "Beef Tacos",
    "Vegetable Stir Fry",
    "Mushroom Risotto",
    "Lamb Curry",
    "Grilled Salmon",
    "Chicken Parmesan",
    "Beef Burger",
    "Tomato Soup",
    "French Onion Soup",
    "Chicken Noodle Soup",
    "Beef Stew",
    "Vegetable Lasagna",
    "Chicken Fajitas",
    "Shrimp Scampi",
    "Pork Chops",
    "Eggplant Parmesan",
    "Chicken Quesadilla",
    "Beef Burrito",
    "Vegetable Curry",
    "Tuna Casserole",
    "Chicken Pot Pie",
    "Beef Meatballs",
    "Caprese Salad",
    "Minestrone Soup",
    "Chicken Wings",
    "Garlic Bread",
    "Caesar Wrap",
    "Greek Salad",
];

/// Pool of (ingredient_name, quantity) pairs — all quantities are non-empty.
const INGREDIENT_POOL: &[(&str, &str)] = &[
    ("Salt", "1 tsp"),
    ("Olive Oil", "2 tbsp"),
    ("Garlic", "2 cloves"),
    ("Onion", "1 medium"),
    ("Black Pepper", "1/2 tsp"),
    ("Tomato Sauce", "1 cup"),
    ("Chicken Breast", "500 g"),
    ("Pasta", "200 g"),
    ("Ground Beef", "500 g"),
    ("Butter", "2 tbsp"),
    ("Flour", "1 cup"),
    ("Milk", "1 cup"),
    ("Eggs", "2"),
    ("Rice", "2 cups"),
    ("Soy Sauce", "2 tbsp"),
    ("Ginger", "1 tbsp"),
    ("Lemon Juice", "1 tbsp"),
    ("Parmesan Cheese", "1/2 cup"),
    ("Mozzarella", "200 g"),
    ("Basil", "1/4 cup"),
    ("Oregano", "1 tsp"),
    ("Cumin", "1 tsp"),
    ("Paprika", "1 tbsp"),
    ("Chili Powder", "1 tsp"),
    ("Thyme", "1 tsp"),
    ("Rosemary", "1 tsp"),
    ("Bay Leaf", "2 leaves"),
    ("Carrots", "2 medium"),
    ("Celery", "2 stalks"),
    ("Bell Pepper", "1"),
    ("Potatoes", "3 medium"),
    ("Tomatoes", "2"),
    ("Mushrooms", "200 g"),
    ("Spinach", "2 cups"),
    ("Corn", "1 can"),
    ("Cheddar Cheese", "1 cup"),
    ("Sour Cream", "1/2 cup"),
    ("Cilantro", "1/4 cup"),
    ("Lime", "1"),
    ("Green Beans", "200 g"),
];

/// Outcome of a seed run.
pub enum SeedOutcome {
    Inserted(u64),
    Skipped,
}

/// Seed the database with 15 deterministic meals.
///
/// If the database already contains meals, the run is skipped.
/// Otherwise 15 meals are inserted sequentially via [`db::insert_meal`].
pub async fn run(pool: &sqlx::SqlitePool) -> Result<SeedOutcome, AppError> {
    if db::meals_count(pool).await? > 0 {
        return Ok(SeedOutcome::Skipped);
    }

    let mut rng = rand::rngs::StdRng::seed_from_u64(SEED_RNG_SEED);

    // Deterministic 15-name sample.
    let mut names: Vec<&str> = MEAL_NAME_POOL.to_vec();
    names.shuffle(&mut rng);
    names.truncate(15);

    for name in &names {
        let ing_count: usize = rng.random_range(2..=6);

        // Shuffle ingredients per meal to randomize selection ordering.
        let mut ings: Vec<NewIngredientLine> = INGREDIENT_POOL
            .iter()
            .map(|(n, q)| NewIngredientLine {
                name: n.to_string(),
                quantity: Some(q.to_string()),
            })
            .collect();
        ings.shuffle(&mut rng);
        ings.truncate(ing_count);

        let new_meal = NewMeal {
            name: name.to_string(),
            ingredients: ings,
            instructions: format!("Cook the {} according to your preferred method.", name),
            portions: None,
        };
        db::insert_meal(pool, new_meal, db::ImageChange::Keep).await?;
    }

    Ok(SeedOutcome::Inserted(15))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::db;
    use crate::model::Meal;

    async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = db::init_db(&db_path).await.expect("init_db");
        (pool, dir)
    }

    async fn insert_one_meal(pool: &sqlx::SqlitePool, name: &str) -> Meal {
        db::insert_meal(
            pool,
            NewMeal {
                name: name.to_string(),
                ingredients: vec![NewIngredientLine {
                    name: "salt".into(),
                    quantity: Some("1 tsp".into()),
                }],
                instructions: "test".into(),
                portions: None,
            },
            db::ImageChange::Keep,
        )
        .await
        .expect("insert_one_meal")
    }

    #[tokio::test]
    async fn given_empty_db_when_run_then_inserts_exactly_15_meals() {
        let (pool, _dir) = setup_db().await;
        let outcome = run(&pool).await.unwrap();
        match outcome {
            SeedOutcome::Inserted(n) => assert_eq!(n, 15),
            SeedOutcome::Skipped => panic!("expected Inserted(15), got Skipped"),
        }
        let meals = db::list_meals(&pool, None).await.unwrap();
        assert_eq!(meals.len(), 15);
    }

    #[tokio::test]
    async fn given_populated_db_when_run_then_returns_skipped_and_does_not_insert() {
        let (pool, _dir) = setup_db().await;
        insert_one_meal(&pool, "Existing").await;
        let outcome = run(&pool).await.unwrap();
        match outcome {
            SeedOutcome::Skipped => {}
            SeedOutcome::Inserted(_) => panic!("expected Skipped"),
        }
        let meals = db::list_meals(&pool, None).await.unwrap();
        assert_eq!(meals.len(), 1);
    }

    #[tokio::test]
    async fn given_run_called_twice_when_second_call_then_skips() {
        let (pool, _dir) = setup_db().await;
        match run(&pool).await.unwrap() {
            SeedOutcome::Inserted(15) => {}
            _ => panic!("first call should insert"),
        }
        match run(&pool).await.unwrap() {
            SeedOutcome::Skipped => {}
            _ => panic!("second call should skip"),
        }
        assert_eq!(db::meals_count(&pool).await.unwrap(), 15);
    }

    #[tokio::test]
    async fn given_two_fresh_dbs_when_run_on_each_then_meals_have_identical_names_and_ingredients()
    {
        let (pool_a, _dir_a) = setup_db().await;
        let (pool_b, _dir_b) = setup_db().await;

        run(&pool_a).await.unwrap();
        run(&pool_b).await.unwrap();

        let mut meals_a = db::list_meals(&pool_a, None).await.unwrap();
        let mut meals_b = db::list_meals(&pool_b, None).await.unwrap();

        // Sort by name for stable comparison.
        meals_a.sort_by(|a, b| a.name.cmp(&b.name));
        meals_b.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(meals_a.len(), meals_b.len());
        for (ma, mb) in meals_a.iter().zip(meals_b.iter()) {
            assert_eq!(ma.name, mb.name);
            // Sort ingredients by name for stable comparison.
            let mut ings_a: Vec<(&str, Option<&str>)> = ma
                .ingredients
                .iter()
                .map(|i| (i.name.as_str(), i.quantity.as_deref()))
                .collect();
            let mut ings_b: Vec<(&str, Option<&str>)> = mb
                .ingredients
                .iter()
                .map(|i| (i.name.as_str(), i.quantity.as_deref()))
                .collect();
            ings_a.sort_by(|a, b| a.0.cmp(b.0));
            ings_b.sort_by(|a, b| a.0.cmp(b.0));
            assert_eq!(ings_a, ings_b);
        }
    }

    #[tokio::test]
    async fn given_seeded_db_when_inspected_then_every_meal_has_between_2_and_6_ingredients_with_nonempty_quantity()
     {
        let (pool, _dir) = setup_db().await;
        run(&pool).await.unwrap();
        let meals = db::list_meals(&pool, None).await.unwrap();
        assert_eq!(meals.len(), 15);

        let mut name_set = HashSet::new();
        for meal in &meals {
            let count = meal.ingredients.len();
            assert!(
                (2..=6).contains(&count),
                "meal '{}' has {} ingredients, expected 2..=6",
                meal.name,
                count
            );
            for ing in &meal.ingredients {
                assert!(
                    ing.quantity.as_deref().is_some_and(|q| !q.is_empty()),
                    "ingredient '{}' in meal '{}' has empty quantity",
                    ing.name,
                    meal.name
                );
            }
            // Verify names are unique (deterministic sampling picks distinct names).
            assert!(
                name_set.insert(meal.name.clone()),
                "duplicate meal name: {}",
                meal.name
            );
        }
    }
}
