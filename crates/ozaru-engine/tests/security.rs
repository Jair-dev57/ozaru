use std::path::Path;

use ozaru_engine::base::{ErrorKind, Limits};
use ozaru_engine::filters::decode_data;
use ozaru_engine::security::{CryptMethod, FileKey, StandardSecurity};
use ozaru_engine::syntax::{
    Dictionary, Name, Object, ObjectId, PdfString, StringFormat, parse_object,
};
use ozaru_engine::xref::Resolver;

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
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests")
        .join(name);
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
    assert!(security.authenticate(b"owner").is_none());
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
fn other_handlers_and_newer_revisions_are_not_supported_yet() {
    for entries in [
        format!("/Filter /Adobe.PubSec /V 1 /R 2 /P -4 {OU}"),
        format!("/V 1 /R 2 /P -4 {OU}"),
        format!("/Filter /Standard /V 5 /R 6 /P -4 {OU}"),
        format!("/Filter /Standard /V 2 /R 5 /P -4 {OU}"),
        format!("/Filter /Standard /V 3 /R 3 /P -4 {OU}"),
        format!("/Filter /Standard /V 4 /R 4 /P -4 {OU} /CF << /X << /CFM /AESV3 >> >> /StmF /X"),
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
        .decrypt_stream_data(&key, id, raw, stream.data_range().start)
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
    let mut resolver = Resolver::open(&file, Limits::default()).unwrap();
    let (security, key) = owner_password_key();
    let id = resolver
        .trailer()
        .get("Info")
        .and_then(Object::as_reference)
        .unwrap();
    let mut info = (*resolver.resolve(id).unwrap()).clone();

    security.decrypt_object(&key, id, &mut info, 0).unwrap();

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

    security.decrypt_object(&key, id, &mut object, 0).unwrap();
    let once = object.clone();
    // RC4 is its own inverse, so decrypting again gives the original back.
    security.decrypt_object(&key, id, &mut object, 0).unwrap();

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
        .decrypt_string(&key, ObjectId::new(1, 0), data, 0)
        .unwrap();
    let two = security
        .decrypt_string(&key, ObjectId::new(2, 0), data, 0)
        .unwrap();
    let generation = security
        .decrypt_string(&key, ObjectId::new(1, 1), data, 0)
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
        .decrypt_object(&key, ObjectId::new(1, 0), &mut object, 0)
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
fn identity_leaves_data_alone_and_aes_is_not_supported_yet() {
    let o = "<5588040988e54a8ae991ac9d80aac232a494d494b4bcc37a4dcc6d07206c9192>";
    let u = "<f77233651a89373c54329ddb98c3e40f00000000000000000000000000000000>";
    let identity = security(&format!("/V 4 /R 4 /P -3904 /O {o} /U {u}"));
    let aes = security(&format!("{REVISION_4} /U {u}"));
    let id = ObjectId::new(1, 0);

    let key = identity.authenticate(b"").unwrap();
    assert_eq!(
        identity.decrypt_string(&key, id, b"plain", 0).unwrap(),
        b"plain"
    );
    assert_eq!(
        identity.decrypt_stream_data(&key, id, b"plain", 0).unwrap(),
        b"plain"
    );

    let key = aes.authenticate(b"").unwrap();
    let error = aes.decrypt_stream_data(&key, id, &[0; 32], 12).unwrap_err();
    assert_eq!(
        (error.kind(), error.offset()),
        (ErrorKind::UnsupportedEncryption, 12)
    );
}
