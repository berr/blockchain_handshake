pub fn encode_bytes(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose, Engine as _};
    general_purpose::STANDARD_NO_PAD.encode(bytes)
}

pub fn decode_bytes(bytes: &str) -> Vec<u8> {
    use base64::{engine::general_purpose, Engine as _};
    general_purpose::STANDARD_NO_PAD
        .decode(bytes)
        .expect("Only used in tests")
}
