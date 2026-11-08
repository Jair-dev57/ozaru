use std::path::Path;

use ozaru_engine::base::{DiagnosticKind, Diagnostics, ErrorKind, Limits};
use ozaru_engine::filters::decode_data;
use ozaru_engine::security::{CryptMethod, FileKey, StandardSecurity};
use ozaru_engine::syntax::{
    Dictionary, Name, Object, ObjectId, PdfString, StringFormat, parse_indirect_object,
    parse_object,
};
use ozaru_engine::xref::{Resolver, XrefEntry};

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn dictionary(text: &str) -> Dictionary {
    parse_object(text.as_bytes(), Limits::default())
        .unwrap()
        .object
        .as_dictionary()
        .unwrap()
        .clone()
}

// The file ID used by the vectors below: the bytes 0 to 15.
fn file_id() -> Vec<u8> {
    (0..16).collect()
}

fn security(entries: &str) -> StandardSecurity {
    StandardSecurity::new(
        &dictionary(&format!("<< /Filter /Standard {entries} >>")),
        &file_id(),
        0,
    )
    .expect("dictionary should be valid")
}

fn error(entries: &str) -> ErrorKind {
    StandardSecurity::new(&dictionary(&format!("<< {entries} >>")), &file_id(), 7)
        .expect_err("should fail")
        .kind()
}

// /Encrypt and the first /ID string of a real file.
fn real_security(name: &str) -> StandardSecurity {
    real_security_at(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests")
            .join(name),
    )
}

fn real_security_at(path: &Path) -> StandardSecurity {
    let file = std::fs::read(path).unwrap();
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let encrypt = resolver
        .trailer()
        .get("Encrypt")
        .and_then(Object::as_reference)
        .unwrap();
    let id = resolver
        .trailer()
        .get("ID")
        .and_then(Object::as_array)
        .unwrap()[0]
        .as_string()
        .unwrap()
        .as_bytes()
        .to_vec();
    let encrypt = resolver.resolve(encrypt).unwrap();
    StandardSecurity::new(encrypt.as_dictionary().unwrap(), &id, 0).unwrap()
}

// Real files

#[test]
fn a_file_with_only_an_owner_password_opens_with_the_empty_password() {
    let security = real_security("owner_password.pdf");

    let key = security
        .authenticate(b"")
        .expect("the user password is empty");

    // The key computed by an independent Python implementation of the standard.
    assert_eq!(key.as_bytes(), hex("571ced4c34"));
    assert_eq!((security.version(), security.revision()), (1, 2));
    assert_eq!(security.stream_method(), CryptMethod::Rc4);
}

#[test]
fn a_file_with_a_user_password_doesnt_open_without_it() {
    let security = real_security("password.pdf");

    assert!(security.authenticate(b"").is_none());
    assert!(security.authenticate(b"wrong").is_none());
}

#[test]
fn permissions_are_read() {
    // ReportLab writes /P -8: everything allowed except bits 1 and 2, reserved.
    assert_eq!(
        real_security("owner_password.pdf").permissions(),
        -8i32 as u32
    );
}

// Vectors made with an independent Python implementation, for user and owner
// passwords chosen there.

#[test]
fn revision_3_with_a_128_bit_key() {
    let security = security(
        "/V 2 /R 3 /Length 128 /P -3904 \
         /O <0ba3835f88f90388e74e54584125ce142be0de24c6b0d37746e075b891756671> \
         /U <b8d04c0b647956d75df3b1f5a437ef9700000000000000000000000000000000>",
    );

    let key = security
        .authenticate(b"user")
        .expect("password should be right");

    assert_eq!(key.as_bytes(), hex("ebc53cf170c71152a5ba9925bd0fefc3"));
    assert!(security.authenticate(b"").is_none());
    // /O was made from the owner password "owner", which gives the same key.
    assert_eq!(security.authenticate(b"owner"), Some(key));
}

#[test]
fn revision_3_with_a_40_bit_key() {
    let security = security(
        "/V 2 /R 3 /Length 40 /P -3904 \
         /O <28592738977ebfdb1974b696a0740839b9a05fcfb17bd8ef4450e8520aba1e92> \
         /U <094ddfb188b5ecf4f012eb085c1e73aa00000000000000000000000000000000>",
    );

    let key = security
        .authenticate(b"abc")
        .expect("password should be right");

    assert_eq!(key.as_bytes(), hex("bd1ea4f53c"));
}

const REVISION_4: &str = "/V 4 /R 4 /P -3904 \
    /CF << /StdCF << /CFM /AESV2 /Length 16 >> >> /StmF /StdCF /StrF /StdCF \
    /O <5588040988e54a8ae991ac9d80aac232a494d494b4bcc37a4dcc6d07206c9192>";

#[test]
fn revision_4_with_crypt_filters() {
    let security = security(&format!(
        "{REVISION_4} /U <f77233651a89373c54329ddb98c3e40f00000000000000000000000000000000>"
    ));

    let key = security
        .authenticate(b"")
        .expect("the user password is empty");

    assert_eq!(key.as_bytes(), hex("83bf6c3586fbcfb8a2c47b3f5122b2a1"));
    assert_eq!(security.key_length(), 16);
    assert_eq!(security.stream_method(), CryptMethod::Aes128);
    assert_eq!(security.string_method(), CryptMethod::Aes128);
}

#[test]
fn unencrypted_metadata_changes_the_key() {
    let security = security(&format!(
        "{REVISION_4} /EncryptMetadata false \
         /U <045e43f3e601552a55989b7ea531e74400000000000000000000000000000000>"
    ));

    let key = security
        .authenticate(b"")
        .expect("the user password is empty");

    assert_eq!(key.as_bytes(), hex("54c205bce3c7dff9ed12046f11ee45a5"));
}

#[test]
fn crypt_filters_can_be_rc4_or_identity() {
    let o = "<5588040988e54a8ae991ac9d80aac232a494d494b4bcc37a4dcc6d07206c9192>";
    let u = "<f77233651a89373c54329ddb98c3e40f00000000000000000000000000000000>";
    let filters = security(&format!(
        "/V 4 /R 4 /P -3904 /O {o} /U {u} \
         /CF << /Old << /CFM /V2 >> >> /StmF /Old /StrF /Identity"
    ));

    assert_eq!(filters.stream_method(), CryptMethod::Rc4);
    assert_eq!(filters.string_method(), CryptMethod::Identity);
    // Without /StmF and /StrF, nothing is encrypted.
    let defaults = security(&format!("/V 4 /R 4 /P -3904 /O {o} /U {u}"));
    assert_eq!(defaults.stream_method(), CryptMethod::Identity);
}

#[test]
fn permissions_stored_unsigned_are_the_same() {
    let o = "<0ba3835f88f90388e74e54584125ce142be0de24c6b0d37746e075b891756671>";
    let u = "<b8d04c0b647956d75df3b1f5a437ef9700000000000000000000000000000000>";
    let signed = security(&format!("/V 2 /R 3 /Length 128 /P -3904 /O {o} /U {u}"));
    let unsigned = security(&format!(
        "/V 2 /R 3 /Length 128 /P 4294963392 /O {o} /U {u}"
    ));

    assert_eq!(signed.permissions(), unsigned.permissions());
    assert!(unsigned.authenticate(b"user").is_some());
}

#[test]
fn file_keys_are_not_printed() {
    let key = real_security("owner_password.pdf")
        .authenticate(b"")
        .unwrap();

    assert_eq!(format!("{key:?}"), "FileKey(5 bytes)");
}

// Errors

const OU: &str = "/O <0000000000000000000000000000000000000000000000000000000000000000> \
    /U <0000000000000000000000000000000000000000000000000000000000000000>";

#[test]
fn other_handlers_and_methods_are_not_supported() {
    for entries in [
        format!("/Filter /Adobe.PubSec /V 1 /R 2 /P -4 {OU}"),
        format!("/V 1 /R 2 /P -4 {OU}"),
        format!("/Filter /Standard /V 3 /R 3 /P -4 {OU}"),
        format!("/Filter /Standard /V 4 /R 4 /P -4 {OU} /CF << /X << /CFM /SM4 >> >> /StmF /X"),
    ] {
        assert_eq!(
            error(&entries),
            ErrorKind::UnsupportedEncryption,
            "{entries}"
        );
    }
}

#[test]
fn missing_or_impossible_entries_are_an_error() {
    let short = "/O <00> /U <00>";
    for entries in [
        format!("/Filter /Standard /R 2 /P -4 {OU}"),
        format!("/Filter /Standard /V 1 /P -4 {OU}"),
        format!("/Filter /Standard /V 1 /R 2 {OU}"),
        format!("/Filter /Standard /V 1 /R 2 /P -4 {short}"),
        format!("/Filter /Standard /V 1 /R 9 /P -4 {OU}"),
        format!("/Filter /Standard /V 1 /R 2 /P 9999999999 {OU}"),
        format!("/Filter /Standard /V 2 /R 3 /Length 44 /P -4 {OU}"),
        format!("/Filter /Standard /V 2 /R 3 /Length 256 /P -4 {OU}"),
        format!("/Filter /Standard /V 2 /R 2 /Length 128 /P -4 {OU}"),
        format!("/Filter /Standard /V 2 /R 4 /P -4 {OU}"),
        format!("/Filter /Standard /V 4 /R 3 /P -4 {OU}"),
        format!("/Filter /Standard /V 2 /R 5 /P -4 {OU}"),
        format!("/Filter /Standard /V 5 /R 4 /P -4 {OU}"),
        // Revision 6 needs 48-byte /O and /U, and /UE.
        format!("/Filter /Standard /V 5 /R 6 /P -4 {OU} /UE <00>"),
        format!("/Filter /Standard /V 4 /R 4 /P -4 {OU} /StmF /Missing"),
        format!("/Filter /Standard /V 4 /R 4 /P -4 {OU} /StmF 1"),
    ] {
        assert_eq!(error(&entries), ErrorKind::InvalidEncryption, "{entries}");
    }
}

// Decryption

const OWNER_PASSWORD_PDF: &str = "owner_password.pdf";

fn read_real(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests")
            .join(name),
    )
    .unwrap()
}

fn owner_password_key() -> (StandardSecurity, FileKey) {
    let security = real_security(OWNER_PASSWORD_PDF);
    let key = security.authenticate(b"").unwrap();
    (security, key)
}

#[test]
fn the_page_of_an_encrypted_file_is_read() {
    let file = read_real(OWNER_PASSWORD_PDF);
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let (security, key) = owner_password_key();
    // The page's /Contents is object 8, with [/ASCII85Decode /FlateDecode].
    let id = ObjectId::new(8, 0);
    let contents = resolver.resolve(id).unwrap();
    let stream = contents.as_stream().unwrap();
    assert!(security.decrypts_stream(stream.dictionary()));

    let raw = stream.raw_data(&file).unwrap();
    let plain = security
        .decrypt_stream_data(
            &key,
            id,
            raw,
            stream.data_range().start,
            &mut Diagnostics::new(10),
        )
        .unwrap();
    let decoded = decode_data(stream.dictionary(), &plain, 0, Limits::default()).unwrap();

    let text = String::from_utf8_lossy(&decoded.data);
    assert!(text.contains("(visible text) Tj"), "{text}");
    // Without decryption, the filters can't make sense of it.
    assert!(decode_data(stream.dictionary(), raw, 0, Limits::default()).is_err());
}

#[test]
fn strings_of_an_encrypted_file_are_read() {
    let file = read_real(OWNER_PASSWORD_PDF);
    let resolver = Resolver::open(&file, Limits::default()).unwrap();
    let (security, key) = owner_password_key();
    let id = resolver
        .trailer()
        .get("Info")
        .and_then(Object::as_reference)
        .unwrap();
    // Read as it is in the file, still encrypted: the resolver would decrypt it.
    let Some(XrefEntry::InUse { offset, .. }) = resolver.table().get(id.number) else {
        panic!("/Info should be in use");
    };
    let mut info = parse_indirect_object(&file, offset, Limits::default())
        .unwrap()
        .object
        .object;

    security
        .decrypt_object(&key, id, &mut info, 0, &mut Diagnostics::new(10))
        .unwrap();

    let producer = info.as_dictionary().unwrap().get("Producer").unwrap();
    let producer = String::from_utf8_lossy(producer.as_string().unwrap().as_bytes()).into_owned();
    assert!(producer.starts_with("ReportLab PDF Library"), "{producer}");
}

fn string(bytes: &[u8]) -> Object {
    Object::String(PdfString::new(bytes, StringFormat::Literal))
}

#[test]
fn every_string_inside_an_object_is_decrypted() {
    let (security, key) = owner_password_key();
    let id = ObjectId::new(3, 0);
    let mut nested = Dictionary::new();
    nested.insert(
        Name::from("Deep"),
        Object::Array(vec![string(b"two"), Object::Integer(7)]),
    );
    let mut dictionary = Dictionary::new();
    dictionary.insert(Name::from("One"), string(b"one"));
    dictionary.insert(Name::from("Nested"), Object::Dictionary(nested));
    dictionary.insert(Name::from("Name"), Object::Name(Name::from("Plain")));
    let original = Object::Dictionary(dictionary);
    let mut object = original.clone();

    security
        .decrypt_object(&key, id, &mut object, 0, &mut Diagnostics::new(10))
        .unwrap();
    let once = object.clone();
    // RC4 is its own inverse, so decrypting again gives the original back.
    security
        .decrypt_object(&key, id, &mut object, 0, &mut Diagnostics::new(10))
        .unwrap();

    assert_ne!(once, original);
    assert_eq!(object, original);
    let once = once.as_dictionary().unwrap();
    assert_eq!(once.get("Name"), Some(&Object::Name(Name::from("Plain"))));
    assert_eq!(
        once.get("Nested")
            .unwrap()
            .as_dictionary()
            .unwrap()
            .get("Deep")
            .unwrap()
            .as_array()
            .unwrap()[1],
        Object::Integer(7)
    );
}

#[test]
fn each_object_has_its_own_key() {
    let (security, key) = owner_password_key();
    let data = b"same bytes";

    let one = security
        .decrypt_string(
            &key,
            ObjectId::new(1, 0),
            data,
            0,
            &mut Diagnostics::new(10),
        )
        .unwrap();
    let two = security
        .decrypt_string(
            &key,
            ObjectId::new(2, 0),
            data,
            0,
            &mut Diagnostics::new(10),
        )
        .unwrap();
    let generation = security
        .decrypt_string(
            &key,
            ObjectId::new(1, 1),
            data,
            0,
            &mut Diagnostics::new(10),
        )
        .unwrap();

    assert_ne!(one, two);
    assert_ne!(one, generation);
}

#[test]
fn deeply_nested_objects_are_decrypted_without_recursion() {
    let (security, key) = owner_password_key();
    let mut object = string(b"bottom");
    for _ in 0..100_000 {
        object = Object::Array(vec![object]);
    }

    security
        .decrypt_object(
            &key,
            ObjectId::new(1, 0),
            &mut object,
            0,
            &mut Diagnostics::new(10),
        )
        .unwrap();

    // Taken apart level by level, since dropping it whole would recurse.
    let mut level = object;
    while let Object::Array(mut items) = level {
        level = items.pop().unwrap();
    }
    assert_ne!(level, string(b"bottom"));
}

#[test]
fn cross_reference_and_unencrypted_metadata_streams_are_left_alone() {
    let rc4 = real_security(OWNER_PASSWORD_PDF);
    let u = "<045e43f3e601552a55989b7ea531e74400000000000000000000000000000000>";
    let no_metadata = security(&format!("{REVISION_4} /EncryptMetadata false /U {u}"));

    assert!(!rc4.decrypts_stream(&dictionary("<< /Type /XRef >>")));
    assert!(rc4.decrypts_stream(&dictionary("<< /Type /Metadata >>")));
    assert!(rc4.decrypts_stream(&dictionary("<< /Length 4 >>")));
    assert!(!no_metadata.decrypts_stream(&dictionary("<< /Type /Metadata >>")));
    assert!(no_metadata.decrypts_stream(&dictionary("<< /Type /Page >>")));
}

#[test]
fn identity_leaves_data_alone() {
    let o = "<5588040988e54a8ae991ac9d80aac232a494d494b4bcc37a4dcc6d07206c9192>";
    let u = "<f77233651a89373c54329ddb98c3e40f00000000000000000000000000000000>";
    let identity = security(&format!("/V 4 /R 4 /P -3904 /O {o} /U {u}"));
    let key = identity.authenticate(b"").unwrap();
    let id = ObjectId::new(1, 0);
    let mut diagnostics = Diagnostics::new(10);

    assert_eq!(
        identity
            .decrypt_string(&key, id, b"plain", 0, &mut diagnostics)
            .unwrap(),
        b"plain"
    );
    assert_eq!(
        identity
            .decrypt_stream_data(&key, id, b"plain", 0, &mut diagnostics)
            .unwrap(),
        b"plain"
    );
    assert!(diagnostics.is_empty());
}

// AES-128. The vectors were encrypted by OpenSSL, with object keys computed by
// Python's hashlib from the file key of the revision 4 vector above.

fn aes() -> (StandardSecurity, FileKey) {
    let u = "<f77233651a89373c54329ddb98c3e40f00000000000000000000000000000000>";
    let security = security(&format!("{REVISION_4} /U {u}"));
    let key = security.authenticate(b"").unwrap();
    (security, key)
}

fn aes_decrypt(id: ObjectId, data: &[u8]) -> (Vec<u8>, Vec<(DiagnosticKind, usize)>) {
    let (security, key) = aes();
    let mut diagnostics = Diagnostics::new(10);
    let plain = security
        .decrypt_stream_data(&key, id, data, 40, &mut diagnostics)
        .unwrap();
    (
        plain,
        diagnostics.iter().map(|d| (d.kind(), d.offset())).collect(),
    )
}

#[test]
fn aes_data_matches_openssl() {
    for (id, encrypted, plain) in [
        (
            ObjectId::new(1, 0),
            "101112131415161718191a1b1c1d1e1f991470c92ad5769e4fa1c283f6e71f21cfe1bc09a26e7b6769691bd93b2563e4",
            &b"Hola, PDF cifrado con AES"[..],
        ),
        (
            ObjectId::new(7, 0),
            "101112131415161718191a1b1c1d1e1f578beb75642ab6cc6369bd1cf350791df000280fbd2e1404975f94980806fb77",
            b"exactly 16 bytes",
        ),
        (
            ObjectId::new(1, 0),
            "101112131415161718191a1b1c1d1e1f4e410a653f90288089ead476afb65c08",
            b"",
        ),
        (
            ObjectId::new(300, 2),
            "101112131415161718191a1b1c1d1e1f5416c93ff67585c624e047650e6092c14f8adc117a8fe23c942a5a6b2ae5f60c609bd5512b9c4383219e6015e3dee445",
            b"BT /F1 12 Tf (visible text) Tj ET",
        ),
    ] {
        let (decrypted, diagnostics) = aes_decrypt(id, &hex(encrypted));

        assert_eq!(decrypted, plain, "{id:?}");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}

#[test]
fn aes_strings_use_the_method_for_strings() {
    let (security, key) = aes();
    let mut object = Object::String(PdfString::new(
        hex(
            "101112131415161718191a1b1c1d1e1f991470c92ad5769e4fa1c283f6e71f21cfe1bc09a26e7b6769691bd93b2563e4",
        ),
        StringFormat::Hexadecimal,
    ));

    security
        .decrypt_object(
            &key,
            ObjectId::new(1, 0),
            &mut object,
            0,
            &mut Diagnostics::new(10),
        )
        .unwrap();

    assert_eq!(
        object.as_string().unwrap().as_bytes(),
        b"Hola, PDF cifrado con AES"
    );
}

#[test]
fn the_wrong_object_number_gives_damaged_data() {
    let encrypted = hex(
        "101112131415161718191a1b1c1d1e1f991470c92ad5769e4fa1c283f6e71f21cfe1bc09a26e7b6769691bd93b2563e4",
    );

    let (decrypted, diagnostics) = aes_decrypt(ObjectId::new(2, 0), &encrypted);

    assert_ne!(decrypted, b"Hola, PDF cifrado con AES");
    assert_eq!(diagnostics, [(DiagnosticKind::DamagedEncryptedData, 40)]);
}

#[test]
fn damaged_aes_data_is_decrypted_as_far_as_it_goes() {
    let full = hex(
        "101112131415161718191a1b1c1d1e1f5416c93ff67585c624e047650e6092c14f8adc117a8fe23c942a5a6b2ae5f60c609bd5512b9c4383219e6015e3dee445",
    );
    let id = ObjectId::new(300, 2);

    // A partial block at the end is left out.
    let (cut, diagnostics) = aes_decrypt(id, &full[..full.len() - 5]);
    assert_eq!(cut, b"BT /F1 12 Tf (visible text) Tj ET"[..32]);
    assert_eq!(diagnostics, [(DiagnosticKind::DamagedEncryptedData, 40)]);

    // Without its initialization vector, or with only it, nothing is left.
    for data in [&full[..10], &full[..16]] {
        let (decrypted, diagnostics) = aes_decrypt(id, data);
        assert!(decrypted.is_empty());
        assert_eq!(diagnostics, [(DiagnosticKind::DamagedEncryptedData, 40)]);
    }
}

#[test]
fn empty_aes_data_stays_empty() {
    let (decrypted, diagnostics) = aes_decrypt(ObjectId::new(1, 0), b"");

    assert!(decrypted.is_empty());
    assert!(diagnostics.is_empty());
}

// Revisions 5 and 6, AES-256. The vectors were made by an independent Python
// implementation, with hashlib for SHA-2 and OpenSSL for AES, for the file key
// 40 41 42 ... 5f.

const FILE_KEY_256: &str = "404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f";
const O_48: &str = "<000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000>";

fn revision_6(u: &str, ue: &str) -> StandardSecurity {
    security(&format!(
        "/V 5 /R 6 /Length 256 /P -1028 \
         /CF << /StdCF << /CFM /AESV3 /Length 32 >> >> /StmF /StdCF /StrF /StdCF \
         /O {O_48} /U <{u}> /UE <{ue}> /OE <{}>",
        "00".repeat(32)
    ))
}

#[test]
fn revision_6_with_the_empty_password() {
    let security = revision_6(
        "8d1efb4f1bdbb651341704c2139de4f6be05d6d4609af56916b21646ed74825c0102030405060708090a0b0c0d0e0f10",
        "20838853b2a91db91049a4aad8d2ce985e5a8bb1c1dff18b2e55c855eb66041f",
    );

    let key = security
        .authenticate(b"")
        .expect("the user password is empty");

    assert_eq!(key.as_bytes(), hex(FILE_KEY_256));
    assert_eq!(security.key_length(), 32);
    assert_eq!(security.stream_method(), CryptMethod::Aes256);
    assert!(security.authenticate(b"x").is_none());
}

#[test]
fn revision_6_passwords_are_utf8() {
    let security = revision_6(
        "fc000de170d6739385e906cab1453dd15ae60ec493c15d88809c93740e99d6a40102030405060708090a0b0c0d0e0f10",
        "0607b432452e0a8b826348485211fe882cfe0a443ffd29d90ecc4916682def8d",
    );

    let key = security
        .authenticate("contraseña".as_bytes())
        .expect("password should be right");

    assert_eq!(key.as_bytes(), hex(FILE_KEY_256));
    assert!(security.authenticate(b"").is_none());
    assert!(security.authenticate(b"contrasena").is_none());
}

#[test]
fn revision_5_uses_a_single_sha256() {
    let security = security(&format!(
        "/V 5 /R 5 /P -1028 /CF << /StdCF << /CFM /AESV3 >> >> /StmF /StdCF /StrF /StdCF \
         /O {O_48} \
         /U <66840dda154e8a113c31dd0ad32f7f3a366a80e8136979d8f5a101d3d29d6f720102030405060708090a0b0c0d0e0f10> \
         /UE <fe6ecad7255942aea7edfc7337d093e1da6025328039b63d7c3534a6a438fd44>"
    ));

    let key = security
        .authenticate(b"")
        .expect("the user password is empty");

    assert_eq!(key.as_bytes(), hex(FILE_KEY_256));
}

#[test]
fn aes_256_data_is_decrypted_with_the_file_key() {
    let security = revision_6(
        "8d1efb4f1bdbb651341704c2139de4f6be05d6d4609af56916b21646ed74825c0102030405060708090a0b0c0d0e0f10",
        "20838853b2a91db91049a4aad8d2ce985e5a8bb1c1dff18b2e55c855eb66041f",
    );
    let key = security.authenticate(b"").unwrap();
    let encrypted = hex(
        "a0a1a2a3a4a5a6a7a8a9aaabacadaeaf9d08954161279daca83ca8c5978ccdf2814209cc93201cd7000bfc7755838875",
    );
    let mut diagnostics = Diagnostics::new(10);

    // Every object uses the file key itself, whatever its number.
    for id in [ObjectId::new(1, 0), ObjectId::new(900, 3)] {
        let plain = security
            .decrypt_stream_data(&key, id, &encrypted, 0, &mut diagnostics)
            .unwrap();
        assert_eq!(plain, b"BT (AES-256 text) Tj ET");
    }
    assert!(diagnostics.is_empty());
}

// The owner password. Every file below was encrypted with the owner password
// "owner": the Ghostscript rewrites in tests/files and the qpdf ones in
// tests/corpus.

fn engine_file(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(path)
}

#[test]
fn the_owner_password_gives_the_same_key_as_the_user_password() {
    for (path, user, revision) in [
        ("files/report-rc4-40.pdf", "", 2),
        ("files/report-rc4-128.pdf", "", 3),
        ("files/report-user-password.pdf", "user", 3),
        ("corpus/aes-128.pdf", "", 4),
        ("corpus/aes-256.pdf", "", 6),
        ("corpus/aes-256-user-password.pdf", "user", 6),
    ] {
        let security = real_security_at(&engine_file(path));
        assert_eq!(security.revision(), revision, "{path}");

        let from_user = security.authenticate(user.as_bytes());
        let from_owner = security.authenticate(b"owner");

        assert!(from_user.is_some(), "{path}");
        assert_eq!(from_owner, from_user, "{path}");
    }
}

#[test]
fn a_wrong_owner_password_opens_nothing() {
    for path in [
        "files/report-user-password.pdf",
        "corpus/aes-256-user-password.pdf",
    ] {
        let security = real_security_at(&engine_file(path));

        for password in [&b"Owner"[..], b"owner ", b"wrong", b""] {
            assert_eq!(
                security.authenticate(password),
                None,
                "{path}: {password:?}"
            );
        }
    }
}

#[test]
fn a_file_opened_with_the_owner_password_is_decrypted() {
    let file = std::fs::read(engine_file("corpus/aes-256-user-password.pdf")).unwrap();
    let options = ozaru_engine::OpenOptions {
        password: Some(b"owner"),
        ..ozaru_engine::OpenOptions::default()
    };

    let document = ozaru_engine::Document::open_with(&file, options).unwrap();

    assert_eq!(document.page_count(), 6);
    let title = document.info().and_then(|info| info.title());
    assert_eq!(title.as_deref(), Some("sizes-and-rotations.pdf"));
}
