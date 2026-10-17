//! Page range syntax: "1-3, 5, 8-" (open ends allowed: "-4" and "8-").

/// Parses each comma-separated item into its own list of pages.
pub fn parse_groups(text: &str, total: usize) -> Result<Vec<Vec<usize>>, String> {
    let items: Vec<&str> = text.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()).collect();
    if items.is_empty() {
        return Err("Enter page ranges, e.g. 1-3, 4-6".into());
    }
    items
        .into_iter()
        .map(|item| {
            let (from, to) = match item.split_once(['-', '–']) {
                Some((a, b)) => (a.trim(), Some(b.trim())),
                None => (item, None),
            };
            let number = |s: &str, default: usize| -> Result<usize, String> {
                if s.is_empty() {
                    Ok(default)
                } else {
                    s.parse().map_err(|_| format!("“{item}” isn’t a page or range"))
                }
            };
            if from.is_empty() && to.is_none_or(str::is_empty) {
                return Err(format!("“{item}” isn’t a page or range"));
            }
            let start = number(from, 1)?;
            let end = match to {
                Some(to) => number(to, total)?,
                None => start,
            };
            if start < 1 || end < 1 || start > total || end > total {
                return Err(format!("“{item}” is outside 1–{total}"));
            }
            if start > end {
                return Err(format!("“{item}” runs backwards"));
            }
            Ok((start..=end).collect())
        })
        .collect()
}

/// Parses ranges into a sorted list of distinct pages.
pub fn parse_list(text: &str, total: usize) -> Result<Vec<usize>, String> {
    let mut pages: Vec<usize> = parse_groups(text, total)?.into_iter().flatten().collect();
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

/// Formats pages compactly, e.g. [1, 2, 3, 5] → "1-3, 5".
pub fn format_list(pages: &[usize]) -> String {
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        while i + 1 < sorted.len() && sorted[i + 1] == sorted[i] + 1 {
            i += 1;
        }
        parts.push(if start == sorted[i] { start.to_string() } else { format!("{start}-{}", sorted[i]) });
        i += 1;
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ranges() {
        assert_eq!(parse_groups("1-3, 5, 8-", 9).unwrap(), vec![vec![1, 2, 3], vec![5], vec![8, 9]]);
        assert_eq!(parse_groups("-2; 4–4", 5).unwrap(), vec![vec![1, 2], vec![4]]);
        assert_eq!(parse_list("3, 1-2, 2", 5).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn explains_bad_ranges() {
        assert_eq!(parse_groups("", 3).unwrap_err(), "Enter page ranges, e.g. 1-3, 4-6");
        assert_eq!(parse_groups("1-3, 40", 14).unwrap_err(), "“40” is outside 1–14");
        assert_eq!(parse_groups("5-2", 9).unwrap_err(), "“5-2” runs backwards");
        assert_eq!(parse_groups("x", 9).unwrap_err(), "“x” isn’t a page or range");
        assert_eq!(parse_groups("-", 9).unwrap_err(), "“-” isn’t a page or range");
        assert_eq!(parse_groups("0", 9).unwrap_err(), "“0” is outside 1–9");
    }

    #[test]
    fn formats_lists() {
        assert_eq!(format_list(&[5, 1, 2, 3, 3]), "1-3, 5");
        assert_eq!(format_list(&[]), "");
    }
}
