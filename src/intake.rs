//! One seam for turning a parsed recipe into a stored meal.
//!
//! Every import path ends here, so the duplicate policy, the image conversion
//! policy and the validation order have a single home.

use sqlx::SqlitePool;

use crate::db;
use crate::error::AppError;
use crate::image;
use crate::model::{Meal, NewMeal};

/// The image that goes with the meal being inserted.
pub(crate) enum ImageInput<'a> {
    /// No image.
    Keep,
    /// Raw bytes in any supported format; a decode failure fails the insert.
    Set(&'a [u8]),
    /// Candidate images in any supported format; the first one that decodes
    /// is used, and if none does the meal is inserted without an image.
    TryAny(&'a [&'a [u8]]),
}

/// What happened to the meal.
#[derive(Debug)]
pub(crate) enum IntakeOutcome {
    Created(Meal),
    /// A meal with the same normalized name already exists.
    Duplicate,
}

/// Whether a meal with this normalized name already exists.
///
/// [`insert`] checks this too; importers that want to skip duplicates before
/// doing any parsing call it first, exactly as the pre-intake importers did.
pub(crate) async fn name_taken(pool: &SqlitePool, name: &str) -> Result<bool, AppError> {
    db::meal_name_exists(pool, name, None).await
}

/// Insert a parsed meal: reject a duplicate name, convert the image, then
/// validate and write. `db::insert_meal` owns validation, so it runs once.
pub(crate) async fn insert(
    pool: &SqlitePool,
    meal: NewMeal,
    image_input: ImageInput<'_>,
) -> Result<IntakeOutcome, AppError> {
    if name_taken(pool, &meal.name).await? {
        return Ok(IntakeOutcome::Duplicate);
    }

    let jpeg = match image_input {
        ImageInput::Keep => None,
        ImageInput::Set(bytes) => Some(image::convert_to_jpeg(bytes)?),
        ImageInput::TryAny(candidates) => candidates
            .iter()
            .find_map(|bytes| image::convert_to_jpeg(bytes).ok()),
    };
    let image = match &jpeg {
        Some(bytes) => db::ImageChange::Set(bytes),
        None => db::ImageChange::Keep,
    };

    let meal = db::insert_meal(pool, meal, image).await?;
    Ok(IntakeOutcome::Created(meal))
}

/// Reason text for a failed insert, shared by the bulk and archive importers.
pub(crate) fn failure_reason(err: &AppError) -> String {
    match err {
        AppError::Validation(message) => format!("validation failed: {message}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ::image::ImageEncoder;

    use super::*;
    use crate::model::NewIngredientLine;

    async fn setup() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = db::init_db(&db_path).await.expect("init_db");
        (pool, dir)
    }

    fn meal(name: &str) -> NewMeal {
        NewMeal {
            name: name.into(),
            ingredients: vec![NewIngredientLine {
                name: "salt".into(),
                quantity: Some("1 tsp".into()),
            }],
            instructions: "Cook.".into(),
            portions: Some(2),
            source_url: None,
        }
    }

    /// A 4x4 PNG, large enough for the image crate to decode.
    fn png() -> Vec<u8> {
        let img = ::image::RgbaImage::from_pixel(4, 4, ::image::Rgba([10, 20, 30, 255]));
        let mut buf = Cursor::new(Vec::new());
        ::image::codecs::png::PngEncoder::new(&mut buf)
            .write_image(img.as_raw(), 4, 4, ::image::ExtendedColorType::Rgba8)
            .expect("encode png");
        buf.into_inner()
    }

    #[tokio::test]
    async fn given_existing_name_when_insert_then_returns_duplicate_and_writes_nothing() {
        let (pool, _dir) = setup().await;
        insert(&pool, meal("Soup"), ImageInput::Keep)
            .await
            .expect("first insert");
        let outcome = insert(&pool, meal("  soup "), ImageInput::Keep)
            .await
            .expect("second insert");
        assert!(matches!(outcome, IntakeOutcome::Duplicate));
        assert_eq!(db::meals_count(&pool).await.expect("count"), 1);
    }

    #[tokio::test]
    async fn given_decodable_image_when_insert_then_meal_has_image() {
        let (pool, _dir) = setup().await;
        let bytes = png();
        let outcome = insert(&pool, meal("Photo"), ImageInput::Set(&bytes))
            .await
            .expect("insert");
        match outcome {
            IntakeOutcome::Created(created) => assert!(created.has_image),
            IntakeOutcome::Duplicate => panic!("unexpected duplicate"),
        }
    }

    #[tokio::test]
    async fn given_undecodable_image_when_insert_set_then_fails_without_writing() {
        let (pool, _dir) = setup().await;
        let err = insert(&pool, meal("Broken"), ImageInput::Set(b"not an image"))
            .await
            .expect_err("invalid image must fail");
        assert!(matches!(err, AppError::BadRequest(_)), "got: {err}");
        assert_eq!(db::meals_count(&pool).await.expect("count"), 0);
    }

    #[tokio::test]
    async fn given_undecodable_image_when_insert_try_any_then_creates_meal_without_image() {
        let (pool, _dir) = setup().await;
        let outcome = insert(
            &pool,
            meal("No Photo"),
            ImageInput::TryAny(&[b"not an image"]),
        )
        .await
        .expect("insert");
        match outcome {
            IntakeOutcome::Created(created) => assert!(!created.has_image),
            IntakeOutcome::Duplicate => panic!("unexpected duplicate"),
        }
    }

    #[tokio::test]
    async fn given_first_candidate_broken_when_insert_try_any_then_uses_the_next_one() {
        let (pool, _dir) = setup().await;
        let valid = png();
        let outcome = insert(
            &pool,
            meal("Fallback Photo"),
            ImageInput::TryAny(&[b"not an image", valid.as_slice()]),
        )
        .await
        .expect("insert");
        match outcome {
            IntakeOutcome::Created(created) => assert!(created.has_image),
            IntakeOutcome::Duplicate => panic!("unexpected duplicate"),
        }
    }

    #[tokio::test]
    async fn given_invalid_meal_when_insert_then_returns_validation_error() {
        let (pool, _dir) = setup().await;
        let err = insert(&pool, meal(""), ImageInput::Keep)
            .await
            .expect_err("empty name must fail validation");
        assert!(matches!(err, AppError::Validation(_)), "got: {err}");
        assert_eq!(db::meals_count(&pool).await.expect("count"), 0);
    }

    #[test]
    fn given_validation_error_when_failure_reason_then_includes_detail() {
        let err = AppError::Validation("name must be 1–200 characters".into());
        assert_eq!(
            failure_reason(&err),
            "validation failed: name must be 1–200 characters"
        );
    }

    #[test]
    fn given_other_error_when_failure_reason_then_returns_message() {
        let err = AppError::BadRequest("something wrong".into());
        assert_eq!(failure_reason(&err), "something wrong");
    }
}
