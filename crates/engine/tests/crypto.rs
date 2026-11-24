//! The hashes and ciphers of PDF encryption, against vectors made by Python's
//! `hashlib` and `cryptography`, and by the standards' own examples.

use ozaru_engine::crypto::{Aes, Md5, Rc4, Sha256, Sha384, Sha512, cbc_decrypt, cbc_encrypt};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

// The bytes the vectors were made from.
fn data(length: usize, seed: usize) -> Vec<u8> {
    (0..length).map(|i| ((i * 7 + seed) & 255) as u8).collect()
}

#[test]
fn md5_matches_the_reference_for_every_length() {
    let vectors: &[(usize, &str)] = &[
        (0, "d41d8cd98f00b204e9800998ecf8427e"),
        (1, "8666683506aacd900bbd5a74ac4edf68"),
        (3, "c9aee4810523ef8658121b8d492c6b41"),
        (55, "52c0e574e1198de5fe3f8f11440dcb1b"),
        (56, "46c9907fc908ee68b1e7b8e71286a518"),
        (57, "1c805dd236c35cab25fcb1bc73802c51"),
        (63, "a62f6d59e837867693f042f5b8f5a236"),
        (64, "7160b8fb5e9e4023d549c3971fbaeead"),
        (65, "70bd662e7aefbda85a0f7244167b7897"),
        (111, "13f00cb15f0c3fe588d1c17cde14b38f"),
        (112, "5fe6a9f76b541ec645559589da9a088a"),
        (113, "d4be13fd3019b6e959e11ff06e5671ae"),
        (127, "acce2474d6cc8302120d09c818d17ef7"),
        (128, "10b2da1a82f16d99a81a7203fe9f02cb"),
        (129, "03fefbbcebe2959cf2270241aafe0250"),
        (200, "4c79b81ac94bad7a875519ce6b964c66"),
        (1000, "10046f077f2082ac19676b8079f1cb1a"),
        (4097, "c7cfb8a6d3d4929f9035e0918250b62d"),
    ];
    for &(length, expected) in vectors {
        let input = data(length, 3);
        assert_eq!(hex(&Md5::digest(&input)), expected, "length {length}");
        // In pieces of every size, the hash is the same.
        for piece in [1, 7, 64, 100] {
            let mut hash = Md5::new();
            for chunk in input.chunks(piece) {
                hash.update(chunk);
            }
            assert_eq!(
                hex(&hash.finalize()),
                expected,
                "length {length} in pieces of {piece}"
            );
        }
    }
}

#[test]
fn sha256_matches_the_reference_for_every_length() {
    let vectors: &[(usize, &str)] = &[
        (
            0,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            1,
            "084fed08b978af4d7d196a7446a86b58009e636b611db16211b65a9aadff29c5",
        ),
        (
            3,
            "6ab0dba1f4f1dfbb37b4f9eeb092c09fca4900ad32bdcd147d8dde35d6c87c35",
        ),
        (
            55,
            "e7313d333c272e639f790978283f9eb392e843d0f29b7016828bb1daa4aac70b",
        ),
        (
            56,
            "4324d65f3c103567f5589c710bc08f8523f929a9272e3af36fc968e52abc6c27",
        ),
        (
            57,
            "35df609437dcfea3279283ab79fd554e2bf78f8f7ae2de532d8ee300b09e8f73",
        ),
        (
            63,
            "81c80242132f230c3bd41b3e63bbcff16107339549214a99614ff26664625055",
        ),
        (
            64,
            "39e3d7b6b5d075d37d053ad89b24b41bef4f3c29760c84447cab3f3be1882241",
        ),
        (
            65,
            "aacca6ff74fdbb296d165a45cecfa04e5127bc008770fbbdd48006f2d2fae95e",
        ),
        (
            111,
            "67d9492e628fd376e0b2efec8ca2b99b123e202cf620deb270728df979b2f73e",
        ),
        (
            112,
            "96b928cff8528dbb99602c709a65b846cb6467acb8b722f0d758e4dc27bfc508",
        ),
        (
            113,
            "cded73eed7df41eecd1ead825a38ca8763a9e910d5181c8ad8f201cc8ff2cbe4",
        ),
        (
            127,
            "a8d23e75d936f303d248888d9b165ee543f4cbafcad3c9dd2a79bd84faa11d07",
        ),
        (
            128,
            "d2742f1f4ac6bb7ca2b239ee18402ba8b3f9f8e652d2a72973c2b9ba11c08cf6",
        ),
        (
            129,
            "307f8fc2c1622b92762e818d39a185d4d667ad49a4b07ceae1f4afa008a93ec4",
        ),
        (
            200,
            "2c7e18c942ef065b526a2d4e5546283749cd3ddfb51d8fc71f42717363685f46",
        ),
        (
            1000,
            "1e9bc38cbf860b9ec31918b065f9b52476c549a782e0e7990bed8ce3868d2371",
        ),
        (
            4097,
            "225270fa091dc75bf959a0e3bbcb897fc3ae8e7fa128ae1394cf5614e15505f5",
        ),
    ];
    for &(length, expected) in vectors {
        let input = data(length, 3);
        assert_eq!(hex(&Sha256::digest(&input)), expected, "length {length}");
        // In pieces of every size, the hash is the same.
        for piece in [1, 7, 64, 100] {
            let mut hash = Sha256::new();
            for chunk in input.chunks(piece) {
                hash.update(chunk);
            }
            assert_eq!(
                hex(&hash.finalize()),
                expected,
                "length {length} in pieces of {piece}"
            );
        }
    }
}

#[test]
fn sha384_matches_the_reference_for_every_length() {
    let vectors: &[(usize, &str)] = &[
        (
            0,
            "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b",
        ),
        (
            1,
            "8a6c69af6fb6247635f837958446fb8f10e39bd5fbc244f7e635176339a3be614f6394247f01dbe1126c178c7bd48cb5",
        ),
        (
            3,
            "de5060203c58b419f136c17666d339a05705575827e3600f0475f6fb3089870aede26c5a0903dea5938bcded4ee9e619",
        ),
        (
            55,
            "fc2728f452750d08b5dbad1aea53202e7fc19c06894d13e36c433b0444a899277c49f16e0577c03db0f0605578836ef6",
        ),
        (
            56,
            "6c969a865f38d2eaac60b7963a84c97ba27bcb76e589cbe1e2e89ead6aee4ad0337a4268f054be49c604ddd99692036d",
        ),
        (
            57,
            "a6c1a73ce16f33c777d99dd4a66304bc5de531caab5aa4f80b46eaf18b08b45875aa2b385bc511d5a5f1985cad2e5dee",
        ),
        (
            63,
            "be67a9a1162c1d97f183980dc811f4f4f835b8b850eb32ecbf87397b26369bc6646a8f2234940dcff142b627c8e21ebb",
        ),
        (
            64,
            "99d26430f561864c2f7d90b68650b960c86c85d6bf7f54ceeee404bdd59663a7f05588b5c750b862a2a24f6a9ad4c050",
        ),
        (
            65,
            "cada16d798ec25243470f7827eb89a2402d52680b092dffc0f59f5a919e5058aebf7ad79dba85c61b179d23534d7c855",
        ),
        (
            111,
            "341388a9dc2275074e90cf394323761919761c805fd9e370977c9966a0e8c81a52135f02577670b0071638a4a26dbc31",
        ),
        (
            112,
            "619cc5d06138526d70659eccf602d197e63e1050e22039a7feb40a30a5b2b08fb03729e291df12f8c576e6f1cd8af22a",
        ),
        (
            113,
            "484a92d02b61b620ccae687fce4cd073eb995294a777d5deb5356bc8c1e2ce984f63f16420058ac5da666fda410a4feb",
        ),
        (
            127,
            "f8234ab81f5f9641c656680825a9bbb3c2616bcc80b65d5e479a4d96742e1b8742330c9e8256ef5f03caa932487044ca",
        ),
        (
            128,
            "e8480e9c4dd90f88104a79cbaccec48edbd798a142b4f241d726dc252f1502350e824c7d18dadd59d7d716919fb8f9bf",
        ),
        (
            129,
            "430173387764874661acba1e914e50f6d0fbac534809e859b1da4396fad3d5f24c25c6a2d02486d8fbda3994713d981a",
        ),
        (
            200,
            "8deb83535fa35d2f493c6c3695b1057b19232d2f531a0d398d2b958413855aa1594b6bcca0ddcbd24a981330a7ff1cc1",
        ),
        (
            1000,
            "94c38db521ca733b8904c2d14b6e82d33dcfcc26e1318c579dbee1fc2f472019034e792263b9d46e90e700de2f6e7e91",
        ),
        (
            4097,
            "c3066deb312d098041e5e6e4ed4c857b1dc87d7fd58cbe925bacf4428591bcd72e423c74547c7abac85a04287545058a",
        ),
    ];
    for &(length, expected) in vectors {
        let input = data(length, 3);
        assert_eq!(hex(&Sha384::digest(&input)), expected, "length {length}");
        // In pieces of every size, the hash is the same.
        for piece in [1, 7, 64, 100] {
            let mut hash = Sha384::new();
            for chunk in input.chunks(piece) {
                hash.update(chunk);
            }
            assert_eq!(
                hex(&hash.finalize()),
                expected,
                "length {length} in pieces of {piece}"
            );
        }
    }
}

#[test]
fn sha512_matches_the_reference_for_every_length() {
    let vectors: &[(usize, &str)] = &[
        (
            0,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
        ),
        (
            1,
            "e45bf5817ddf94aa2f7a407071f0eedc6beb98f768b4cd33d1176d44d1563a45a5d7212290eb7670c6786b13591aedac86478993895e8b24e612014abaa6ba04",
        ),
        (
            3,
            "8853167ab14c043919981a6409307ef68b5b726f4b80dcab259daa24b7cabcb8b4bbfd3e2e36ed98465059661f672e6529cdcc673e7eabfa78dd3186745b112a",
        ),
        (
            55,
            "14fd424b1fcadee624da946ab03f7e1def7c0d6e00f689594319881a26ff30b875ba4c622ac13100c8cc784c9c2eb23159aecbb4a02e3999062f551193e2b256",
        ),
        (
            56,
            "480fa85be41ef55a41208ca28ffc8743c91cf7d24758defe6f95bfb16de614fc86b701034896b047dd571de4318853d80e0809df162f1752cb26da6ddb94a0dd",
        ),
        (
            57,
            "0582c85feba6881e7c3c371a82a645547a1a9bdb3fd0709d6a42e271697333b23d1692ff0076071fef520f46d04ca74c2b088b58a60ade3ba0c18380d230789c",
        ),
        (
            63,
            "ecd42a703a4e93e163d60d55e3785b1a763838b0351bc2e6f7c94b4bfb24f9aa15da5d744ebcebe11f0fc4315d45ba3a047b6e60e07448357f2795bf34b73502",
        ),
        (
            64,
            "8f3cc30b3fb5bf963688a46488249248ac2c67f0f85a145233c6c1e3c16dcd1df634c07d1d31da02576f65b9cf64e1c3fdb318b689b8a14e2e9552bcf30fb133",
        ),
        (
            65,
            "22050d1b2c5bd016c4b04c3e84f513ffbbbc057f83dc9ab196fee6684e5ee19d98b84c6538b3c5e1fc67dbc484e9d0a198254e41d0fa0cf5106991e106632419",
        ),
        (
            111,
            "68cffa6d0d76f309c9ce0d35280939f8e25990c43b7b086ccdf709be35b07d4ddba599541ff2b1c19d34ea49aeafb9659adb7ac3c0b078bb30a22d57fc6687ef",
        ),
        (
            112,
            "d0865c524d1dddf7c23b799c413f5adcd7caefd3f66a9b49750ec81066012c25a8bcf94ddea6dc525691673097ca40e0101e897fc97218cfdb0704084e2bef4b",
        ),
        (
            113,
            "606314353bd419f9f7720e8297f2af8a9ed5f0bb0bed29b716bf18e34577623ae7effa46f496bd4282134e7e895284a4760a910d2ad8ec11e9dd75863058cd9c",
        ),
        (
            127,
            "e0b6a20f1c0c88970a9340152cd5a1c1ecf3d3b8de55102741879438079473540133b812706e5dbec322c8c9523b6fc8c6d16ee626e87ad5fe3d2916afedc369",
        ),
        (
            128,
            "99b16f17aa0b969a5b8f08f367719d516e330ccd2660b6f0688ec031dbc783de50a1cd185a2568dba75070a2403d17d4741d163578515dfd2ff756ddfe4d47b1",
        ),
        (
            129,
            "a1556e29185778aa5991e34b8884c840d589f0fbb4b8ed590e51e9ac4eb03a008125000db2671f8fe7f485b59a77b518670078ecb41a54b4cd02a7f1d2ca4c6d",
        ),
        (
            200,
            "cca3c0276046ef9f2897bdfc3ec330f77f4959914b1462bd581b232ddb3e9aa98acf5f5a2b21c7f49d2e43721daa61a2b5cee6af6052dfeb766e66ddb0d1719c",
        ),
        (
            1000,
            "00e36fccf193e59697a92b5ab24666ce6326d7fa16bf10832d0991ddc591112e9dfa6a636950ed9c4d67344a760654c2ff7785e1d60094d651038735b5dccabd",
        ),
        (
            4097,
            "6ab0d4006246eac1d01876ab7337fa52cbe0b0f31708c15827feab8aaed85ec56f6839b9f155372bcdf0da5c0529bfe50deb836d4567a554af7d735513848518",
        ),
    ];
    for &(length, expected) in vectors {
        let input = data(length, 3);
        assert_eq!(hex(&Sha512::digest(&input)), expected, "length {length}");
        // In pieces of every size, the hash is the same.
        for piece in [1, 7, 64, 100] {
            let mut hash = Sha512::new();
            for chunk in input.chunks(piece) {
                hash.update(chunk);
            }
            assert_eq!(
                hex(&hash.finalize()),
                expected,
                "length {length} in pieces of {piece}"
            );
        }
    }
}

#[test]
fn the_hashes_of_the_standards_examples_are_right() {
    assert_eq!(
        hex(&Md5::digest(b"abc")),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(hex(&Md5::digest(b"")), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(
        hex(&Sha256::digest(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&Sha384::digest(b"abc")),
        "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
    );
    assert_eq!(
        hex(&Sha512::digest(b"abc")),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    // A million letters a.
    let million = vec![b'a'; 1_000_000];
    assert_eq!(
        hex(&Sha256::digest(&million)),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
    assert_eq!(
        hex(&Sha512::digest(&million)),
        "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b"
    );
}

#[test]
fn rc4_matches_the_reference() {
    let vectors: &[(&str, usize, &str)] = &[
        ("0b", 10, "cf5a3092fd3715f647b3"),
        (
            "0b12192027",
            100,
            "88fb92259650793076b156582f5a87c1091386831a49622fe0695d1a3e8767c7d41ce899601887abaca1c42ff083b37ac0523b264c708003b0b08ad10ade7f27ef4fea67fad2d0c532c28008f51a0084338b19634676dcbf7b2f283df05ed3b99b4ee115",
        ),
        (
            "0b121920272e353c",
            33,
            "f2994020822e2b1165fa54bec8d0772dff71ec91e87394e78a7971f78ca6a17000",
        ),
        (
            "0b121920272e353c434a51585f666d74",
            500,
            "fb2182ec6e79a988d674d2a7d7de9b5a7d41c62a495f407c249f97434fd77f0000639bf46653f6f4f37d5c47c59906f324058be1f20bff1071450d85a57a3a612e6ba38079fe95fc4658b510642b8b134e366d653012f12087c7f7d360191c89c64997c57b25adfac16824c24144ff27b9a5e8d424c1b653a648a4fd0a91cf062f3dadef7d904c63c82f76fb11f9cc66bf8e995bae79a6b1bbf7cf21fa652999d7487a9257a49935877b740e153c927887141833849f52ea03dd3e7ed754cd4244f6e098674ff48ff47991c84382c15e7befaf56df09df63b9c6adef0745606c2891ba22ae0a83786eab9f2245593966309922b5f332ec52a7df36f4f0a52ce1832aabefe3630407268d8c5d9ccc9e7d0b460442f110c3d2a26d4d4ad76fabc7cddb2357e700e79ae44110045b79ba5bb00c6430cee491b738b063f93eea79dad4cead542ff560aac8f27dd7a8790318f8115634008cf9bf7408f74132fe7fe1a33b4022d91f58881a9bd763b33648218fe82742e7cc57e80c9a6518b21a9b84ad53035be730801d6bce46b853f1379c53a5a766acc52e348c8bfe0f9532bc01653ae821fc6902f65939644e190396f8c8ca660c8d4c61393aeb4488e77e9e5ff30e20bc408b9e442b2c24041bcf1bda7ed77087d50aea2b17fd9b37da73102e2c3094153eeb2f397227580a4a530a6f1dc1cff8",
        ),
        ("0b121920272e353c434a51585f666d74", 1, "fb"),
        (
            "0b121920272e353c434a51585f666d747b828990979ea5acb3bac1c8cfd6dde4",
            300,
            "7606ff521e5c754cc7588548e29e40123dcc19ec8c613028e9e1c218f8739a60053e0bcabbe4f2653c74703ffcf93c820afa9ca3c9a14cf3fb0cf0b472328f3fa3d99cfd48218d45cb48a154437088a333707f45db5fc80c85ac0592bf40b2a1c5ac31c79c739ca3841fdb8ac30468bfffb15db040b03acd208f85fde62b4503f17f06e7f9edbb39c5047580552ca51a726b994fda4b886f78861e50fb1d6b3149be9c9d898156830d340f67d5325ebc74855ecb7e40535a5a6379aa0cf1838bc49a519fc084110daa33719053b0906145d389f66d1bc14e616eebe37ba35ecc19e5808a912c39da8b1495197a79e449fc8efe43208e30495ab9cf4352df5953cb009f449069afed1d8bb40985050d0c4292fb07c9c8a2c2501cd1af8f6d53995f5a5fc0c541ea0d6f4a28b9",
        ),
        (
            "0b121920272e353c434a51585f666d747b828990979ea5acb3bac1c8cfd6dde4ebf2f900070e151c232a31383f464d545b626970777e858c939aa1a8afb6bdc4cbd2d9e0e7eef5fc030a11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dce3eaf1f8ff060d141b222930373e454c535a61686f767d848b9299a0a7aeb5bcc3cad1d8dfe6edf4fb020910171e252c333a41484f565d646b727980878e959ca3aab1b8bfc6cdd4dbe2e9f0f7fe050c131a21282f363d444b525960676e757c838a91989fa6adb4bbc2c9d0d7dee5ecf3fa01080f161d242b323940474e555c636a71787f868d949ba2a9b0b7bec5ccd3dae1e8eff6fd04",
            64,
            "416b9339b426d8e2ab176731bbdbe22beadf1106ca9ba0f6dee05ea429a21d6f16245e91151ed61b4c65d780bd53f06f8f91cfebb83fc7d9d9240864d6e7aebc",
        ),
    ];
    for &(key, length, expected) in vectors {
        let mut text = data(length, 5);
        Rc4::new(&unhex(key)).apply(&mut text);
        assert_eq!(hex(&text), expected, "key {key}");
        // Applying it again gives the text back.
        Rc4::new(&unhex(key)).apply(&mut text);
        assert_eq!(text, data(length, 5));
    }
}

#[test]
fn aes_matches_the_examples_of_fips_197() {
    let plain = unhex("00112233445566778899aabbccddeeff");
    let vectors: &[(&str, &str)] = &[
        (
            "000102030405060708090a0b0c0d0e0f",
            "69c4e0d86a7b0430d8cdb78070b4c55a",
        ),
        (
            "000102030405060708090a0b0c0d0e0f1011121314151617",
            "dda97ca4864cdfe06eaf70a0ec0d7191",
        ),
        (
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
            "8ea2b7ca516745bfeafc49904b496089",
        ),
    ];
    for &(key, cipher) in vectors {
        let aes = Aes::new(&unhex(key)).unwrap();
        let mut block: [u8; 16] = plain.clone().try_into().unwrap();
        aes.encrypt_block(&mut block);
        assert_eq!(hex(&block), cipher, "key of {} bytes", key.len() / 2);
        aes.decrypt_block(&mut block);
        assert_eq!(block.to_vec(), plain);
    }
    assert!(Aes::new(&[0; 15]).is_none());
}

#[test]
fn cbc_matches_the_reference_and_decrypts_back() {
    // The length of the plain text, the SHA-256 of the cipher text and its last block.
    let vectors: &[(&str, &str, usize, &str, &str)] = &[
        (
            "151c232a31383f464d545b626970777e",
            "0910171e252c333a41484f565d646b72",
            16,
            "714eeec903d7931ae00a87a2daae34015c37681f635e64ceb7d67f0a883c49dc",
            "eadcc3b0a8f8e7c5eb64d842c9cc0cd8",
        ),
        (
            "151c232a31383f464d545b626970777e",
            "0910171e252c333a41484f565d646b72",
            80,
            "71eb3f7add2e076cd532a9d45ef0e53241bd971a7c411046d1bc0d63a21a1d24",
            "a6f31a324b7904158621fcdde19ddca7",
        ),
        (
            "151c232a31383f464d545b626970777e858c939aa1a8afb6bdc4cbd2d9e0e7ee",
            "0910171e252c333a41484f565d646b72",
            16,
            "f2a14e0bf2f95e2e9c18446806707c8dc9f31336da0d3bd57df6a5d40bfc444b",
            "13b95e5f1f7cbe8fed759c335698b42c",
        ),
        (
            "151c232a31383f464d545b626970777e858c939aa1a8afb6bdc4cbd2d9e0e7ee",
            "0910171e252c333a41484f565d646b72",
            144,
            "dd0b539f812952575013d2d272790d4c5bf1f7cf2700a0afa0b62716b17bdf0c",
            "80f46fcbdb287921d31bdacaf96d2371",
        ),
        (
            "151c232a31383f464d545b626970777e858c939aa1a8afb6bdc4cbd2d9e0e7ee",
            "0910171e252c333a41484f565d646b72",
            1024,
            "cd68fae273157e3e10bc52c1e3998e199a1b2a7b6522b124d72cb02ad1233f13",
            "36c6120c39527111176135df3219eb4e",
        ),
        (
            "151c232a31383f464d545b626970777e858c939aa1a8afb6",
            "0910171e252c333a41484f565d646b72",
            48,
            "68747174de0daeff62fa55710c2e0a07668c17bc6a8ff6229928eb05d45d33e5",
            "15d691ae3da7f28866f3be5c4d6946a5",
        ),
    ];
    for &(key, iv, length, digest, last) in vectors {
        let aes = Aes::new(&unhex(key)).unwrap();
        let iv: [u8; 16] = unhex(iv).try_into().unwrap();
        let plain = data(length, 2);
        let mut text = plain.clone();
        cbc_encrypt(&aes, &iv, &mut text);
        assert_eq!(
            hex(&Sha256::digest(&text)),
            digest,
            "key of {} bytes",
            key.len() / 2
        );
        assert_eq!(hex(&text[length - 16..]), last);
        cbc_decrypt(&aes, &iv, &mut text);
        assert_eq!(text, plain);
    }
}
