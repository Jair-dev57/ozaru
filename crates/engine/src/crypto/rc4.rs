//! RC4, a stream cipher.

/// RC4 with a key of 1 to 256 bytes.
pub struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    /// The cipher for `key`. An empty key is used as the single byte 0.
    pub fn new(key: &[u8]) -> Self {
        let key: &[u8] = if key.is_empty() { &[0] } else { key };
        let mut s = [0u8; 256];
        for (i, byte) in s.iter_mut().enumerate() {
            *byte = i as u8;
        }
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, usize::from(j));
        }
        Self { s, i: 0, j: 0 }
    }

    /// Turns `data` into its encryption or decryption: they are the same.
    pub fn apply(&mut self, data: &mut [u8]) {
        for byte in data {
            self.i = self.i.wrapping_add(1);
            self.j = self.j.wrapping_add(self.s[usize::from(self.i)]);
            self.s.swap(usize::from(self.i), usize::from(self.j));
            let index = self.s[usize::from(self.i)].wrapping_add(self.s[usize::from(self.j)]);
            *byte ^= self.s[usize::from(index)];
        }
    }
}

/// `data` encrypted or decrypted with RC4 under `key`.
pub fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    Rc4::new(key).apply(&mut out);
    out
}
