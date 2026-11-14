use std::io::Cursor;

use crate::error::AppError;
use image::DynamicImage;
use image::ImageEncoder;
use image::ImageReader;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;

/// JPEG quality for stored images (0-100).
pub const JPEG_QUALITY: u8 = 82;
/// MIME type for stored images.
pub const JPEG_CONTENT_TYPE: &str = "image/jpeg";
/// Maximum pixel count on the longer edge after downscale (UHD-4K: 3840).
/// Images whose longer edge exceeds this are resized down to fit,
/// preserving aspect ratio. This is a downscale-only cap — smaller
/// images pass through unchanged.
pub const MAX_LONG_EDGE: u32 = 3840;
/// Longest edge a decoder will accept before giving up. Well above any
/// real camera output, far below what a decompression bomb would ask for.
const MAX_DECODE_EDGE: u32 = 12_000;
/// Ceiling for the decoded pixel buffer. A 12k x 12k RGBA image needs
/// 576 MB, so this rejects the top of the allowed range before allocating.
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// Decode `bytes` (any common format), downscale to [`MAX_LONG_EDGE`] on
/// the longer edge (preserving aspect ratio), re-encode as JPEG at
/// [`JPEG_QUALITY`], and return the JPEG bytes.
pub fn convert_to_jpeg(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits.max_image_width = Some(MAX_DECODE_EDGE);
    limits.max_image_height = Some(MAX_DECODE_EDGE);

    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| AppError::BadRequest("file is not a recognizable image format".into()))?;
    reader.limits(limits);

    let img = reader
        .decode()
        .map_err(|_| AppError::BadRequest("image data is corrupt or unsupported".into()))?;

    let (w, h) = (img.width(), img.height());
    let longer = w.max(h);

    let resized: DynamicImage = if longer > MAX_LONG_EDGE {
        let (new_w, new_h) = if w >= h {
            let nh = ((h as u64 * MAX_LONG_EDGE as u64) / w as u64) as u32;
            (MAX_LONG_EDGE, nh)
        } else {
            let nw = ((w as u64 * MAX_LONG_EDGE as u64) / h as u64) as u32;
            (nw, MAX_LONG_EDGE)
        };
        img.resize_exact(new_w, new_h, FilterType::Triangle)
    } else {
        img
    };

    let rgb = resized.to_rgb8();
    let (w, h) = rgb.dimensions();

    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    encoder
        .write_image(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Internal(format!("image encoding error: {e}")))?;

    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use image::ImageReader;

    use super::*;

    /// Build a small in-memory PNG from a solid-color pixel buffer.
    fn build_png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([120, 180, 60, 255]));
        let mut buf = Cursor::new(Vec::new());
        image::codecs::png::PngEncoder::new(&mut buf)
            .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn given_png_bytes_when_convert_then_output_is_valid_jpeg() {
        let png = build_png(100, 100);
        let jpeg = convert_to_jpeg(&png).expect("convert_to_jpeg failed");
        // Verify JPEG header
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "not a valid JPEG");
        // Decode back to verify integrity
        let decoded = ImageReader::new(Cursor::new(&jpeg))
            .with_guessed_format()
            .unwrap()
            .decode()
            .expect("decoding output JPEG failed");
        assert_eq!(decoded.width(), 100);
        assert_eq!(decoded.height(), 100);
    }

    #[test]
    fn given_image_longer_edge_under_4k_when_convert_then_output_dimensions_unchanged() {
        let png = build_png(1920, 1080);
        let jpeg = convert_to_jpeg(&png).expect("convert_to_jpeg failed");
        let decoded = ImageReader::new(Cursor::new(&jpeg))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(decoded.width(), 1920);
        assert_eq!(decoded.height(), 1080);
    }

    #[test]
    fn given_image_longer_edge_over_4k_when_convert_then_output_longer_edge_equals_3840_and_aspect_preserved()
     {
        // 8000×4500 → should downscale to 3840×2160
        let png = build_png(8000, 4500);
        let jpeg = convert_to_jpeg(&png).expect("convert_to_jpeg failed");
        let decoded = ImageReader::new(Cursor::new(&jpeg))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        // Within ±1 pixel rounding tolerance
        assert!(
            (decoded.width() as i32 - 3840).abs() <= 1,
            "expected width ~3840, got {}",
            decoded.width()
        );
        assert!(
            (decoded.height() as i32 - 2160).abs() <= 1,
            "expected height ~2160, got {}",
            decoded.height()
        );
    }

    #[test]
    fn given_text_bytes_when_convert_then_returns_bad_request() {
        let err = convert_to_jpeg(b"this is not an image").unwrap_err();
        match err {
            AppError::BadRequest(_) => {} // any BadRequest is fine — the image crate may vary the exact message
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }
    #[test]
    fn given_truncated_png_bytes_when_convert_then_returns_bad_request() {
        // First 20 bytes of a PNG magic + header, truncated
        let truncated = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR header
            0x00, 0x00, 0x04, 0x00, // width=1024  (truncated, no further chunks)
        ];
        let err = convert_to_jpeg(&truncated).unwrap_err();
        match err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("corrupt") || msg.contains("unsupported"));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }
}
