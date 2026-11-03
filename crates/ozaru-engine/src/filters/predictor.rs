use crate::base::{DiagnosticKind, Diagnostics, Error, ErrorKind, Result};
use crate::syntax::Dictionary;

/// The PNG predictors of `/DecodeParms` (ISO 32000-2, section 7.4.4.4). Each row
/// starts with a byte that says how it was predicted from the row above and the
/// byte to its left.
pub(super) struct Predictor {
    // Bytes per complete pixel, at least 1.
    pixel_length: usize,
    row_length: usize,
}

impl Predictor {
    /// The predictor in `parameters`, or `None` if the data isn't predicted.
    pub(super) fn from_parameters(
        parameters: Option<&Dictionary>,
        offset: usize,
    ) -> Result<Option<Self>> {
        let Some(parameters) = parameters else {
            return Ok(None);
        };
        let invalid = || Error::new(ErrorKind::InvalidDecodeParameters, offset);
        let integer = |key: &str, default: usize| match parameters.get(key) {
            None => Ok(default),
            Some(value) => value
                .as_integer()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(invalid),
        };

        match integer("Predictor", 1)? {
            1 => return Ok(None),
            // TIFF predictor 2 comes with the other filters in phase 4.
            2 => return Err(Error::new(ErrorKind::UnsupportedPredictor, offset)),
            10..=15 => {}
            _ => return Err(invalid()),
        }
        let colors = integer("Colors", 1)?;
        let bits = integer("BitsPerComponent", 8)?;
        let columns = integer("Columns", 1)?;
        if colors == 0 || columns == 0 || !matches!(bits, 1 | 2 | 4 | 8 | 16) {
            return Err(invalid());
        }

        let pixel_bits = colors.checked_mul(bits).ok_or_else(invalid)?;
        let row_bits = pixel_bits.checked_mul(columns).ok_or_else(invalid)?;
        Ok(Some(Self {
            pixel_length: pixel_bits.div_ceil(8),
            row_length: row_bits.div_ceil(8),
        }))
    }

    /// Undoes the prediction. Predicted data is one byte per row longer than the
    /// result, so this can't expand it.
    pub(super) fn undo(
        &self,
        data: &[u8],
        offset: usize,
        diagnostics: &mut Diagnostics,
    ) -> Result<Vec<u8>> {
        let mut output = Vec::with_capacity(data.len());
        // `row_length` comes from multiplications that fit in usize, but adding the
        // filter byte might not.
        let chunk = self.row_length.saturating_add(1);

        for (index, row) in data.chunks(chunk).enumerate() {
            let (&filter, row) = row.split_first().expect("chunks are never empty");
            if filter > 4 {
                return Err(Error::new(ErrorKind::InvalidPredictorData, offset));
            }
            if row.len() < self.row_length {
                diagnostics.push(DiagnosticKind::IncompletePredictorRow, offset);
            }
            let start = output.len();
            let has_row_above = index > 0;

            for (i, &byte) in row.iter().enumerate() {
                let left = if i >= self.pixel_length {
                    output[start + i - self.pixel_length]
                } else {
                    0
                };
                let above = if has_row_above {
                    output[start + i - self.row_length]
                } else {
                    0
                };
                let above_left = if has_row_above && i >= self.pixel_length {
                    output[start + i - self.row_length - self.pixel_length]
                } else {
                    0
                };
                let prediction = match filter {
                    0 => 0,
                    1 => left,
                    2 => above,
                    3 => ((u16::from(left) + u16::from(above)) / 2) as u8,
                    _ => paeth(left, above, above_left),
                };
                output.push(byte.wrapping_add(prediction));
            }
        }
        Ok(output)
    }
}

// The PNG Paeth predictor: whichever neighbor is closest to left + above - above_left.
fn paeth(left: u8, above: u8, above_left: u8) -> u8 {
    let estimate = i16::from(left) + i16::from(above) - i16::from(above_left);
    let distance = |value: u8| (estimate - i16::from(value)).abs();
    if distance(left) <= distance(above) && distance(left) <= distance(above_left) {
        left
    } else if distance(above) <= distance(above_left) {
        above
    } else {
        above_left
    }
}
