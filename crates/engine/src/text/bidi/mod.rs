//! The Unicode Bidirectional Algorithm (UAX #9), for putting lines of Arabic and
//! Hebrew painted left to right back in reading order.
//!
//! [`bidi_levels`] resolves the embedding level of each character of a paragraph,
//! through rule I2: explicit embeddings, overrides and isolates, the weak types,
//! paired brackets and the neutrals. Rule L1 and the reordering of L2 are left to
//! the caller, since a line is not always a whole paragraph. It is checked against
//! `BidiCharacterTest.txt`, the conformance test of the Unicode Consortium.

mod tables;

use tables::{BRACKETS, CLASSES};

/// The Bidi_Class of a character (UAX #44, table 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiClass {
    /// Left to right.
    L,
    /// Right to left.
    R,
    /// Arabic letter.
    AL,
    /// European number.
    EN,
    /// European separator.
    ES,
    /// European terminator.
    ET,
    /// Arabic number.
    AN,
    /// Common separator.
    CS,
    /// Nonspacing mark.
    NSM,
    /// Boundary neutral.
    BN,
    /// Paragraph separator.
    B,
    /// Segment separator.
    S,
    /// White space.
    WS,
    /// Other neutral.
    ON,
    /// Left-to-right embedding.
    LRE,
    /// Left-to-right override.
    LRO,
    /// Right-to-left embedding.
    RLE,
    /// Right-to-left override.
    RLO,
    /// Pop directional format.
    PDF,
    /// Left-to-right isolate.
    LRI,
    /// Right-to-left isolate.
    RLI,
    /// First strong isolate.
    FSI,
    /// Pop directional isolate.
    PDI,
}

use BidiClass::*;

/// The Bidi_Class of `c`.
pub fn bidi_class(c: char) -> BidiClass {
    let code = c as u32;
    let at = CLASSES.partition_point(|&(_, last, _)| last < code);
    match CLASSES.get(at) {
        Some(&(first, _, class)) if first <= code => class,
        _ => L,
    }
}

// The deepest explicit embedding (BD2).
const MAX_DEPTH: u8 = 125;
// The most brackets waiting for their pair (BD16).
const MAX_BRACKETS: usize = 63;

fn is_initiator(class: BidiClass) -> bool {
    matches!(class, LRI | RLI | FSI)
}

fn is_removed(class: BidiClass) -> bool {
    matches!(class, RLE | LRE | RLO | LRO | PDF | BN)
}

// Neutral and isolate formatting types (N1).
fn is_neutral(class: BidiClass) -> bool {
    matches!(class, B | S | WS | ON | FSI | LRI | RLI | PDI)
}

// BD9: the PDI that closes each isolate initiator, and whether each PDI closes one.
fn match_isolates(classes: &[BidiClass]) -> (Vec<Option<usize>>, Vec<bool>) {
    let mut closing = vec![None; classes.len()];
    let mut closes = vec![false; classes.len()];
    let mut open: Vec<usize> = Vec::new();
    for (i, &class) in classes.iter().enumerate() {
        if is_initiator(class) {
            open.push(i);
        } else if class == PDI
            && let Some(initiator) = open.pop()
        {
            closing[initiator] = Some(i);
            closes[i] = true;
        }
    }
    (closing, closes)
}

// P2 and P3: the level of the first strong character in `from..to`, skipping
// the text of isolates; `None` if there isn't one.
fn first_strong(
    classes: &[BidiClass],
    closing: &[Option<usize>],
    from: usize,
    to: usize,
) -> Option<u8> {
    let mut i = from;
    while i < to {
        match classes[i] {
            L => return Some(0),
            R | AL => return Some(1),
            LRI | RLI | FSI => i = closing[i]?,
            _ => {}
        }
        i += 1;
    }
    None
}

/// The paragraph embedding level of `text`: 0 or 1 by its first strong character
/// (rules P2 and P3), 0 if it has none.
pub fn paragraph_level(text: &str) -> u8 {
    let classes: Vec<BidiClass> = text.chars().map(bidi_class).collect();
    let (closing, _) = match_isolates(&classes);
    first_strong(&classes, &closing, 0, classes.len()).unwrap_or(0)
}

#[derive(Clone, Copy)]
struct Status {
    level: u8,
    // Whether everything is made left to right (L) or right to left (R).
    force: Option<BidiClass>,
    isolate: bool,
}

/// The embedding level of each character of `text`, a paragraph, after rule I2.
///
/// `paragraph` is its level: 0 or 1, or `None` to take it from the first strong
/// character. The levels of characters the algorithm removes (explicit
/// formatting characters and boundary neutrals) are those of the character
/// before them.
pub fn bidi_levels(text: &str, paragraph: Option<u8>) -> Vec<u8> {
    let chars: Vec<char> = text.chars().collect();
    let original: Vec<BidiClass> = chars.iter().map(|&c| bidi_class(c)).collect();
    let n = chars.len();
    let (closing, closes) = match_isolates(&original);
    let paragraph = match paragraph {
        Some(level) => level & 1,
        None => first_strong(&original, &closing, 0, n).unwrap_or(0),
    };

    // X1 to X8: explicit levels and overrides.
    let mut types = original.clone();
    let mut levels = vec![paragraph; n];
    let mut stack = vec![Status {
        level: paragraph,
        force: None,
        isolate: false,
    }];
    let (mut overflow_isolates, mut overflow_embeddings, mut valid_isolates) = (0usize, 0usize, 0);
    for i in 0..n {
        let top = *stack.last().expect("the stack is never empty");
        match original[i] {
            RLE | LRE | RLO | LRO => {
                levels[i] = top.level;
                let next = if matches!(original[i], RLE | RLO) {
                    (top.level + 1) | 1
                } else {
                    (top.level + 2) & !1
                };
                if next <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                    stack.push(Status {
                        level: next,
                        force: match original[i] {
                            RLO => Some(R),
                            LRO => Some(L),
                            _ => None,
                        },
                        isolate: false,
                    });
                } else if overflow_isolates == 0 {
                    overflow_embeddings += 1;
                }
            }
            RLI | LRI | FSI => {
                levels[i] = top.level;
                if let Some(force) = top.force {
                    types[i] = force;
                }
                let right_to_left = match original[i] {
                    RLI => true,
                    LRI => false,
                    _ => {
                        first_strong(&original, &closing, i + 1, closing[i].unwrap_or(n)) == Some(1)
                    }
                };
                let next = if right_to_left {
                    (top.level + 1) | 1
                } else {
                    (top.level + 2) & !1
                };
                if next <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                    valid_isolates += 1;
                    stack.push(Status {
                        level: next,
                        force: None,
                        isolate: true,
                    });
                } else {
                    overflow_isolates += 1;
                }
            }
            PDI => {
                if overflow_isolates > 0 {
                    overflow_isolates -= 1;
                } else if valid_isolates > 0 {
                    overflow_embeddings = 0;
                    while stack.last().is_some_and(|s| !s.isolate) {
                        stack.pop();
                    }
                    stack.pop();
                    valid_isolates -= 1;
                }
                let top = *stack.last().expect("the stack is never empty");
                levels[i] = top.level;
                if let Some(force) = top.force {
                    types[i] = force;
                }
            }
            PDF => {
                levels[i] = top.level;
                if overflow_isolates > 0 {
                } else if overflow_embeddings > 0 {
                    overflow_embeddings -= 1;
                } else if !top.isolate && stack.len() >= 2 {
                    stack.pop();
                }
            }
            B => levels[i] = paragraph,
            BN => levels[i] = top.level,
            _ => {
                levels[i] = top.level;
                if let Some(force) = top.force {
                    types[i] = force;
                }
            }
        }
    }

    // X9: what is left, and X10: its isolating run sequences.
    let kept: Vec<usize> = (0..n).filter(|&i| !is_removed(original[i])).collect();
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for &i in &kept {
        match runs.last_mut() {
            Some(run) if levels[run[0]] == levels[i] => run.push(i),
            _ => runs.push(vec![i]),
        }
    }
    let mut run_of = vec![usize::MAX; n];
    for (id, run) in runs.iter().enumerate() {
        for &i in run {
            run_of[i] = id;
        }
    }
    // The position of each kept character among the kept ones.
    let mut position = vec![usize::MAX; n];
    for (at, &i) in kept.iter().enumerate() {
        position[i] = at;
    }
    let mut result = levels.clone();
    for run in &runs {
        if original[run[0]] == PDI && closes[run[0]] {
            continue;
        }
        let mut sequence = run.clone();
        while let Some(&last) = sequence.last() {
            let next = match (is_initiator(original[last]), closing[last]) {
                (true, Some(pdi)) if run_of[pdi] != usize::MAX && run_of[pdi] != run_of[last] => {
                    run_of[pdi]
                }
                _ => break,
            };
            sequence.extend_from_slice(&runs[next]);
        }
        let level = levels[sequence[0]];
        let before = position[sequence[0]]
            .checked_sub(1)
            .map_or(paragraph, |p| levels[kept[p]]);
        let last = *sequence.last().expect("a sequence has a character");
        let after = if is_initiator(original[last]) && closing[last].is_none() {
            paragraph
        } else {
            kept.get(position[last] + 1)
                .map_or(paragraph, |&i| levels[i])
        };
        let sos = if level.max(before) % 2 == 1 { R } else { L };
        let eos = if level.max(after) % 2 == 1 { R } else { L };
        let mut sequence_types: Vec<BidiClass> = sequence.iter().map(|&i| types[i]).collect();
        resolve_sequence(
            &mut sequence_types,
            &sequence,
            &chars,
            &original,
            level,
            sos,
            eos,
        );
        // I1 and I2.
        for (&i, &class) in sequence.iter().zip(&sequence_types) {
            result[i] = if level % 2 == 0 {
                match class {
                    R => level + 1,
                    AN | EN => level + 2,
                    _ => level,
                }
            } else {
                match class {
                    L | EN | AN => level + 1,
                    _ => level,
                }
            };
        }
    }
    // The characters removed by X9 take the level of the one before.
    for i in 0..n {
        if is_removed(original[i]) {
            result[i] = if i == 0 { paragraph } else { result[i - 1] };
        }
    }
    result
}

// A direction as a strong type: numbers count as right to left (N0 and N1).
fn strong(class: BidiClass) -> Option<BidiClass> {
    match class {
        L => Some(L),
        R | EN | AN => Some(R),
        _ => None,
    }
}

// W1 to W7, N0 to N2 on one isolating run sequence.
fn resolve_sequence(
    types: &mut [BidiClass],
    indexes: &[usize],
    chars: &[char],
    original: &[BidiClass],
    level: u8,
    sos: BidiClass,
    eos: BidiClass,
) {
    let len = types.len();
    // W1: a nonspacing mark takes the type of the character before it.
    for k in 0..len {
        if types[k] == NSM {
            types[k] = if k == 0 {
                sos
            } else if matches!(types[k - 1], LRI | RLI | FSI | PDI) {
                ON
            } else {
                types[k - 1]
            };
        }
    }
    // W2: a European number after an Arabic letter is an Arabic number.
    for k in 0..len {
        if types[k] == EN {
            let before = types[..k]
                .iter()
                .rev()
                .find(|&&t| matches!(t, L | R | AL))
                .copied()
                .unwrap_or(sos);
            if before == AL {
                types[k] = AN;
            }
        }
    }
    // W3.
    for t in types.iter_mut() {
        if *t == AL {
            *t = R;
        }
    }
    // W4: a separator between two numbers of one type joins them.
    for k in 1..len.saturating_sub(1) {
        match (types[k - 1], types[k], types[k + 1]) {
            (EN, ES | CS, EN) => types[k] = EN,
            (AN, CS, AN) => types[k] = AN,
            _ => {}
        }
    }
    // W5: terminators next to a European number are European numbers.
    let mut k = 0;
    while k < len {
        if types[k] != ET {
            k += 1;
            continue;
        }
        let start = k;
        while k < len && types[k] == ET {
            k += 1;
        }
        let next_to_number = (start > 0 && types[start - 1] == EN) || (k < len && types[k] == EN);
        if next_to_number {
            types[start..k].fill(EN);
        }
    }
    // W6.
    for t in types.iter_mut() {
        if matches!(*t, ES | ET | CS) {
            *t = ON;
        }
    }
    // W7: a European number after a left-to-right letter is left to right.
    for k in 0..len {
        if types[k] == EN {
            let before = types[..k]
                .iter()
                .rev()
                .find(|&&t| matches!(t, L | R))
                .copied()
                .unwrap_or(sos);
            if before == L {
                types[k] = L;
            }
        }
    }

    let embedding = if level % 2 == 1 { R } else { L };
    brackets(types, indexes, chars, original, embedding, sos);

    // N1 and N2: runs of neutrals take the direction around them if both sides
    // agree, and the embedding direction if not.
    let mut k = 0;
    while k < len {
        if !is_neutral(types[k]) {
            k += 1;
            continue;
        }
        let start = k;
        while k < len && is_neutral(types[k]) {
            k += 1;
        }
        let before = if start == 0 {
            Some(sos)
        } else {
            strong(types[start - 1])
        };
        let after = if k == len {
            Some(eos)
        } else {
            strong(types[k])
        };
        let direction = match (before, after) {
            (Some(a), Some(b)) if a == b => a,
            _ => embedding,
        };
        types[start..k].fill(direction);
    }
}

// N0: brackets that pair take the direction of what is inside them, or of what
// is before them (BD14 to BD16).
fn brackets(
    types: &mut [BidiClass],
    indexes: &[usize],
    chars: &[char],
    original: &[BidiClass],
    embedding: BidiClass,
    sos: BidiClass,
) {
    let len = types.len();
    // The canonical equivalents of the angle brackets are the same bracket.
    let canonical = |c: u32| match c {
        0x2329 => 0x3008,
        0x232A => 0x3009,
        other => other,
    };
    let mut open: Vec<(u32, usize)> = Vec::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for k in 0..len {
        if types[k] != ON {
            continue;
        }
        let code = chars[indexes[k]] as u32;
        let Some(&(_, pair, opens)) = BRACKETS.iter().find(|&&(bracket, _, _)| bracket == code)
        else {
            continue;
        };
        if opens {
            if open.len() == MAX_BRACKETS {
                break;
            }
            open.push((canonical(pair), k));
        } else if let Some(found) = open
            .iter()
            .rposition(|&(closing, _)| closing == canonical(code))
        {
            pairs.push((open[found].1, k));
            open.truncate(found);
        }
    }
    pairs.sort_unstable();
    let opposite = if embedding == L { R } else { L };
    for (opening, closing) in pairs {
        let mut found_embedding = false;
        let mut found_opposite = false;
        for &t in &types[opening + 1..closing] {
            match strong(t) {
                Some(d) if d == embedding => found_embedding = true,
                Some(_) => found_opposite = true,
                None => {}
            }
        }
        let direction = if found_embedding {
            embedding
        } else if found_opposite {
            let context = types[..opening]
                .iter()
                .rev()
                .find_map(|&t| strong(t))
                .unwrap_or(sos);
            if context == opposite {
                opposite
            } else {
                embedding
            }
        } else {
            continue;
        };
        for bracket in [opening, closing] {
            types[bracket] = direction;
            // Marks that followed the bracket keep to it.
            let mut next = bracket + 1;
            while next < len && original[indexes[next]] == NSM {
                types[next] = direction;
                next += 1;
            }
        }
    }
}
