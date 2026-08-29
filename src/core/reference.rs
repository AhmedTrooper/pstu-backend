use rand::RngExt;

const CROCKFORD_CHARS: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub fn generate_trx_reference() -> String {
    generate_reference("TRX", 10)
}

pub fn generate_funding_reference() -> String {
    generate_reference("FND", 10)
}

pub fn generate_reference(prefix: &str, length: usize) -> String {
    let mut bytes = vec![0u8; length];
    rand::rng().fill(&mut bytes[..]);
    for b in bytes.iter_mut() {
        let idx = (*b as usize) % CROCKFORD_CHARS.len();
        *b = CROCKFORD_CHARS[idx];
    }
    format!("{}{}", prefix, String::from_utf8(bytes).unwrap())
}
