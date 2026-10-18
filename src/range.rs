//! Parsing of Unicode code point range specifications.
//!
//! The accepted grammar is:
//!
//! ```text
//! spec   = range ( "," range )*
//! range  = point [ "-" point ]
//! point  = "0x" hexdigit+
//! ```
//!
//! Ranges are inclusive, may overlap, and are merged before use.

use std::fmt;

/// Highest valid Unicode scalar value.
pub const MAX_CODE_POINT: u32 = 0x10_FFFF;

const SURROGATE_START: u32 = 0xD800;
const SURROGATE_END: u32 = 0xDFFF;

/// An inclusive range of Unicode code points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodePointRange {
    pub start: u32,
    pub end: u32,
}

impl CodePointRange {
    /// Number of code points in the range.
    pub fn count(self) -> usize {
        (self.end - self.start) as usize + 1
    }
}

/// A malformed range specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    /// The whole specification was empty or whitespace only.
    Empty,
    /// A comma-separated item was empty (leading, trailing, or doubled comma).
    EmptyItem,
    /// A point did not start with `0x`.
    MissingPrefix { item: String },
    /// A point had no digits, or a non-hexadecimal digit.
    InvalidHex { item: String },
    /// More than one `-` inside a single item.
    TooManyDashes { item: String },
    /// `start > end`.
    Reversed { item: String, start: u32, end: u32 },
    /// A point outside `0x0..=0x10FFFF`.
    OutOfRange { item: String, value: u64 },
    /// A range overlapping `0xD800..=0xDFFF`.
    Surrogate { item: String, start: u32, end: u32 },
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty range specification"),
            Self::EmptyItem => f.write_str("empty range in specification (check for stray commas)"),
            Self::MissingPrefix { item } => {
                write!(f, "invalid range \"{item}\": expected a \"0x\" prefix")
            }
            Self::InvalidHex { item } => {
                write!(f, "invalid range \"{item}\": expected hexadecimal digits")
            }
            Self::TooManyDashes { item } => {
                write!(f, "invalid range \"{item}\": expected at most one \"-\"")
            }
            Self::Reversed { item, start, end } => write!(
                f,
                "invalid range \"{item}\": start 0x{start:X} is greater than end 0x{end:X}"
            ),
            Self::OutOfRange { item, value } => write!(
                f,
                "invalid range \"{item}\": 0x{value:X} is outside Unicode (max 0x10FFFF)"
            ),
            Self::Surrogate { item, start, end } => {
                write!(
                    f,
                    "invalid range \"{item}\": 0x{start:X}..0x{end:X} is a surrogate range"
                )
            }
        }
    }
}

impl std::error::Error for RangeError {}

/// Parses one or more specifications and merges them into sorted, disjoint
/// ranges.
///
/// Overlapping and adjacent ranges are merged, so the result never contains a
/// code point twice.
pub fn parse_specs(specs: &[String]) -> Result<Vec<CodePointRange>, RangeError> {
    let mut ranges = Vec::new();
    for spec in specs {
        if spec.trim().is_empty() {
            return Err(RangeError::Empty);
        }
        for item in spec.split(',') {
            ranges.push(parse_item(item.trim())?);
        }
    }
    Ok(merge(ranges))
}

fn parse_item(item: &str) -> Result<CodePointRange, RangeError> {
    if item.is_empty() {
        return Err(RangeError::EmptyItem);
    }

    let mut parts = item.split('-');
    let first = parts.next().expect("split always yields one element");
    let second = parts.next();
    if parts.next().is_some() {
        return Err(RangeError::TooManyDashes {
            item: item.to_owned(),
        });
    }

    let start = parse_point(first, item)?;
    let end = match second {
        Some(second) => parse_point(second, item)?,
        None => start,
    };

    if start > end {
        return Err(RangeError::Reversed {
            item: item.to_owned(),
            start,
            end,
        });
    }
    if start <= SURROGATE_END && end >= SURROGATE_START {
        return Err(RangeError::Surrogate {
            item: item.to_owned(),
            start,
            end,
        });
    }

    Ok(CodePointRange { start, end })
}

fn parse_point(text: &str, item: &str) -> Result<u32, RangeError> {
    let text = text.trim();
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .ok_or_else(|| RangeError::MissingPrefix {
            item: item.to_owned(),
        })?;

    if digits.is_empty() {
        return Err(RangeError::InvalidHex {
            item: item.to_owned(),
        });
    }

    let mut value: u64 = 0;
    for ch in digits.chars() {
        let digit = ch.to_digit(16).ok_or_else(|| RangeError::InvalidHex {
            item: item.to_owned(),
        })?;
        value = value.saturating_mul(16).saturating_add(u64::from(digit));
        if value > u64::from(MAX_CODE_POINT) {
            return Err(RangeError::OutOfRange {
                item: item.to_owned(),
                value,
            });
        }
    }

    Ok(value as u32)
}

fn merge(mut ranges: Vec<CodePointRange>) -> Vec<CodePointRange> {
    ranges.sort_unstable_by_key(|range| range.start);

    let mut merged: Vec<CodePointRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end.saturating_add(1) => {
                last.end = last.end.max(range.end);
            }
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(spec: &str) -> Vec<(u32, u32)> {
        parse_specs(&[spec.to_owned()])
            .expect("spec should parse")
            .into_iter()
            .map(|range| (range.start, range.end))
            .collect()
    }

    fn error(spec: &str) -> RangeError {
        parse_specs(&[spec.to_owned()]).expect_err("spec should be rejected")
    }

    #[test]
    fn parses_the_documented_example() {
        assert_eq!(
            parse("0xAAAA-0xBBBB, 0xCCCC-0xCCCD"),
            vec![(0xAAAA, 0xBBBB), (0xCCCC, 0xCCCD)]
        );
    }

    #[test]
    fn parses_a_single_point() {
        assert_eq!(parse("0x41"), vec![(0x41, 0x41)]);
    }

    #[test]
    fn accepts_lowercase_and_uppercase_hex() {
        assert_eq!(parse("0x4a-0X4B"), vec![(0x4A, 0x4B)]);
    }

    #[test]
    fn ignores_whitespace_around_tokens() {
        assert_eq!(
            parse("  0x41 - 0x5A ,  0x61  "),
            vec![(0x41, 0x5A), (0x61, 0x61)]
        );
    }

    #[test]
    fn ignores_leading_zeros() {
        assert_eq!(parse("0x0041"), vec![(0x41, 0x41)]);
        assert_eq!(parse("0x0000-0x0001"), vec![(0, 1)]);
    }

    #[test]
    fn merges_overlapping_ranges() {
        assert_eq!(parse("0x41-0x5A, 0x50-0x60"), vec![(0x41, 0x60)]);
    }

    #[test]
    fn merges_adjacent_ranges() {
        assert_eq!(parse("0x41-0x5A, 0x5B-0x60"), vec![(0x41, 0x60)]);
    }

    #[test]
    fn keeps_disjoint_ranges_sorted() {
        assert_eq!(
            parse("0x100, 0x41-0x5A"),
            vec![(0x41, 0x5A), (0x100, 0x100)]
        );
    }

    #[test]
    fn unions_multiple_specifications() {
        let ranges = parse_specs(&["0x41-0x42".to_owned(), "0x42-0x43".to_owned()]).expect("valid");
        assert_eq!(
            ranges,
            vec![CodePointRange {
                start: 0x41,
                end: 0x43
            }]
        );
        assert_eq!(ranges[0].count(), 3);
    }

    #[test]
    fn rejects_reversed_ranges() {
        assert_eq!(
            error("0x5A-0x41").to_string(),
            "invalid range \"0x5A-0x41\": start 0x5A is greater than end 0x41"
        );
    }

    #[test]
    fn rejects_values_above_unicode() {
        assert_eq!(
            error("0x110000").to_string(),
            "invalid range \"0x110000\": 0x110000 is outside Unicode (max 0x10FFFF)"
        );
        assert!(error("0xFFFFFFFFFFFFFFFF")
            .to_string()
            .contains("outside Unicode"));
    }

    #[test]
    fn rejects_surrogate_ranges() {
        assert_eq!(
            error("0xD800-0xDFFF").to_string(),
            "invalid range \"0xD800-0xDFFF\": 0xD800..0xDFFF is a surrogate range"
        );
        assert!(error("0xD7FF-0xE000").to_string().contains("surrogate"));
        assert!(error("0xD800").to_string().contains("surrogate"));
    }

    #[test]
    fn rejects_empty_specifications() {
        assert_eq!(error(""), RangeError::Empty);
        assert_eq!(error("   "), RangeError::Empty);
        assert_eq!(error(","), RangeError::EmptyItem);
        assert_eq!(error("0x41,"), RangeError::EmptyItem);
        assert_eq!(error(",0x41"), RangeError::EmptyItem);
        assert_eq!(error("0x41,,0x42"), RangeError::EmptyItem);
    }

    #[test]
    fn rejects_missing_prefix() {
        assert!(matches!(error("41"), RangeError::MissingPrefix { .. }));
        assert!(matches!(error("U+0041"), RangeError::MissingPrefix { .. }));
        assert!(matches!(error("0x41-5A"), RangeError::MissingPrefix { .. }));
    }

    #[test]
    fn rejects_bad_digits() {
        assert!(matches!(error("0xZZ"), RangeError::InvalidHex { .. }));
        assert!(matches!(error("0x"), RangeError::InvalidHex { .. }));
        assert!(matches!(error("0x4G"), RangeError::InvalidHex { .. }));
        assert!(matches!(error("0x-5"), RangeError::InvalidHex { .. }));
    }

    #[test]
    fn rejects_extra_dashes() {
        assert!(matches!(
            error("0x41-0x5A-0x60"),
            RangeError::TooManyDashes { .. }
        ));
    }
}
