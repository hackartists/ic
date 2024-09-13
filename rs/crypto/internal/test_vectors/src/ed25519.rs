//! Test vectors for Ed25519 signatures.
use crate::unhex::*;
use strum_macros::EnumIter;

pub fn crypto_lib_testvec(test_vec: Ed25519TestVector) -> ([u8; 32], [u8; 32], Vec<u8>, [u8; 64]) {
    match test_vec {
        Ed25519TestVector::RFC8032_ED25519_1 => {
            let sk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_1_SK);
            let pk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_1_PK);
            let msg = hex_to_byte_vec(TESTVEC_RFC8032_ED25519_1_MSG);
            let sig = hex_to_64_bytes(TESTVEC_RFC8032_ED25519_1_SIG);
            (sk, pk, msg, sig)
        }
        Ed25519TestVector::RFC8032_ED25519_2 => {
            let sk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_2_SK);
            let pk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_2_PK);
            let msg = hex_to_byte_vec(TESTVEC_RFC8032_ED25519_2_MSG);
            let sig = hex_to_64_bytes(TESTVEC_RFC8032_ED25519_2_SIG);
            (sk, pk, msg, sig)
        }
        Ed25519TestVector::RFC8032_ED25519_3 => {
            let sk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_3_SK);
            let pk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_3_PK);
            let msg = hex_to_byte_vec(TESTVEC_RFC8032_ED25519_3_MSG);
            let sig = hex_to_64_bytes(TESTVEC_RFC8032_ED25519_3_SIG);
            (sk, pk, msg, sig)
        }
        Ed25519TestVector::RFC8032_ED25519_1024 => {
            let sk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_1024_SK);
            let pk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_1024_PK);
            let msg = hex_to_byte_vec(TESTVEC_RFC8032_ED25519_1024_MSG);
            let sig = hex_to_64_bytes(TESTVEC_RFC8032_ED25519_1024_SIG);
            (sk, pk, msg, sig)
        }
        Ed25519TestVector::RFC8032_ED25519_SHA_ABC => {
            let sk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_SHA_ABC_SK);
            let pk = hex_to_32_bytes(TESTVEC_RFC8032_ED25519_SHA_ABC_PK);
            let msg = hex_to_byte_vec(TESTVEC_RFC8032_ED25519_SHA_ABC_MSG);
            let sig = hex_to_64_bytes(TESTVEC_RFC8032_ED25519_SHA_ABC_SIG);
            (sk, pk, msg, sig)
        }
        Ed25519TestVector::MESSAGE_LEN_256_BIT_STABILITY_1 => {
            let sk = hex_to_32_bytes(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_SK);
            let pk = hex_to_32_bytes(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_PK);
            let msg = hex_to_byte_vec(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_MSG);
            let sig = hex_to_64_bytes(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_SIG);
            (sk, pk, msg, sig)
        }
        Ed25519TestVector::MESSAGE_LEN_256_BIT_STABILITY_2 => {
            let sk = hex_to_32_bytes(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_SK);
            let pk = hex_to_32_bytes(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_PK);
            let msg = hex_to_byte_vec(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_MSG);
            let sig = hex_to_64_bytes(TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_SIG);
            (sk, pk, msg, sig)
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, EnumIter)]
pub enum Ed25519TestVector {
    RFC8032_ED25519_1,
    RFC8032_ED25519_2,
    RFC8032_ED25519_3,
    RFC8032_ED25519_1024,
    RFC8032_ED25519_SHA_ABC,
    MESSAGE_LEN_256_BIT_STABILITY_1,
    MESSAGE_LEN_256_BIT_STABILITY_2,
}

// See TEST 1 in https://tools.ietf.org/html/rfc8032#section-7.1
pub const TESTVEC_RFC8032_ED25519_1_SK: &str = "9d61b19deffd5a60ba844af492ec2cc4\
                                                4449c5697b326919703bac031cae7f60";
pub const TESTVEC_RFC8032_ED25519_1_PK: &str = "d75a980182b10ab7d54bfed3c964073a\
                                                0ee172f3daa62325af021a68f707511a";
pub const TESTVEC_RFC8032_ED25519_1_MSG: &str = "";
pub const TESTVEC_RFC8032_ED25519_1_SIG: &str = "e5564300c360ac729086e2cc806e828a\
                                                 84877f1eb8e5d974d873e06522490155\
                                                 5fb8821590a33bacc61e39701cf9b46b\
                                                 d25bf5f0595bbe24655141438e7a100b";

// See TEST 2 in https://tools.ietf.org/html/rfc8032#section-7.1
pub const TESTVEC_RFC8032_ED25519_2_SK: &str = "4ccd089b28ff96da9db6c346ec114e0f\
                                                5b8a319f35aba624da8cf6ed4fb8a6fb";
pub const TESTVEC_RFC8032_ED25519_2_PK: &str = "3d4017c3e843895a92b70aa74d1b7ebc\
                                                9c982ccf2ec4968cc0cd55f12af4660c";
pub const TESTVEC_RFC8032_ED25519_2_MSG: &str = "72";
pub const TESTVEC_RFC8032_ED25519_2_SIG: &str = "92a009a9f0d4cab8720e820b5f642540\
                                                 a2b27b5416503f8fb3762223ebdb69da\
                                                 085ac1e43e15996e458f3613d0f11d8c\
                                                 387b2eaeb4302aeeb00d291612bb0c00";

// See TEST 3 in https://tools.ietf.org/html/rfc8032#section-7.1
pub const TESTVEC_RFC8032_ED25519_3_SK: &str = "c5aa8df43f9f837bedb7442f31dcb7b1\
                                                66d38535076f094b85ce3a2e0b4458f7";
pub const TESTVEC_RFC8032_ED25519_3_PK: &str = "fc51cd8e6218a1a38da47ed00230f058\
                                                0816ed13ba3303ac5deb911548908025";
pub const TESTVEC_RFC8032_ED25519_3_MSG: &str = "af82";
pub const TESTVEC_RFC8032_ED25519_3_SIG: &str = "6291d657deec24024827e69c3abe01a3\
                                                 0ce548a284743a445e3680d7db5ac3ac\
                                                 18ff9b538d16f290ae67f760984dc659\
                                                 4a7c15e9716ed28dc027beceea1ec40a";

// See TEST 1024 in https://tools.ietf.org/html/rfc8032#section-7.1
pub const TESTVEC_RFC8032_ED25519_1024_SK: &str = "f5e5767cf153319517630f226876b86c\
                                                   8160cc583bc013744c6bf255f5cc0ee5";
pub const TESTVEC_RFC8032_ED25519_1024_PK: &str = "278117fc144c72340f67d0f2316e8386\
                                                   ceffbf2b2428c9c51fef7c597f1d426e";
pub const TESTVEC_RFC8032_ED25519_1024_MSG: &str = "08b8b2b733424243760fe426a4b54908\
                                                    632110a66c2f6591eabd3345e3e4eb98\
                                                    fa6e264bf09efe12ee50f8f54e9f77b1\
                                                    e355f6c50544e23fb1433ddf73be84d8\
                                                    79de7c0046dc4996d9e773f4bc9efe57\
                                                    38829adb26c81b37c93a1b270b20329d\
                                                    658675fc6ea534e0810a4432826bf58c\
                                                    941efb65d57a338bbd2e26640f89ffbc\
                                                    1a858efcb8550ee3a5e1998bd177e93a\
                                                    7363c344fe6b199ee5d02e82d522c4fe\
                                                    ba15452f80288a821a579116ec6dad2b\
                                                    3b310da903401aa62100ab5d1a36553e\
                                                    06203b33890cc9b832f79ef80560ccb9\
                                                    a39ce767967ed628c6ad573cb116dbef\
                                                    efd75499da96bd68a8a97b928a8bbc10\
                                                    3b6621fcde2beca1231d206be6cd9ec7\
                                                    aff6f6c94fcd7204ed3455c68c83f4a4\
                                                    1da4af2b74ef5c53f1d8ac70bdcb7ed1\
                                                    85ce81bd84359d44254d95629e9855a9\
                                                    4a7c1958d1f8ada5d0532ed8a5aa3fb2\
                                                    d17ba70eb6248e594e1a2297acbbb39d\
                                                    502f1a8c6eb6f1ce22b3de1a1f40cc24\
                                                    554119a831a9aad6079cad88425de6bd\
                                                    e1a9187ebb6092cf67bf2b13fd65f270\
                                                    88d78b7e883c8759d2c4f5c65adb7553\
                                                    878ad575f9fad878e80a0c9ba63bcbcc\
                                                    2732e69485bbc9c90bfbd62481d9089b\
                                                    eccf80cfe2df16a2cf65bd92dd597b07\
                                                    07e0917af48bbb75fed413d238f5555a\
                                                    7a569d80c3414a8d0859dc65a46128ba\
                                                    b27af87a71314f318c782b23ebfe808b\
                                                    82b0ce26401d2e22f04d83d1255dc51a\
                                                    ddd3b75a2b1ae0784504df543af8969b\
                                                    e3ea7082ff7fc9888c144da2af58429e\
                                                    c96031dbcad3dad9af0dcbaaaf268cb8\
                                                    fcffead94f3c7ca495e056a9b47acdb7\
                                                    51fb73e666c6c655ade8297297d07ad1\
                                                    ba5e43f1bca32301651339e22904cc8c\
                                                    42f58c30c04aafdb038dda0847dd988d\
                                                    cda6f3bfd15c4b4c4525004aa06eeff8\
                                                    ca61783aacec57fb3d1f92b0fe2fd1a8\
                                                    5f6724517b65e614ad6808d6f6ee34df\
                                                    f7310fdc82aebfd904b01e1dc54b2927\
                                                    094b2db68d6f903b68401adebf5a7e08\
                                                    d78ff4ef5d63653a65040cf9bfd4aca7\
                                                    984a74d37145986780fc0b16ac451649\
                                                    de6188a7dbdf191f64b5fc5e2ab47b57\
                                                    f7f7276cd419c17a3ca8e1b939ae49e4\
                                                    88acba6b965610b5480109c8b17b80e1\
                                                    b7b750dfc7598d5d5011fd2dcc5600a3\
                                                    2ef5b52a1ecc820e308aa342721aac09\
                                                    43bf6686b64b2579376504ccc493d97e\
                                                    6aed3fb0f9cd71a43dd497f01f17c0e2\
                                                    cb3797aa2a2f256656168e6c496afc5f\
                                                    b93246f6b1116398a346f1a641f3b041\
                                                    e989f7914f90cc2c7fff357876e506b5\
                                                    0d334ba77c225bc307ba537152f3f161\
                                                    0e4eafe595f6d9d90d11faa933a15ef1\
                                                    369546868a7f3a45a96768d40fd9d034\
                                                    12c091c6315cf4fde7cb68606937380d\
                                                    b2eaaa707b4c4185c32eddcdd306705e\
                                                    4dc1ffc872eeee475a64dfac86aba41c\
                                                    0618983f8741c5ef68d3a101e8a3b8ca\
                                                    c60c905c15fc910840b94c00a0b9d0";
pub const TESTVEC_RFC8032_ED25519_1024_SIG: &str = "0aab4c900501b3e24d7cdf4663326a3a\
                                                    87df5e4843b2cbdb67cbf6e460fec350\
                                                    aa5371b1508f9f4528ecea23c436d94b\
                                                    5e8fcd4f681e30a6ac00a9704a188a03";

// See TEST SHA(abc) in https://tools.ietf.org/html/rfc8032#section-7.1
pub const TESTVEC_RFC8032_ED25519_SHA_ABC_SK: &str = "833fe62409237b9d62ec77587520911e\
                                                      9a759cec1d19755b7da901b96dca3d42";
pub const TESTVEC_RFC8032_ED25519_SHA_ABC_PK: &str = "ec172b93ad5e563bf4932c70e1245034\
                                                      c35467ef2efd4d64ebf819683467e2bf";
pub const TESTVEC_RFC8032_ED25519_SHA_ABC_MSG: &str = "ddaf35a193617abacc417349ae204131\
                                                       12e6fa4e89a97ea20a9eeee64b55d39a\
                                                       2192992a274fc1a836ba3c23a3feebbd\
                                                       454d4423643ce80e2a9ac94fa54ca49f";
pub const TESTVEC_RFC8032_ED25519_SHA_ABC_SIG: &str = "dc2a4459e7369633a52b1bf277839a00\
                                                       201009a3efbf3ecb69bea2186c26b589\
                                                       09351fc9ac90b3ecfdfbc7c66431e030\
                                                       3dca179c138ac17ad9bef1177331a704";

// Custom test vector with a message=SHA256(abc) of length 256 bit
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_SK: &str = "a3f57690c381c6324cd5ed06c9b0c73f\
                                                              e620cdffd3b83103179f52a853eefda7";
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_PK: &str = "e286bfdb6e9aeb3c082435e9c669e800\
                                                              9e98042d4e71f36e8ae5c501a9100150";
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_MSG: &str = "ba7816bf8f01cfea414140de5dae2223\
                                                               b00361a396177a9cb410ff61f20015ad";
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_1_SIG: &str = "41e7c78cc44128adcf80dbacbd1ba13e\
                                                               b324a9ca4da78d1aa1d28e3b8a35bda0\
                                                               4acf86757bd2d24f081ab795e8f52e6f\
                                                               084a8f84a0a73680d9af619ca6e59b02";

// Custom test vector with a message=SHA256(xyz) of length 256 bit
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_SK: &str = "12aca40167fbdd1a11037e9fd440e3d5\
                                                              10d9d9dea70a6646aa4aaf84d718d75a";
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_PK: &str = "acdd6d5b53bfee478bf689f8e012fe79\
                                                              88bf755e3d7c5152947abc149bc20189";
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_MSG: &str = "3608bca1e44ea6c4d268eb6db0226026\
                                                               9892c0b42b86bbf1e77a6fa16c3c9282";
pub const TESTVEC_MESSAGE_LEN_256_BIT_STABILITY_2_SIG: &str = "3b3a199d7a3440a888780bc0289451c9\
                                                               ce835bc7d9712bbdec62e4ea082a1c27\
                                                               e7ab2ca8a7169ec9c42866ecbb849f7d\
                                                               f78e469e7ce28f193e22f3eecb936207";

// Signature-only test vectors, for Ed25519
pub const TESTVEC_ED25519_STABILITY_1_SIG: &str = "957465c1edc2d55f9eb8773af5885cd309580ca1a\
                                                   7dfdd9be223ee7fa7dc1eb584d3a516a450c51df8\
                                                   cf1e5ccd471a6af136cb82531f14ab0baedc68db3\
                                                   7ef07";

pub const TESTVEC_ED25519_STABILITY_2_SIG: &str = "dd8b5455bf35654337220ebfc9e22c98134e2cb81\
                                                   ce759526da3d60e2df6fd9317ac4257dfe65828ac\
                                                   5812b15335ecaab640c4f4e2e3f0fdf155c1e19f9\
                                                   c7e09";
