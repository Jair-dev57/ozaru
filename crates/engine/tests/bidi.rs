//! The Unicode Bidirectional Algorithm, against the conformance test of the Unicode
//! Consortium (`BidiCharacterTest.txt`, through rule L2). `tests/files/` has a
//! sample; set `BIDI_FULL` to the path of the whole file to check all its cases.

use ozaru_engine::text::{BidiClass, bidi_class, bidi_levels, paragraph_level};

// Rule L1: segment and paragraph separators, and whitespace before them or at the
// end of the line, go back to the paragraph level.
fn reset_whitespace(chars: &[char], levels: &mut [u8], paragraph: u8) {
    use BidiClass::*;
    let classes: Vec<BidiClass> = chars.iter().map(|&c| bidi_class(c)).collect();
    let mut trailing = true;
    for i in (0..chars.len()).rev() {
        match classes[i] {
            S | B => {
                levels[i] = paragraph;
                trailing = true;
            }
            WS | FSI | LRI | RLI | PDI | LRE | RLE | LRO | RLO | PDF | BN if trailing => {
                levels[i] = paragraph;
            }
            WS | FSI | LRI | RLI | PDI | LRE | RLE | LRO | RLO | PDF | BN => {}
            _ => trailing = false,
        }
    }
}

// Rule L2, over the characters that are not removed: their indexes in visual order.
fn visual_order(levels: &[Option<u8>]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).filter(|&i| levels[i].is_some()).collect();
    let level = |i: usize| levels[i].unwrap();
    let highest = order.iter().map(|&i| level(i)).max().unwrap_or(0);
    let lowest_odd = order.iter().map(|&i| level(i)).filter(|l| l % 2 == 1).min();
    let Some(lowest_odd) = lowest_odd else {
        return order;
    };
    for l in (lowest_odd..=highest).rev() {
        let mut start = 0;
        while start < order.len() {
            if level(order[start]) < l {
                start += 1;
                continue;
            }
            let mut end = start;
            while end < order.len() && level(order[end]) >= l {
                end += 1;
            }
            order[start..end].reverse();
            start = end;
        }
    }
    order
}

fn check(line: &str, number: usize) {
    let fields: Vec<&str> = line.trim_end().split(';').collect();
    let chars: Vec<char> = fields[0]
        .split(' ')
        .map(|code| char::from_u32(u32::from_str_radix(code, 16).unwrap()).unwrap())
        .collect();
    let text: String = chars.iter().collect();
    let direction = match fields[1] {
        "0" => Some(0),
        "1" => Some(1),
        _ => None,
    };
    let paragraph: u8 = fields[2].parse().unwrap();
    assert_eq!(
        direction.unwrap_or_else(|| paragraph_level(&text)),
        paragraph,
        "line {number}: paragraph level of {line}"
    );
    let mut levels = bidi_levels(&text, direction);
    reset_whitespace(&chars, &mut levels, paragraph);
    let expected: Vec<Option<u8>> = fields[3]
        .split(' ')
        .map(|level| {
            if level == "x" {
                None
            } else {
                Some(level.parse().unwrap())
            }
        })
        .collect();
    for (i, level) in expected.iter().enumerate() {
        if let Some(level) = level {
            assert_eq!(levels[i], *level, "line {number}, character {i}: {line}");
        }
    }
    let order: Vec<usize> = fields[4]
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(|i| i.parse().unwrap())
        .collect();
    assert_eq!(
        visual_order(&expected),
        order,
        "line {number}: order of {line}"
    );
}

#[test]
fn the_sample_of_the_conformance_test_passes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/files/bidi-character-test.txt");
    let text = std::fs::read_to_string(path).unwrap();
    let mut count = 0;
    for (number, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.starts_with('#') && !l.trim().is_empty())
    {
        check(line, number + 1);
        count += 1;
    }
    assert!(count > 2000);
}

#[test]
fn the_whole_conformance_test_passes_when_it_is_there() {
    let Ok(path) = std::env::var("BIDI_FULL") else {
        return;
    };
    let text = std::fs::read_to_string(path).unwrap();
    for (number, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.starts_with('#') && !l.trim().is_empty())
    {
        check(line, number + 1);
    }
}

#[test]
fn classes_follow_the_unicode_database() {
    use BidiClass::*;
    assert_eq!(bidi_class('a'), L);
    assert_eq!(bidi_class('\u{05D0}'), R);
    assert_eq!(bidi_class('\u{0627}'), AL);
    assert_eq!(bidi_class('1'), EN);
    assert_eq!(bidi_class('\u{0661}'), AN);
    assert_eq!(bidi_class(' '), WS);
    assert_eq!(bidi_class('('), ON);
    assert_eq!(bidi_class('\u{202E}'), RLO);
    assert_eq!(bidi_class('\u{2067}'), RLI);
    // Not assigned, in the Hebrew and Arabic blocks, which read right to left.
    assert_eq!(bidi_class('\u{05FF}'), R);
    assert_eq!(bidi_class('\u{07BF}'), AL);
}
