use super::{clip, interpolate, invalid, numbers};
use crate::base::{Error, ErrorKind, Limit, Limits, Result};
use crate::syntax::{Dictionary, Object};

/// Type 0, a table of samples (ISO 32000-2, section 7.10.2): each input
/// picks a place in the table, and the outputs are interpolated between the
/// samples around it, linearly in each input.
#[derive(Debug, Clone, PartialEq)]
pub struct Sampled {
    domain: Vec<f64>,
    range: Vec<f64>,
    size: Vec<usize>,
    encode: Vec<f64>,
    decode: Vec<f64>,
    bits: u32,
    // The samples, as raw values, the first input varying fastest.
    samples: Vec<u32>,
}

// Inputs a table can have: interpolating takes 2^m samples.
const MAX_INPUTS: usize = 8;

impl Sampled {
    pub(super) fn new(
        dictionary: &Dictionary,
        domain: &[f64],
        range: Vec<f64>,
        data: &[u8],
        limits: Limits,
    ) -> Result<Self> {
        let inputs = domain.len() / 2;
        let outputs = range.len() / 2;
        if inputs > MAX_INPUTS {
            return Err(invalid());
        }
        let size: Vec<usize> = dictionary
            .get("Size")
            .and_then(Object::as_array)
            .ok_or_else(invalid)?
            .iter()
            .map(|n| {
                n.as_integer()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|&n| n > 0)
            })
            .collect::<Option<_>>()
            .ok_or_else(invalid)?;
        let bits = dictionary
            .get("BitsPerSample")
            .and_then(Object::as_integer)
            .and_then(|b| u32::try_from(b).ok())
            .filter(|b| matches!(b, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32))
            .ok_or_else(invalid)?;
        if size.len() != inputs {
            return Err(invalid());
        }
        let encode = numbers(dictionary, "Encode")
            .filter(|e| e.len() == 2 * inputs)
            .unwrap_or_else(|| size.iter().flat_map(|&s| [0.0, (s - 1) as f64]).collect());
        let decode = numbers(dictionary, "Decode")
            .filter(|d| d.len() == 2 * outputs)
            .unwrap_or_else(|| range.clone());

        let count = size
            .iter()
            .try_fold(outputs, |count, &s| count.checked_mul(s))
            .filter(|&count| count <= limits.max_decoded_length)
            .ok_or(Error::new(
                ErrorKind::LimitExceeded(Limit::DecodedLength),
                0,
            ))?;
        // Samples past the end of the data are 0.
        let samples = (0..count)
            .map(|index| read_bits(data, index * bits as usize, bits))
            .collect();
        Ok(Self {
            domain: domain.to_vec(),
            range,
            size,
            encode,
            decode,
            bits,
            samples,
        })
    }

    pub(super) fn inputs(&self) -> usize {
        self.size.len()
    }

    pub(super) fn outputs(&self) -> usize {
        self.range.len() / 2
    }

    pub(super) fn evaluate(&self, input: &mut [f64]) -> Vec<f64> {
        let outputs = self.outputs();
        // The place in the table of each input, and how far it is from the
        // sample below it.
        let mut below = Vec::with_capacity(input.len());
        let mut fraction = Vec::with_capacity(input.len());
        for (i, x) in input.iter().enumerate() {
            let (d0, d1) = (self.domain[2 * i], self.domain[2 * i + 1]);
            let x = x.clamp(d0.min(d1), d1.max(d0));
            let last = (self.size[i] - 1) as f64;
            let e = interpolate(x, d0, d1, self.encode[2 * i], self.encode[2 * i + 1]);
            let e = e.clamp(0.0, last);
            let floor = e.floor().min((last - 1.0).max(0.0));
            below.push(floor as usize);
            fraction.push(e - floor);
        }

        let max = if self.bits == 32 {
            f64::from(u32::MAX)
        } else {
            f64::from((1u32 << self.bits) - 1)
        };
        let mut out = vec![0.0; outputs];
        // Each corner of the cell around the place, weighted by how near it is.
        for corner in 0..1usize << input.len() {
            let mut weight = 1.0;
            let mut index = 0;
            let mut stride = 1;
            for i in 0..input.len() {
                let up = corner >> i & 1 == 1;
                let at = (below[i] + usize::from(up)).min(self.size[i] - 1);
                weight *= if up { fraction[i] } else { 1.0 - fraction[i] };
                index += at * stride;
                stride *= self.size[i];
            }
            if weight == 0.0 {
                continue;
            }
            for (j, value) in out.iter_mut().enumerate() {
                let raw = self.samples[index * outputs + j];
                *value += weight * f64::from(raw);
            }
        }
        for (j, value) in out.iter_mut().enumerate() {
            *value = interpolate(*value, 0.0, max, self.decode[2 * j], self.decode[2 * j + 1]);
        }
        clip(&mut out, Some(&self.range));
        out
    }
}

// The `bits` bits at bit `at` of `data`, most significant first, 0 past its
// end.
fn read_bits(data: &[u8], at: usize, bits: u32) -> u32 {
    let mut value: u64 = 0;
    for bit in at..at + bits as usize {
        let byte = data.get(bit / 8).copied().unwrap_or(0);
        value = value << 1 | u64::from(byte >> (7 - bit % 8) & 1);
    }
    value as u32
}
