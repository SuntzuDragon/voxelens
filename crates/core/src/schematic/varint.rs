//! Unsigned LEB128 ("varint") encoding.
//!
//! Used by the Sponge schematic `BlockData` array (and the Minecraft protocol):
//! a non-negative integer split into 7-bit little-endian groups, each byte's
//! high bit signalling "more bytes follow". A `u32` needs at most 5 bytes.

/// Append the unsigned LEB128 encoding of `value` to `out`.
pub fn write_unsigned(value: u32, out: &mut Vec<u8>) {
    let mut v = value;
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Error decoding a varint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarintError {
    /// The input ended in the middle of a value (continuation bit set on the
    /// final available byte).
    Truncated,
    /// The value did not terminate within the 5 bytes a `u32` allows.
    Overflow,
}

impl std::fmt::Display for VarintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VarintError::Truncated => write!(f, "varint truncated"),
            VarintError::Overflow => write!(f, "varint overflows u32"),
        }
    }
}

impl std::error::Error for VarintError {}

/// Decode a single varint from the front of `input`, returning the decoded
/// value and the number of bytes it consumed.
pub fn read_unsigned(input: &[u8]) -> Result<(u32, usize), VarintError> {
    let mut result: u32 = 0;
    for (i, &b) in input.iter().take(5).enumerate() {
        // The 5th byte may only contribute the top 4 bits of a u32.
        if i == 4 && (b & 0x7f) > 0x0f {
            return Err(VarintError::Overflow);
        }
        result |= ((b & 0x7f) as u32) << (7 * i as u32);
        if b & 0x80 == 0 {
            return Ok((result, i + 1));
        }
    }
    if input.len() >= 5 {
        Err(VarintError::Overflow)
    } else {
        Err(VarintError::Truncated)
    }
}

/// Decode every varint in `input` (which must contain only complete varints).
pub fn read_all_unsigned(mut input: &[u8]) -> Result<Vec<u32>, VarintError> {
    let mut out = Vec::new();
    while !input.is_empty() {
        let (value, consumed) = read_unsigned(input)?;
        out.push(value);
        input = &input[consumed..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(v: u32) -> Vec<u8> {
        let mut o = Vec::new();
        write_unsigned(v, &mut o);
        o
    }

    #[test]
    fn writes_known_values() {
        assert_eq!(enc(0), [0x00]);
        assert_eq!(enc(1), [0x01]);
        assert_eq!(enc(127), [0x7f]);
        assert_eq!(enc(128), [0x80, 0x01]);
        assert_eq!(enc(255), [0xff, 0x01]);
        assert_eq!(enc(300), [0xac, 0x02]);
        assert_eq!(enc(16384), [0x80, 0x80, 0x01]);
        assert_eq!(enc(2_097_151), [0xff, 0xff, 0x7f]);
        assert_eq!(enc(u32::MAX), [0xff, 0xff, 0xff, 0xff, 0x0f]);
    }

    #[test]
    fn round_trips() {
        for v in [
            0,
            1,
            2,
            127,
            128,
            255,
            256,
            300,
            16_384,
            2_097_151,
            16_777_215,
            u32::MAX / 2,
            u32::MAX,
        ] {
            let bytes = enc(v);
            assert_eq!(
                read_unsigned(&bytes).unwrap(),
                (v, bytes.len()),
                "value {v}"
            );
        }
    }

    #[test]
    fn reads_a_sequence() {
        let mut bytes = Vec::new();
        for v in [1u32, 300, 2, 128] {
            write_unsigned(v, &mut bytes);
        }
        assert_eq!(read_all_unsigned(&bytes).unwrap(), vec![1, 300, 2, 128]);
        // Consumes a trailing partial value as an error, never silently.
        bytes.push(0x80);
        assert_eq!(read_all_unsigned(&bytes), Err(VarintError::Truncated));
    }

    #[test]
    fn rejects_truncated_and_overflow() {
        assert_eq!(read_unsigned(&[]), Err(VarintError::Truncated));
        assert_eq!(read_unsigned(&[0x80]), Err(VarintError::Truncated));
        // Five continuation bytes never terminate.
        assert_eq!(
            read_unsigned(&[0xff, 0xff, 0xff, 0xff, 0xff]),
            Err(VarintError::Overflow)
        );
        // Fifth byte carries more than the top 4 bits.
        assert_eq!(
            read_unsigned(&[0xff, 0xff, 0xff, 0xff, 0x1f]),
            Err(VarintError::Overflow)
        );
    }
}
