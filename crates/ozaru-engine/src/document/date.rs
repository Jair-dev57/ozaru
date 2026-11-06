/// A date, as written in PDF strings such as `D:20240131143000+01'00'`
/// (ISO 32000-2, section 7.9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: u16,
    /// 1 to 12. 1 when the date doesn't say.
    pub month: u8,
    /// 1 to 31. 1 when the date doesn't say.
    pub day: u8,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
    /// 0 to 59.
    pub second: u8,
    /// The difference from UTC, in minutes: 60 for `+01'00'`, 0 for `Z`. `None`
    /// when the date doesn't say, so the time zone is unknown.
    pub utc_offset: Option<i16>,
}

/// Parses a date string, `D:YYYYMMDDHHmmSSOHH'mm'`, where everything after the
/// year is optional.
///
/// Common variations are accepted: no `D:` prefix, no apostrophes in the time
/// zone or a trailing one, a time zone with hours only, and whitespace around.
/// It is `None` if the date is impossible, such as month 13, or isn't a date.
pub fn parse_date(text: &str) -> Option<Date> {
    let text = text.trim();
    let text = text.strip_prefix("D:").unwrap_or(text);
    let bytes = text.as_bytes();
    let mut position = 0;
    let mut field = |length: usize| -> Option<Option<u16>> {
        let digits = bytes.get(position..position + length);
        match digits {
            Some(digits) if digits.iter().all(u8::is_ascii_digit) => {
                position += length;
                Some(Some(digits.iter().fold(0, |value, &digit| {
                    value * 10 + u16::from(digit - b'0')
                })))
            }
            // A field that isn't there ends the date, but a partial one is wrong.
            _ if bytes
                .get(position)
                .is_none_or(|byte| !byte.is_ascii_digit()) =>
            {
                Some(None)
            }
            _ => None,
        }
    };

    let year = field(4)??;
    let mut parts = [1, 1, 0, 0, 0];
    for part in &mut parts {
        match field(2)? {
            Some(value) => *part = value,
            None => break,
        }
    }
    let [month, day, hour, minute, second] = parts.map(|part| part as u8);

    let utc_offset = match bytes.get(position) {
        None => None,
        Some(b'Z') => {
            position += 1;
            zone_tail(bytes, &mut position)?;
            Some(0)
        }
        Some(&sign @ (b'+' | b'-')) => {
            position += 1;
            let hours = two_digits(bytes, &mut position)?;
            skip_apostrophe(bytes, &mut position);
            let minutes = match two_digits(bytes, &mut position) {
                Some(minutes) => {
                    skip_apostrophe(bytes, &mut position);
                    minutes
                }
                None => 0,
            };
            if hours > 23 || minutes > 59 {
                return None;
            }
            let offset = i16::from(hours) * 60 + i16::from(minutes);
            Some(if sign == b'-' { -offset } else { offset })
        }
        Some(_) => return None,
    };
    if position != bytes.len() {
        return None;
    }

    let valid = (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 59;
    valid.then_some(Date {
        year,
        month,
        day,
        hour,
        minute,
        second,
        utc_offset,
    })
}

// After `Z`, some writers add `00'00'` anyway. It is skipped if it is zero.
fn zone_tail(bytes: &[u8], position: &mut usize) -> Option<()> {
    if bytes.get(*position).is_none() {
        return Some(());
    }
    let hours = two_digits(bytes, position)?;
    skip_apostrophe(bytes, position);
    let minutes = two_digits(bytes, position).unwrap_or(0);
    skip_apostrophe(bytes, position);
    (hours == 0 && minutes == 0).then_some(())
}

fn two_digits(bytes: &[u8], position: &mut usize) -> Option<u8> {
    let digits = bytes.get(*position..*position + 2)?;
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    *position += 2;
    Some((digits[0] - b'0') * 10 + (digits[1] - b'0'))
}

fn skip_apostrophe(bytes: &[u8], position: &mut usize) {
    if bytes.get(*position) == Some(&b'\'') {
        *position += 1;
    }
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
