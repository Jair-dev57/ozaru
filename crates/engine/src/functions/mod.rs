//! Functions: how a PDF maps numbers to numbers, such as the tint of a spot
//! ink to the color that shows it, or the place in a shading to its color
//! (ISO 32000-2, section 7.10).
//!
//! [`read_function`] reads one of the four types, and
//! [`Function::evaluate`] computes it. Reading checks what evaluating needs,
//! so evaluating never fails: an error of a calculator program gives the
//! lowest values of the function's range.

mod calculator;
mod sampled;

pub use calculator::Calculator;
pub use sampled::Sampled;

use std::sync::Arc;

use crate::base::{Error, ErrorKind, Limits, Result};
use crate::syntax::{Dictionary, Object};
use crate::xref::Resolver;

/// A function of `m` inputs and `n` outputs.
#[derive(Debug, Clone, PartialEq)]
pub enum Function {
    /// Type 0: a table of samples, interpolated.
    Sampled(Sampled),
    /// Type 2: `C0 + x^N × (C1 − C0)`, of one input.
    Exponential(Exponential),
    /// Type 3: one function for each part of the domain of one input.
    Stitching(Stitching),
    /// Type 4: a program of the PostScript calculator.
    Calculator(Calculator),
}

/// Type 2, exponential interpolation (section 7.10.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Exponential {
    domain: [f64; 2],
    range: Option<Vec<f64>>,
    c0: Vec<f64>,
    c1: Vec<f64>,
    exponent: f64,
}

/// Type 3, stitching (section 7.10.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Stitching {
    domain: [f64; 2],
    range: Option<Vec<f64>>,
    functions: Vec<Arc<Function>>,
    bounds: Vec<f64>,
    encode: Vec<f64>,
}

// How deep stitching functions can hold each other.
const MAX_DEPTH: usize = 8;

/// Reads the function `entry`, a dictionary or a stream, or a reference to
/// one (section 7.10).
///
/// It is an `InvalidFunction` error if the function is of an unknown type,
/// lacks an entry its type needs, or its entries don't agree, such as a
/// table of samples of a size its data doesn't say, and a `LimitExceeded`
/// error if its table of samples is larger than `limits` allow.
pub fn read_function(
    resolver: &mut Resolver<'_>,
    entry: &Object,
    limits: Limits,
) -> Result<Function> {
    read(resolver, entry, limits, 0)
}

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidFunction, 0)
}

fn read(
    resolver: &mut Resolver<'_>,
    entry: &Object,
    limits: Limits,
    depth: usize,
) -> Result<Function> {
    if depth > MAX_DEPTH {
        return Err(invalid());
    }
    let (object, id) = match entry {
        Object::Reference(id) => (resolver.resolve(*id)?, Some(*id)),
        other => (Arc::new(other.clone()), None),
    };
    let dictionary = match object.as_ref() {
        Object::Dictionary(dictionary) => dictionary,
        Object::Stream(stream) => stream.dictionary(),
        _ => return Err(invalid()),
    };
    let numbers = |key: &str| numbers(dictionary, key);
    let domain = numbers("Domain").ok_or_else(invalid)?;
    if domain.is_empty() || domain.len() % 2 != 0 {
        return Err(invalid());
    }
    let range = numbers("Range").filter(|range| !range.is_empty() && range.len() % 2 == 0);
    let kind = dictionary.get("FunctionType").and_then(Object::as_integer);

    match kind {
        Some(0) => {
            let id = id
                .filter(|_| object.as_stream().is_some())
                .ok_or_else(invalid)?;
            let data = resolver.stream_data(id)?.data;
            Ok(Function::Sampled(Sampled::new(
                dictionary,
                &domain,
                range.ok_or_else(invalid)?,
                &data,
                limits,
            )?))
        }
        Some(2) => {
            let [low, high] = domain[..] else {
                return Err(invalid());
            };
            let c0 = numbers("C0").unwrap_or_else(|| vec![0.0]);
            let c1 = numbers("C1").unwrap_or_else(|| vec![1.0]);
            let exponent = dictionary
                .get("N")
                .and_then(Object::as_number)
                .ok_or_else(invalid)?;
            if c0.len() != c1.len() || c0.is_empty() {
                return Err(invalid());
            }
            Ok(Function::Exponential(Exponential {
                domain: [low, high],
                range,
                c0,
                c1,
                exponent,
            }))
        }
        Some(3) => {
            let [low, high] = domain[..] else {
                return Err(invalid());
            };
            let entries = match dictionary.get("Functions") {
                Some(Object::Reference(id)) => resolver.resolve(*id)?.as_ref().clone(),
                Some(other) => other.clone(),
                None => return Err(invalid()),
            };
            let entries = entries.as_array().ok_or_else(invalid)?.to_vec();
            if entries.is_empty() {
                return Err(invalid());
            }
            let mut functions = Vec::with_capacity(entries.len());
            for entry in &entries {
                functions.push(Arc::new(read(resolver, entry, limits, depth + 1)?));
            }
            let outputs = functions[0].outputs();
            if functions
                .iter()
                .any(|f| f.inputs() != 1 || f.outputs() != outputs)
            {
                return Err(invalid());
            }
            let bounds = numbers("Bounds").unwrap_or_default();
            let encode = numbers("Encode").ok_or_else(invalid)?;
            if bounds.len() + 1 != functions.len() || encode.len() != 2 * functions.len() {
                return Err(invalid());
            }
            Ok(Function::Stitching(Stitching {
                domain: [low, high],
                range,
                functions,
                bounds,
                encode,
            }))
        }
        Some(4) => {
            let id = id
                .filter(|_| object.as_stream().is_some())
                .ok_or_else(invalid)?;
            let program = resolver.stream_data(id)?.data;
            Ok(Function::Calculator(Calculator::new(
                &program,
                domain,
                range.ok_or_else(invalid)?,
            )?))
        }
        _ => Err(invalid()),
    }
}

// The array `key` of `dictionary`, if it is one of numbers.
fn numbers(dictionary: &Dictionary, key: &str) -> Option<Vec<f64>> {
    dictionary
        .get(key)?
        .as_array()?
        .iter()
        .map(|item| item.as_number().filter(|n| n.is_finite()))
        .collect()
}

/// `x` mapped from `[x0, x1]` to `[y0, y1]` (section 7.10.2).
pub(crate) fn interpolate(x: f64, x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
    if x1 == x0 {
        y0
    } else {
        y0 + (x - x0) * (y1 - y0) / (x1 - x0)
    }
}

// `values` clipped to `range`, pairs of the lowest and highest of each.
fn clip(values: &mut [f64], range: Option<&[f64]>) {
    if let Some(range) = range {
        for (value, pair) in values.iter_mut().zip(range.chunks(2)) {
            *value = value.clamp(pair[0].min(pair[1]), pair[1].max(pair[0]));
        }
    }
}

impl Function {
    /// How many inputs it takes.
    pub fn inputs(&self) -> usize {
        match self {
            Self::Sampled(f) => f.inputs(),
            Self::Exponential(_) | Self::Stitching(_) => 1,
            Self::Calculator(f) => f.inputs(),
        }
    }

    /// How many outputs it gives.
    pub fn outputs(&self) -> usize {
        match self {
            Self::Sampled(f) => f.outputs(),
            Self::Exponential(f) => f.c0.len(),
            Self::Stitching(f) => f.functions[0].outputs(),
            Self::Calculator(f) => f.outputs(),
        }
    }

    /// The outputs for `input`, clipped to the domain first and to the range
    /// after. Missing inputs are 0, extra ones are ignored.
    pub fn evaluate(&self, input: &[f64]) -> Vec<f64> {
        let mut input: Vec<f64> = (0..self.inputs())
            .map(|i| {
                input
                    .get(i)
                    .copied()
                    .filter(|x| x.is_finite())
                    .unwrap_or(0.0)
            })
            .collect();
        match self {
            Self::Sampled(f) => f.evaluate(&mut input),
            Self::Calculator(f) => f.evaluate(&mut input),
            Self::Exponential(f) => {
                let x = input[0].clamp(f.domain[0].min(f.domain[1]), f.domain[1].max(f.domain[0]));
                let power = x.powf(f.exponent);
                let power = if power.is_finite() { power } else { 0.0 };
                let mut out: Vec<f64> =
                    f.c0.iter()
                        .zip(&f.c1)
                        .map(|(c0, c1)| c0 + power * (c1 - c0))
                        .collect();
                clip(&mut out, f.range.as_deref());
                out
            }
            Self::Stitching(f) => {
                let (low, high) = (f.domain[0], f.domain[1]);
                let x = input[0].clamp(low.min(high), high.max(low));
                // The part of the domain `x` is in: the first whose upper
                // bound is above it, or the last.
                let k = f.bounds.iter().take_while(|&&bound| x >= bound).count();
                let k = k.min(f.functions.len() - 1);
                let start = if k == 0 { low } else { f.bounds[k - 1] };
                let end = f.bounds.get(k).copied().unwrap_or(high);
                let encoded = interpolate(x, start, end, f.encode[2 * k], f.encode[2 * k + 1]);
                let mut out = f.functions[k].evaluate(&[encoded]);
                clip(&mut out, f.range.as_deref());
                out
            }
        }
    }
}
