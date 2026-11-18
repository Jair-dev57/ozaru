/// A black and white image, one bit for each pixel, rows starting on a byte,
/// the most significant bit first: 1 is black, as JBIG2 has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Bitmap {
    pub width: usize,
    pub height: usize,
    stride: usize,
    data: Vec<u8>,
}

/// How a region is combined with the bitmap it is drawn on (ITU-T T.88,
/// section 7.4.1.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Combination {
    Or,
    And,
    Xor,
    Xnor,
    Replace,
}

impl Combination {
    pub fn from_bits(bits: u8) -> Self {
        match bits & 7 {
            0 => Self::Or,
            1 => Self::And,
            2 => Self::Xor,
            3 => Self::Xnor,
            _ => Self::Replace,
        }
    }
}

impl Bitmap {
    /// A bitmap of `width` by `height` pixels, all `value`.
    pub fn new(width: usize, height: usize, value: bool) -> Self {
        let stride = width.div_ceil(8);
        Self {
            width,
            height,
            stride,
            data: vec![if value { 0xFF } else { 0 }; stride * height],
        }
    }

    /// The pixel at `x`, `y`: 0 outside the bitmap.
    #[inline]
    pub fn get(&self, x: i64, y: i64) -> u32 {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return 0;
        }
        let (x, y) = (x as usize, y as usize);
        u32::from(self.data[y * self.stride + x / 8] >> (7 - x % 8) & 1)
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, value: bool) {
        if x >= self.width || y >= self.height {
            return;
        }
        let byte = &mut self.data[y * self.stride + x / 8];
        let bit = 0x80 >> (x % 8);
        if value {
            *byte |= bit;
        } else {
            *byte &= !bit;
        }
    }

    /// Copies row `from` onto row `to`.
    pub fn copy_row(&mut self, from: usize, to: usize) {
        if from >= self.height || to >= self.height || from == to {
            return;
        }
        let stride = self.stride;
        self.data
            .copy_within(from * stride..(from + 1) * stride, to * stride);
    }

    /// Makes the bitmap `height` rows tall, the new rows `value`.
    pub fn grow(&mut self, height: usize, value: bool) {
        if height > self.height {
            self.data
                .resize(height * self.stride, if value { 0xFF } else { 0 });
            self.height = height;
        }
    }

    /// Draws `other` with its top left corner at `x`, `y`, combined as
    /// `combination` says. What falls outside is left out.
    pub fn draw(&mut self, other: &Bitmap, x: i64, y: i64, combination: Combination) {
        for row in 0..other.height as i64 {
            let ty = y + row;
            if ty < 0 || ty >= self.height as i64 {
                continue;
            }
            for column in 0..other.width as i64 {
                let tx = x + column;
                if tx < 0 || tx >= self.width as i64 {
                    continue;
                }
                let source = other.get(column, row) == 1;
                let target = self.get(tx, ty) == 1;
                let value = match combination {
                    Combination::Or => target | source,
                    Combination::And => target & source,
                    Combination::Xor => target ^ source,
                    Combination::Xnor => !(target ^ source),
                    Combination::Replace => source,
                };
                self.set(tx as usize, ty as usize, value);
            }
        }
    }

    /// The pixels as PDF image data: rows of bits where 0 is black, as an
    /// image of `/DeviceGray` and 1 bit paints black.
    pub fn into_pdf_data(mut self) -> Vec<u8> {
        for byte in &mut self.data {
            *byte = !*byte;
        }
        self.data
    }

    /// The rows of a bitmap whose data, 1 for black, is `data`.
    pub fn from_rows(width: usize, height: usize, data: Vec<u8>) -> Self {
        let stride = width.div_ceil(8);
        let mut data = data;
        data.resize(stride * height, 0);
        Self {
            width,
            height,
            stride,
            data,
        }
    }

    /// The area of `region`, `width` by `height` from `x`, `y`, cut out.
    pub fn cut(&self, x: i64, y: i64, width: usize, height: usize) -> Bitmap {
        let mut out = Bitmap::new(width, height, false);
        for row in 0..height {
            for column in 0..width {
                if self.get(x + column as i64, y + row as i64) == 1 {
                    out.set(column, row, true);
                }
            }
        }
        out
    }
}
