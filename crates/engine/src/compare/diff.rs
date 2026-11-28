//! The differences between two sequences: Myers' greedy algorithm ("An O(ND)
//! Difference Algorithm and Its Variations", 1986), with the common ends cut
//! off first.

/// A run of elements that differ: `a[a_start..a_end]` of the first sequence
/// was replaced by `b[b_start..b_end]` of the second. Either side may be empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunk {
    pub a_start: usize,
    pub a_end: usize,
    pub b_start: usize,
    pub b_end: usize,
}

/// The most edits looked for; sequences that differ more are one hunk.
const MAX_EDITS: usize = 1500;

/// The hunks that turn `a` into `b`, in order.
pub fn hunks<T: PartialEq>(a: &[T], b: &[T]) -> Vec<Hunk> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a_end, b_end) = (a.len() - suffix, b.len() - suffix);
    let (a_mid, b_mid) = (&a[prefix..a_end], &b[prefix..b_end]);
    if a_mid.is_empty() && b_mid.is_empty() {
        return Vec::new();
    }
    let whole = Hunk {
        a_start: prefix,
        a_end,
        b_start: prefix,
        b_end,
    };
    if a_mid.is_empty() || b_mid.is_empty() {
        return vec![whole];
    }
    let Some(matches) = matches(a_mid, b_mid) else {
        return vec![whole];
    };
    // The gaps between the elements that match are the hunks.
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (x, y) in matches.into_iter().chain([(a_mid.len(), b_mid.len())]) {
        if x > i || y > j {
            result.push(Hunk {
                a_start: prefix + i,
                a_end: prefix + x,
                b_start: prefix + j,
                b_end: prefix + y,
            });
        }
        i = x + 1;
        j = y + 1;
    }
    result
}

// The pairs of positions `(i, j)` of equal elements on the shortest edit
// script, in order; `None` if it takes more than `MAX_EDITS`.
fn matches<T: PartialEq>(a: &[T], b: &[T]) -> Option<Vec<(usize, usize)>> {
    let n = a.len() as isize;
    let m = b.len() as isize;
    let max = (n + m).min(MAX_EDITS as isize);
    let offset = max + 1;
    let mut v = vec![0_i32; (2 * max + 3) as usize];
    let mut trace: Vec<Vec<i32>> = Vec::new();
    let mut found = None;
    'search: for d in 0..=max {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let index = |k: isize| (offset + k) as usize;
            let mut x = if k == -d || (k != d && v[index(k - 1)] < v[index(k + 1)]) {
                v[index(k + 1)] as isize
            } else {
                v[index(k - 1)] as isize + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[index(k)] = x as i32;
            if x >= n && y >= m {
                found = Some(d);
                break 'search;
            }
            k += 2;
        }
    }
    let last = found?;
    let mut pairs = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..=last).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let index = |k: isize| (offset + k) as usize;
        let previous = if k == -d || (k != d && v[index(k - 1)] < v[index(k + 1)]) {
            k + 1
        } else {
            k - 1
        };
        let previous_x = v[index(previous)] as isize;
        let previous_y = previous_x - previous;
        while x > previous_x && y > previous_y {
            pairs.push(((x - 1) as usize, (y - 1) as usize));
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            x = previous_x;
            y = previous_y;
        }
    }
    pairs.reverse();
    Some(pairs)
}
