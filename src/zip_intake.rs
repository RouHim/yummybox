//! Shared intake for ZIP archives uploaded through multipart.
//!
//! Both archive importers (schema.org export and Mealie backup) read one
//! `file` field, enforce the same ceilings and preload image entries the same
//! way. Only the entry layout and the per-recipe adapter differ, so that
//! adapter stays in the importer and the intake lives here.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use axum::extract::Multipart;
use zip::read::ZipArchive;

use crate::error::AppError;
use crate::import::map_multipart_error;

/// Largest accepted archive body.
pub(crate) const MAX_ARCHIVE_SIZE: u64 = 50 * 1024 * 1024;
/// Largest number of recipes one archive may contain.
pub(crate) const MAX_RECIPES: usize = 500;
/// Images larger than this are skipped, so one huge entry cannot exhaust
/// memory; callers treat images as best-effort.
pub(crate) const MAX_IMAGE_SIZE: u64 = 20 * 1024 * 1024;

/// Read the `file` field of a multipart request and open it as a ZIP archive.
/// Body-size rejections keep their 413 status; the rest is a 400.
pub(crate) async fn open(
    multipart: &mut Multipart,
) -> Result<ZipArchive<Cursor<Vec<u8>>>, AppError> {
    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| map_multipart_error(e, "invalid multipart data"))?
    {
        if field.name() == Some("file") {
            bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read file"))?
                    .to_vec(),
            );
            break;
        }
    }
    let bytes = bytes.ok_or_else(|| AppError::BadRequest("missing 'file' field".into()))?;
    if bytes.len() as u64 > MAX_ARCHIVE_SIZE {
        return Err(AppError::PayloadTooLarge(format!(
            "archive exceeds maximum size of {} MB",
            MAX_ARCHIVE_SIZE / (1024 * 1024)
        )));
    }
    ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError::BadRequest(format!("invalid zip file: {e}")))
}

/// Read one archive entry as UTF-8 text.
pub(crate) fn read_string<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String, AppError> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| AppError::BadRequest(format!("zip must contain a {name} entry")))?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| AppError::BadRequest(format!("failed to read {name}: {e}")))?;
    Ok(text)
}

/// Preload every entry `is_image` accepts, keyed by lowercased path so a
/// caller's lowercase lookup matches an archive that mixes case. The filter
/// also sees the lowercased name, so casing is decided once here.
pub(crate) fn read_images<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    is_image: &dyn Fn(&str) -> bool,
) -> Result<HashMap<String, Vec<u8>>, AppError> {
    let mut images = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| AppError::BadRequest(format!("zip read error: {e}")))?;
        let name = entry
            .name()
            .map_err(|e| AppError::BadRequest(format!("zip read error: {e}")))?
            .to_ascii_lowercase();
        if !is_image(&name) || entry.size() > MAX_IMAGE_SIZE {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| AppError::BadRequest(format!("failed to read {name}: {e}")))?;
        images.insert(name, bytes);
    }
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::ZipWriter;

    fn archive_with(entries: &[(&str, &[u8])]) -> ZipArchive<Cursor<Vec<u8>>> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf);
            for (name, bytes) in entries {
                zip.start_file(*name, zip::write::FileOptions::<()>::default())
                    .expect("start_file");
                zip.write_all(bytes).expect("write entry");
            }
            zip.finish().expect("finish");
        }
        ZipArchive::new(buf).expect("open archive")
    }

    #[test]
    fn given_mixed_case_image_entry_when_read_images_then_key_is_lowercased() {
        let mut archive = archive_with(&[("IMAGES/Photo.JPG", b"jpeg-bytes")]);
        let images =
            read_images(&mut archive, &|name| name.starts_with("images/")).expect("read_images");
        assert_eq!(
            images.get("images/photo.jpg"),
            Some(&b"jpeg-bytes".to_vec())
        );
    }

    #[test]
    fn given_entry_rejected_by_filter_when_read_images_then_archive_is_untouched() {
        let mut archive = archive_with(&[("recipes/a.json", b"{}")]);
        let images =
            read_images(&mut archive, &|name| name.starts_with("images/")).expect("read_images");
        assert!(images.is_empty());
        assert_eq!(
            read_string(&mut archive, "recipes/a.json").expect("entry still readable"),
            "{}"
        );
    }

    #[test]
    fn given_missing_entry_when_read_string_then_error_names_the_entry() {
        let mut archive = archive_with(&[("other.json", b"{}")]);
        let err = read_string(&mut archive, "recipes.json").expect_err("missing entry");
        assert!(
            err.to_string().contains("recipes.json"),
            "error should name the entry, got: {err}"
        );
    }

    #[test]
    fn given_non_utf8_entry_when_read_string_then_reports_read_failure() {
        let mut archive = archive_with(&[("recipes.json", &[0xff, 0xfe, 0xfd])]);
        let err = read_string(&mut archive, "recipes.json").expect_err("invalid UTF-8");
        assert!(
            err.to_string().contains("recipes.json"),
            "error should name the entry, got: {err}"
        );
    }
}
