//! Serde para `PathBuf` que preserva caminhos nao UTF-8. Caminho UTF-8 vira
//! string; o resto vira `"hex:<bytes em hex>"`. Um caminho UTF-8 que comeca
//! com `hex:` tambem e gravado em hex, para a leitura nunca ser ambigua.

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serializer};

const HEX_PREFIX: &str = "hex:";

pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&encode(path))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
    let text = String::deserialize(deserializer)?;
    decode(&text).map_err(serde::de::Error::custom)
}

pub fn encode(path: &Path) -> String {
    match path.to_str() {
        Some(text) if !text.starts_with(HEX_PREFIX) => text.to_string(),
        _ => format!("{HEX_PREFIX}{}", to_hex(path.as_os_str().as_bytes())),
    }
}

pub fn decode(text: &str) -> Result<PathBuf, String> {
    let Some(hex) = text.strip_prefix(HEX_PREFIX) else {
        return Ok(PathBuf::from(text));
    };
    let bytes = from_hex(hex).ok_or_else(|| format!("caminho em hex inválido: {text}"))?;
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn from_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || !hex.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(digits, 16).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn round_trip(path: &Path) -> PathBuf {
        decode(&encode(path)).unwrap()
    }

    #[test]
    fn utf8_path_is_plain_string() {
        assert_eq!(encode(Path::new("/home/eu/café")), "/home/eu/café");
    }

    #[test]
    fn non_utf8_path_is_hex() {
        let path = Path::new(OsStr::from_bytes(b"/x/caf\xe9"));
        assert_eq!(encode(path), "hex:2f782f636166e9");
        assert_eq!(round_trip(path), path);
    }

    #[test]
    fn literal_hex_prefix_is_escaped() {
        let path = Path::new("hex:41");
        assert!(encode(path).starts_with(HEX_PREFIX));
        assert_ne!(encode(path), "hex:41");
        assert_eq!(round_trip(path), path);
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(decode("hex:zz").is_err());
        assert!(decode("hex:abc").is_err());
        assert!(decode("hex:+f").is_err());
    }

    #[test]
    fn keeps_newlines_and_spaces() {
        let path = Path::new("/x/com espaco\ncom quebra");
        assert_eq!(round_trip(path), path);
    }

    #[test]
    fn hex_bytes_helper_is_lowercase() {
        assert_eq!(to_hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}
