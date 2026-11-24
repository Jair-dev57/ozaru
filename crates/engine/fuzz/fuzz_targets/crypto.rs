#![no_main]

use libfuzzer_sys::fuzz_target;
use ozaru_engine::crypto::{Aes, Md5, Rc4, Sha256, Sha384, Sha512, cbc_decrypt, cbc_encrypt};

// The first bytes give the key and the vector; the rest is data. Encrypting and
// decrypting gives the data back, hashing in pieces gives the hash of the
// whole, and nothing panics.
fuzz_target!(|data: &[u8]| {
    if data.len() < 49 {
        return;
    }
    let (key, rest) = data.split_at([16, 24, 32][usize::from(data[0]) % 3]);
    let Some((iv, text)) = rest.split_first_chunk::<16>() else {
        return;
    };
    let aes = Aes::new(key).expect("a key of 16, 24 or 32 bytes");
    let whole = text.len() / 16 * 16;
    let mut blocks = text[..whole].to_vec();
    cbc_encrypt(&aes, iv, &mut blocks);
    cbc_decrypt(&aes, iv, &mut blocks);
    assert_eq!(&blocks[..], &text[..whole]);

    let split = usize::from(data[1]) % (text.len() + 1);
    let mut md5 = Md5::new();
    md5.update(&text[..split]);
    md5.update(&text[split..]);
    assert_eq!(md5.finalize(), Md5::digest(text));
    let mut sha = Sha256::new();
    sha.update(&text[..split]);
    sha.update(&text[split..]);
    assert_eq!(sha.finalize(), Sha256::digest(text));
    let mut sha = Sha384::new();
    sha.update(&text[..split]);
    sha.update(&text[split..]);
    assert_eq!(sha.finalize(), Sha384::digest(text));
    let mut sha = Sha512::new();
    sha.update(&text[..split]);
    sha.update(&text[split..]);
    assert_eq!(sha.finalize(), Sha512::digest(text));

    let mut copy = text.to_vec();
    Rc4::new(key).apply(&mut copy);
    Rc4::new(key).apply(&mut copy);
    assert_eq!(&copy[..], text);
});
