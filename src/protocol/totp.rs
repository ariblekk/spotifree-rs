use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Decode Spotify's obfuscated web-player TOTP secret.
///
/// Each character is XORed with `(index % 33) + 9` and the result is written
/// as a decimal string, which is then used as ASCII key bytes. This mirrors
/// the browser code rather than packing the values into a byte array.
pub fn decode_secret(secret: &str) -> Vec<u8> {
    let digits: String = secret
        .chars()
        .enumerate()
        .map(|(index, ch)| ((ch as u32) ^ ((index % 33 + 9) as u32)).to_string())
        .collect();

    digits.into_bytes()
}

/// Generate a standard SHA-1 time-based one-time password.
pub fn generate_totp(secret: &[u8], timestamp: u64) -> String {
    let counter = timestamp / 30;

    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC accepts keys of any size");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();

    let offset = (digest[19] & 0x0f) as usize;
    let code = ((digest[offset] as u32 & 0x7f) << 24)
        | ((digest[offset + 1] as u32) << 16)
        | ((digest[offset + 2] as u32) << 8)
        | (digest[offset + 3] as u32);

    format!("{:06}", code % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_secret_to_decimal_ascii() {
        // 'A' (65) ^ 9 = 72, encoded as the ASCII text "72".
        assert_eq!(decode_secret("A"), b"72".to_vec());
    }

    #[test]
    fn totp_is_six_digits() {
        let code = generate_totp(b"12345678901234567890", 59);
        assert_eq!(code.len(), 6);
        assert_eq!(code, "287082");
    }
}
