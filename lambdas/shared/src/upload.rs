//! Reguły uploadu: limity, dozwolone typy, podział na części, nazwa pliku.
//!
//! Deklaracje klienta (rozmiar, typ) służą tylko do porównania. Faktyczny typ
//! pliku ustala pipeline po magic bytes (etap 2), rozmiar sprawdza
//! upload-complete na podstawie części zapisanych w S3.

use serde::{Deserialize, Serialize};

/// Limit rozmiaru pliku w MVP (rozdział 3.4).
pub const MAX_UPLOAD_BYTES: u64 = 1024 * 1024 * 1024;

/// Minimalny rozmiar części multipart w S3 (poza ostatnią).
pub const MIN_PART_BYTES: u64 = 5 * 1024 * 1024;

/// Docelowy rozmiar części: małe części = szybkie wznawianie.
pub const DEFAULT_PART_BYTES: u64 = 8 * 1024 * 1024;

/// Maksymalna liczba części w S3.
pub const MAX_PARTS: u64 = 10_000;

/// Maksymalna długość tytułu i nazwy pliku (po sanityzacji).
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_FILENAME_CHARS: usize = 200;

/// Typy, które można zadeklarować w MVP: obrazy i PDF (rozdział 14).
/// SVG, HTML, archiwa i pliki wykonywalne są celowo poza listą (rozdział 8).
pub const ALLOWED_CONTENT_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp", "application/pdf"];

/// Żądanie rozpoczęcia uploadu (POST /uploads).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitUploadRequest {
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// Zwalidowane dane uploadu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadSpec {
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    pub title: Option<String>,
    pub part_size: u64,
    pub part_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UploadValidationError {
    #[error("Plik jest pusty")]
    Empty,
    #[error("Plik przekracza limit {MAX_UPLOAD_BYTES} bajtów")]
    TooLarge,
    #[error("Niedozwolony typ pliku")]
    ContentTypeNotAllowed,
    #[error("Nieprawidłowa nazwa pliku")]
    InvalidFilename,
    #[error("Tytuł może mieć maksymalnie {MAX_TITLE_CHARS} znaków")]
    TitleTooLong,
}

impl InitUploadRequest {
    /// # Errors
    ///
    /// [`UploadValidationError`] dla pustego lub zbyt dużego pliku, typu spoza
    /// listy, pustej nazwy albo za długiego tytułu.
    pub fn validate(self) -> Result<UploadSpec, UploadValidationError> {
        if self.size == 0 {
            return Err(UploadValidationError::Empty);
        }
        if self.size > MAX_UPLOAD_BYTES {
            return Err(UploadValidationError::TooLarge);
        }
        let content_type = self.content_type.trim().to_ascii_lowercase();
        if !ALLOWED_CONTENT_TYPES.contains(&content_type.as_str()) {
            return Err(UploadValidationError::ContentTypeNotAllowed);
        }
        let filename = sanitize_display_text(basename(&self.filename), MAX_FILENAME_CHARS);
        if filename.is_empty() {
            return Err(UploadValidationError::InvalidFilename);
        }
        let title = match self.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(title) if title.chars().count() > MAX_TITLE_CHARS => {
                return Err(UploadValidationError::TitleTooLong);
            }
            Some(title) => Some(sanitize_display_text(title, MAX_TITLE_CHARS)),
            None => None,
        };
        let part_size = part_size_for(self.size);
        Ok(UploadSpec {
            filename,
            size: self.size,
            content_type,
            title,
            part_size,
            part_count: self.size.div_ceil(part_size),
        })
    }
}

/// Rozmiar części: domyślnie 8 MiB, większy tylko gdy trzeba zmieścić się
/// w limicie 10 000 części.
#[must_use]
pub fn part_size_for(size: u64) -> u64 {
    DEFAULT_PART_BYTES
        .max(size.div_ceil(MAX_PARTS))
        .max(MIN_PART_BYTES)
}

/// Oczekiwany rozmiar części `part_number` (numeracja od 1).
#[must_use]
pub fn expected_part_size(size: u64, part_size: u64, part_number: u64) -> u64 {
    let start = (part_number - 1) * part_size;
    part_size.min(size.saturating_sub(start))
}

/// Część zapisana w S3 (z ListParts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredPart {
    pub part_number: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartsCheck {
    /// Wszystkie części są i mają oczekiwane rozmiary.
    Complete,
    /// Brakuje części: klient może dokończyć upload.
    Missing(Vec<u64>),
    /// Rozmiar się nie zgadza: upload trzeba przerwać (fail closed).
    SizeMismatch { actual_total: u64 },
}

/// Porównuje części zapisane w S3 z deklaracją. Liczy się to, co jest w S3,
/// nie to, co twierdzi klient.
#[must_use]
pub fn check_parts(declared_size: u64, part_size: u64, part_count: u64, parts: &[StoredPart]) -> PartsCheck {
    let actual_total: u64 = parts.iter().map(|part| part.size).sum();
    let unexpected = parts.iter().any(|part| {
        part.part_number == 0
            || part.part_number > part_count
            || part.size != expected_part_size(declared_size, part_size, part.part_number)
    });
    if unexpected || actual_total > declared_size {
        return PartsCheck::SizeMismatch { actual_total };
    }
    let missing: Vec<u64> = (1..=part_count)
        .filter(|number| !parts.iter().any(|part| part.part_number == *number))
        .collect();
    if !missing.is_empty() {
        return PartsCheck::Missing(missing);
    }
    if actual_total == declared_size {
        PartsCheck::Complete
    } else {
        PartsCheck::SizeMismatch { actual_total }
    }
}

/// Ostatni segment ścieżki: `../../etc/passwd.jpg` → `passwd.jpg`.
fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Tekst wyłącznie do wyświetlania: bez znaków sterujących, przycięty.
/// Escapowanie HTML robi frontend (Angular), tu nie zmieniamy treści.
fn sanitize_display_text(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(filename: &str, size: u64, content_type: &str) -> InitUploadRequest {
        InitUploadRequest {
            filename: filename.to_owned(),
            size,
            content_type: content_type.to_owned(),
            title: None,
        }
    }

    #[test]
    fn accepts_a_photo() {
        let spec = request("mecz.jpg", 20 * 1024 * 1024, "IMAGE/JPEG")
            .validate()
            .unwrap();
        assert_eq!(spec.content_type, "image/jpeg");
        assert_eq!(spec.part_size, DEFAULT_PART_BYTES);
        assert_eq!(spec.part_count, 3);
    }

    #[test]
    fn rejects_empty_oversized_and_disallowed_files() {
        assert_eq!(
            request("a.jpg", 0, "image/jpeg").validate(),
            Err(UploadValidationError::Empty)
        );
        assert_eq!(
            request("a.jpg", MAX_UPLOAD_BYTES + 1, "image/jpeg").validate(),
            Err(UploadValidationError::TooLarge)
        );
        for content_type in [
            "image/svg+xml",
            "text/html",
            "application/zip",
            "application/x-msdownload",
        ] {
            assert_eq!(
                request("a", 10, content_type).validate(),
                Err(UploadValidationError::ContentTypeNotAllowed),
                "{content_type}"
            );
        }
    }

    #[test]
    fn filename_is_reduced_to_a_display_name() {
        let spec = request("../../etc/passwd.jpg", 10, "image/jpeg")
            .validate()
            .unwrap();
        assert_eq!(spec.filename, "passwd.jpg");
        let spec = request("C:\\temp\\zdj\u{0007}ęcie.png", 10, "image/png")
            .validate()
            .unwrap();
        assert_eq!(spec.filename, "zdjęcie.png");
        assert_eq!(
            request("../", 10, "image/png").validate(),
            Err(UploadValidationError::InvalidFilename)
        );
    }

    #[test]
    fn title_is_limited() {
        let mut long = request("a.jpg", 10, "image/jpeg");
        long.title = Some("x".repeat(MAX_TITLE_CHARS + 1));
        assert_eq!(long.validate(), Err(UploadValidationError::TitleTooLong));

        let mut script = request("a.jpg", 10, "image/jpeg");
        script.title = Some("<script>alert(1)</script>".to_owned());
        // Treść zostaje (to tylko tekst); escapowanie robi frontend przy wyświetlaniu.
        assert_eq!(
            script.validate().unwrap().title.as_deref(),
            Some("<script>alert(1)</script>")
        );
    }

    #[test]
    fn part_size_respects_s3_limits() {
        assert_eq!(part_size_for(1), DEFAULT_PART_BYTES);
        assert_eq!(part_size_for(MAX_UPLOAD_BYTES), DEFAULT_PART_BYTES);
        let huge: u64 = 200 * 1024 * 1024 * 1024;
        assert!(huge.div_ceil(part_size_for(huge)) <= MAX_PARTS);
    }

    fn parts(sizes: &[u64]) -> Vec<StoredPart> {
        sizes
            .iter()
            .enumerate()
            .map(|(i, size)| StoredPart {
                part_number: i as u64 + 1,
                size: *size,
            })
            .collect()
    }

    #[test]
    fn complete_upload_matches_declared_size() {
        assert_eq!(check_parts(25, 10, 3, &parts(&[10, 10, 5])), PartsCheck::Complete);
    }

    #[test]
    fn reports_missing_parts() {
        let stored = vec![StoredPart {
            part_number: 2,
            size: 10,
        }];
        assert_eq!(check_parts(25, 10, 3, &stored), PartsCheck::Missing(vec![1, 3]));
    }

    #[test]
    fn detects_parts_larger_than_declared() {
        // Zadeklarowano 25 B, a klient wysłał większe części (scenariusz 11).
        assert_eq!(
            check_parts(25, 10, 3, &parts(&[10, 10, 50])),
            PartsCheck::SizeMismatch { actual_total: 70 }
        );
        assert_eq!(
            check_parts(25, 10, 3, &parts(&[11])),
            PartsCheck::SizeMismatch { actual_total: 11 }
        );
    }

    #[test]
    fn detects_parts_outside_the_plan() {
        let stored = vec![StoredPart {
            part_number: 4,
            size: 1,
        }];
        assert_eq!(
            check_parts(25, 10, 3, &stored),
            PartsCheck::SizeMismatch { actual_total: 1 }
        );
    }
}
