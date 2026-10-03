//! Lambda `cdr` (rola `dam-cdr`): rekonstrukcja treści (Content Disarm and
//! Reconstruction) przed publikacją (rozdział 7.2, ADR 0007).
//!
//! Obraz jest dekodowany do pikseli i kodowany od nowa w tym samym formacie.
//! W nowym pliku nie ma niczego poza obrazem: znika EXIF/XMP/IPTC (np. XSS
//! w polu EXIF, scenariusz 2), profil ICC, miniatury i każda treść doklejona
//! do pliku (poligloty, scenariusz 5). Z EXIF przepuszczamy tylko whitelistę
//! pól (autor, prawa autorskie, data wykonania) jako metadane assetu, a
//! orientację stosujemy do pikseli, żeby zdjęcie nie było obrócone.
//!
//! Wynik trafia do `clean` pod `staging/<assetId>`; publikowalną kopię tworzy
//! dopiero `finalize-clean`.

use std::io::Cursor;
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use lambda_runtime::{Error, LambdaEvent, service_fn};
use sha2::{Digest, Sha256};
use shared::http::env;
use shared::pipeline::limits::{MAX_DIMENSION, MAX_IMAGE_BYTES};
use shared::pipeline::{DisarmOutcome, PreservedMetadata, StepInput, ValidationOutcome, staging_key};

/// Jakość ponownego kodowania JPEG: wizualnie bez straty dla zdjęć prasowych.
const JPEG_QUALITY: u8 = 90;
/// Maksymalna pamięć dekodera (obraz 100 MP jako RGBA to ok. 400 MB).
const MAX_DECODER_ALLOC: u64 = 1024 * 1024 * 1024;
/// Długość przepuszczanego pola EXIF.
const MAX_FIELD_CHARS: usize = 200;

struct App {
    s3: aws_sdk_s3::Client,
    quarantine: String,
    clean: String,
}

/// Zrekonstruowany plik i przepuszczone metadane.
#[derive(Debug)]
struct Disarmed {
    bytes: Vec<u8>,
    metadata: PreservedMetadata,
}

fn format_of(detected_type: &str) -> Option<ImageFormat> {
    match detected_type {
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/png" => Some(ImageFormat::Png),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

/// Tekst z EXIF bez znaków sterujących i nawiasów kątowych, przycięty.
fn sanitize(raw: &[u8]) -> Option<String> {
    let text: String = String::from_utf8_lossy(raw)
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>'))
        .take(MAX_FIELD_CHARS)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Whitelista pól EXIF i orientacja. Błędny lub brakujący EXIF to nie błąd:
/// rekonstrukcja i tak go usuwa.
fn read_exif(bytes: &[u8]) -> (PreservedMetadata, Option<Orientation>) {
    let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) else {
        return (PreservedMetadata::default(), None);
    };
    let ascii = |tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
        Some(exif::Value::Ascii(values)) => values.first().and_then(|v| sanitize(v)),
        _ => None,
    };
    let orientation = exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .and_then(|v| u8::try_from(v).ok())
        .and_then(Orientation::from_exif);
    let metadata = PreservedMetadata {
        artist: ascii(exif::Tag::Artist),
        copyright: ascii(exif::Tag::Copyright),
        taken_at: ascii(exif::Tag::DateTimeOriginal),
    };
    (metadata, orientation)
}

/// Dekoduje obraz z limitami i koduje go od nowa w tym samym formacie.
fn disarm(bytes: &[u8], detected_type: &str) -> Result<Disarmed, String> {
    let format = format_of(detected_type).ok_or_else(|| format!("brak CDR dla typu {detected_type}"))?;
    let (metadata, orientation) = read_exif(bytes);

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODER_ALLOC);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let mut image = reader.decode().map_err(|e| format!("dekodowanie obrazu: {e}"))?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }

    let mut out = Vec::new();
    let encoded = match format {
        // JPEG nie ma kanału alfa.
        ImageFormat::Jpeg => DynamicImage::ImageRgb8(image.to_rgb8())
            .write_with_encoder(JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)),
        ImageFormat::Png => image.write_with_encoder(PngEncoder::new(&mut out)),
        _ => (if image.color().has_alpha() {
            DynamicImage::ImageRgba8(image.to_rgba8())
        } else {
            DynamicImage::ImageRgb8(image.to_rgb8())
        })
        .write_with_encoder(WebPEncoder::new_lossless(&mut out)),
    };
    encoded.map_err(|e| format!("kodowanie obrazu: {e}"))?;
    Ok(Disarmed { bytes: out, metadata })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

async fn handler(app: &App, event: LambdaEvent<StepInput>) -> Result<DisarmOutcome, Error> {
    let input = event.payload;
    let asset_id = input.checked_asset_id()?;
    let Some(ValidationOutcome::Valid {
        detected_type,
        size_bytes,
        ..
    }) = &input.validation
    else {
        return Err("cdr bez pozytywnej walidacji".into());
    };
    if *size_bytes > MAX_IMAGE_BYTES {
        return Err("plik większy niż limit CDR".into());
    }

    let original = app
        .s3
        .get_object()
        .bucket(&app.quarantine)
        .key(asset_id)
        .send()
        .await
        .map_err(|e| format!("odczyt z kwarantanny: {e:?}"))?
        .body
        .collect()
        .await
        .map_err(|e| format!("odczyt z kwarantanny: {e}"))?
        .into_bytes();

    let disarmed = match disarm(&original, detected_type) {
        Ok(disarmed) => disarmed,
        Err(reason) => {
            tracing::warn!(asset_id, %reason, "reconstruction failed");
            return Ok(DisarmOutcome::Rejected { reason });
        }
    };
    let sha256 = sha256_hex(&disarmed.bytes);
    let size = disarmed.bytes.len() as u64;
    app.s3
        .put_object()
        .bucket(&app.clean)
        .key(staging_key(asset_id))
        .content_type(detected_type)
        .body(disarmed.bytes.into())
        .send()
        .await
        .map_err(|e| format!("zapis do clean/staging: {e:?}"))?;
    tracing::info!(
        asset_id,
        original = original.len(),
        reconstructed = size,
        "asset reconstructed"
    );
    Ok(DisarmOutcome::Clean {
        size_bytes: size,
        sha256,
        metadata: disarmed.metadata,
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    shared::telemetry::init();
    let config = aws_config::load_from_env().await;
    let app = Arc::new(App {
        s3: aws_sdk_s3::Client::new(&config),
        quarantine: env("QUARANTINE_BUCKET"),
        clean: env("CLEAN_BUCKET"),
    });
    lambda_runtime::run(service_fn(move |event: LambdaEvent<StepInput>| {
        let app = Arc::clone(&app);
        async move { handler(&app, event).await }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = RgbImage::from_pixel(width, height, Rgb([200, 30, 30]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(image)
            .write_with_encoder(JpegEncoder::new_with_quality(&mut out, 80))
            .unwrap();
        out
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = RgbImage::from_pixel(width, height, Rgb([10, 200, 10]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(image)
            .write_with_encoder(PngEncoder::new(&mut out))
            .unwrap();
        out
    }

    /// Minimalny blok TIFF z polami EXIF jako ASCII (little endian).
    fn tiff(fields: &[(u16, &[u8])], orientation: Option<u16>) -> Vec<u8> {
        let count = fields.len() + usize::from(orientation.is_some());
        let mut ifd = Vec::new();
        let mut data = Vec::new();
        let data_start = 8 + 2 + count * 12 + 4;
        let mut entries: Vec<(u16, Vec<u8>)> = Vec::new();
        if let Some(value) = orientation {
            let mut entry = Vec::new();
            entry.extend_from_slice(&0x0112u16.to_le_bytes());
            entry.extend_from_slice(&3u16.to_le_bytes());
            entry.extend_from_slice(&1u32.to_le_bytes());
            entry.extend_from_slice(&value.to_le_bytes());
            entry.extend_from_slice(&[0, 0]);
            entries.push((0x0112, entry));
        }
        for (tag, value) in fields {
            let mut bytes = value.to_vec();
            bytes.push(0);
            let offset = u32::try_from(data_start + data.len()).unwrap();
            let mut entry = Vec::new();
            entry.extend_from_slice(&tag.to_le_bytes());
            entry.extend_from_slice(&2u16.to_le_bytes());
            entry.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
            entry.extend_from_slice(&offset.to_le_bytes());
            data.extend_from_slice(&bytes);
            entries.push((*tag, entry));
        }
        entries.sort_by_key(|(tag, _)| *tag);
        ifd.extend_from_slice(&u16::try_from(count).unwrap().to_le_bytes());
        for (_, entry) in entries {
            ifd.extend_from_slice(&entry);
        }
        ifd.extend_from_slice(&0u32.to_le_bytes());
        let mut out = b"II*\0\x08\0\0\0".to_vec();
        out.extend_from_slice(&ifd);
        out.extend_from_slice(&data);
        out
    }

    /// JPEG z segmentem APP1 (EXIF) wstawionym zaraz po SOI.
    fn jpeg_with_exif(base: &[u8], tiff: &[u8]) -> Vec<u8> {
        let mut segment = b"Exif\0\0".to_vec();
        segment.extend_from_slice(tiff);
        let len = u16::try_from(segment.len() + 2).unwrap();
        let mut out = base[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&segment);
        out.extend_from_slice(&base[2..]);
        out
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle)
    }

    #[test]
    fn strips_exif_payloads_and_keeps_whitelisted_fields() {
        // Scenariusz 2: XSS w polu EXIF nie przetrwa rekonstrukcji.
        let payload: &[u8] = b"<script>alert(1)</script>";
        let exif = tiff(
            &[
                (0x010E, payload),
                (0x013B, b"Jan Fotograf"),
                (0x8298, b"KS Matchday"),
            ],
            None,
        );
        let original = jpeg_with_exif(&jpeg(32, 16), &exif);
        assert!(contains(&original, payload));

        let disarmed = disarm(&original, "image/jpeg").unwrap();
        assert!(!contains(&disarmed.bytes, payload));
        assert!(!contains(&disarmed.bytes, b"Exif\0\0"));
        assert_eq!(disarmed.metadata.artist.as_deref(), Some("Jan Fotograf"));
        assert_eq!(disarmed.metadata.copyright.as_deref(), Some("KS Matchday"));
        let decoded = image::load_from_memory_with_format(&disarmed.bytes, ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (32, 16));
    }

    #[test]
    fn applies_orientation_before_dropping_exif() {
        // Orientacja 6 = obrót o 90°: piksele obracamy, żeby zdjęcie po CDR nie leżało bokiem.
        let original = jpeg_with_exif(&jpeg(32, 16), &tiff(&[], Some(6)));
        let disarmed = disarm(&original, "image/jpeg").unwrap();
        let decoded = image::load_from_memory(&disarmed.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (16, 32));
    }

    #[test]
    fn drops_content_appended_to_a_polyglot() {
        // Scenariusz 5: poprawny PNG z doklejonym HTML-em.
        let html: &[u8] = b"<html><script>alert(document.cookie)</script></html>";
        let mut polyglot = png(8, 8);
        polyglot.extend_from_slice(html);
        let disarmed = disarm(&polyglot, "image/png").unwrap();
        assert!(!contains(&disarmed.bytes, html));
        assert!(image::load_from_memory_with_format(&disarmed.bytes, ImageFormat::Png).is_ok());
    }

    #[test]
    fn rejects_data_that_is_not_a_valid_image() {
        let mut truncated = jpeg(64, 64);
        truncated.truncate(40);
        assert!(disarm(&truncated, "image/jpeg").is_err());
        assert!(disarm(b"not an image", "image/png").is_err());
        assert!(disarm(&png(4, 4), "application/pdf").is_err());
    }

    #[test]
    fn reencodes_webp_losslessly() {
        let mut source = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::from_pixel(5, 7, Rgb([1, 2, 3])))
            .write_with_encoder(WebPEncoder::new_lossless(&mut source))
            .unwrap();
        let disarmed = disarm(&source, "image/webp").unwrap();
        let decoded = image::load_from_memory_with_format(&disarmed.bytes, ImageFormat::WebP).unwrap();
        assert_eq!(decoded.to_rgb8().get_pixel(0, 0), &Rgb([1, 2, 3]));
    }

    #[test]
    fn sanitizes_exif_text() {
        assert_eq!(
            sanitize(b"  Jan\x07 <b>Kowalski</b> ").as_deref(),
            Some("Jan bKowalski/b")
        );
        assert_eq!(sanitize(b"\x00\x01"), None);
        assert_eq!(sanitize(&[b'a'; 500]).map(|s| s.len()), Some(MAX_FIELD_CHARS));
    }

    #[test]
    fn hashes_reconstructed_bytes() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Pliki z tests/security-fixtures (te same, które wgrywa test e2e).
    mod fixtures {
        use super::*;

        macro_rules! fixture {
            ($name:literal) => {
                include_bytes!(concat!("../../../../tests/security-fixtures/", $name)).as_slice()
            };
        }

        const XSS: &[u8] = b"<script>alert(document.domain)</script>";

        #[test]
        fn exif_xss_is_removed() {
            let disarmed = disarm(fixture!("exif-xss.jpg"), "image/jpeg").unwrap();
            assert!(!contains(&disarmed.bytes, XSS));
            // Pola z nawiasami kątowymi po sanityzacji nie zawierają znacznika.
            assert!(
                disarmed
                    .metadata
                    .artist
                    .as_deref()
                    .is_none_or(|a| !a.contains('<'))
            );
        }

        #[test]
        fn polyglot_payload_is_removed() {
            let disarmed = disarm(fixture!("polyglot.png"), "image/png").unwrap();
            assert!(!contains(&disarmed.bytes, b"<html>"));
        }
    }
}
