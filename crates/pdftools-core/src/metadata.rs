//! Reading and editing the document information dictionary.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use lopdf::{decode_text_string, text_string, Dictionary, Document, Object};

use crate::Error;

/// Document properties. Empty strings are treated as absent when writing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// ISO 8601; read-only.
    pub created: Option<String>,
    /// ISO 8601; read-only.
    pub modified: Option<String>,
}

const TEXT_FIELDS: [&[u8]; 6] = [b"Title", b"Author", b"Subject", b"Keywords", b"Creator", b"Producer"];

impl Metadata {
    fn text_fields(&self) -> [&Option<String>; 6] {
        [&self.title, &self.author, &self.subject, &self.keywords, &self.creator, &self.producer]
    }
}

pub fn read(doc: &Document) -> Metadata {
    let Some(info) = info_dictionary(doc) else {
        return Metadata::default();
    };
    let text = |key: &[u8]| {
        let object = info.get(key).ok()?;
        let object = doc.dereference(object).ok()?.1;
        decode_text_string(object).ok().filter(|s| !s.trim().is_empty())
    };
    let date = |key: &[u8]| text(key).and_then(|raw| parse_pdf_date(&raw));
    Metadata {
        title: text(b"Title"),
        author: text(b"Author"),
        subject: text(b"Subject"),
        keywords: text(b"Keywords"),
        creator: text(b"Creator"),
        producer: text(b"Producer"),
        created: date(b"CreationDate"),
        modified: date(b"ModDate"),
    }
}

/// Replaces the editable fields with `metadata` and stamps a new ModDate.
/// The XMP metadata stream is dropped, since it would otherwise contradict
/// the new values in readers that prefer it.
pub fn write(doc: &mut Document, metadata: &Metadata) -> Result<(), Error> {
    let mut info = info_dictionary(doc).cloned().unwrap_or_default();
    for (key, value) in TEXT_FIELDS.iter().zip(metadata.text_fields()) {
        match value.as_deref().map(str::trim) {
            Some(value) if !value.is_empty() => info.set(*key, text_string(value)),
            _ => {
                info.remove(key);
            }
        }
    }
    info.set("ModDate", Object::string_literal(pdf_date_now()));

    let info_id = match doc.trailer.get(b"Info").and_then(Object::as_reference) {
        Ok(id) => {
            doc.objects.insert(id, Object::Dictionary(info));
            id
        }
        Err(_) => doc.add_object(info),
    };
    doc.trailer.set("Info", Object::Reference(info_id));
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }
    Ok(())
}

pub fn write_file(input: &Path, metadata: &Metadata, output: &Path) -> Result<(), Error> {
    let mut doc = crate::load(input)?;
    write(&mut doc, metadata)?;
    doc.prune_objects();
    crate::save(&mut doc, output)
}

fn info_dictionary(doc: &Document) -> Option<&Dictionary> {
    let info = doc.trailer.get(b"Info").ok()?;
    doc.dereference(info).ok()?.1.as_dict().ok()
}

/// Converts a PDF date (`D:YYYYMMDDHHmmSSOHH'mm'`, all but the year
/// optional) to ISO 8601.
fn parse_pdf_date(raw: &str) -> Option<String> {
    let s = raw.trim().strip_prefix("D:").unwrap_or(raw.trim());
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let part = |start: usize, default: &str| digits.get(start..start + 2).unwrap_or(default).to_owned();
    let (year, month, day) = (&digits[..4], part(4, "01"), part(6, "01"));
    let (hour, minute, second) = (part(8, "00"), part(10, "00"), part(12, "00"));

    let rest = &s[digits.len()..];
    let zone = match rest.chars().next() {
        Some('Z') => "Z".to_owned(),
        Some(sign @ ('+' | '-')) => {
            let tz: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let hh = tz.get(..2).unwrap_or("00");
            let mm = tz.get(2..4).unwrap_or("00");
            format!("{sign}{hh}:{mm}")
        }
        _ => String::new(),
    };
    Some(format!("{year}-{month}-{day}T{hour}:{minute}:{second}{zone}"))
}

fn pdf_date_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "D:{year:04}{month:02}{day:02}{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to (year, month, day); Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{round_trip, sample};

    #[test]
    fn writes_and_reads_back_unicode_fields() {
        let mut doc = sample("a", 1);
        let metadata = Metadata {
            title: Some("Отчёт 2026".into()),
            author: Some("Ada".into()),
            keywords: Some("  ".into()),
            ..Default::default()
        };
        write(&mut doc, &metadata).unwrap();
        let read_back = read(&round_trip(doc));
        assert_eq!(read_back.title.as_deref(), Some("Отчёт 2026"));
        assert_eq!(read_back.author.as_deref(), Some("Ada"));
        assert_eq!(read_back.keywords, None);
        assert!(read_back.modified.is_some());
    }

    #[test]
    fn clearing_removes_fields() {
        let mut doc = sample("a", 1);
        write(&mut doc, &Metadata { title: Some("x".into()), ..Default::default() }).unwrap();
        write(&mut doc, &Metadata::default()).unwrap();
        assert_eq!(read(&doc).title, None);
    }

    #[test]
    fn parses_pdf_dates() {
        assert_eq!(parse_pdf_date("D:20230115123000+01'00'").as_deref(), Some("2023-01-15T12:30:00+01:00"));
        assert_eq!(parse_pdf_date("D:2023").as_deref(), Some("2023-01-01T00:00:00"));
        assert_eq!(parse_pdf_date("D:20230115Z").as_deref(), Some("2023-01-15T00:00:00Z"));
        assert_eq!(parse_pdf_date("garbage"), None);
    }

    #[test]
    fn converts_days_to_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_740), (2026, 10, 14));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }
}
