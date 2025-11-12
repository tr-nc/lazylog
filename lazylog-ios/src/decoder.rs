/// Decodes a vis-encoded syslog string to a UTF-8 representation.
/// https://gist.github.com/cbracken/d88a84370fdde9cbcfd810d944c8f540
///
/// Apple's syslog logs are encoded in 7-bit form. Input bytes are encoded as follows:
/// 1. 0x00 to 0x19: non-printing range. Some ignored, some encoded as <...>.
/// 2. 0x20 to 0x7f: as-is, with the exception of 0x5c (backslash).
/// 3. 0x5c (backslash): octal representation \134.
/// 4. 0x80 to 0x9f: \M^x (using control-character notation for range 0x00 to 0x40).
/// 5. 0xa0: octal representation \240.
/// 6. 0xa1 to 0xf7: \M-x (where x is the input byte stripped of its high-order bit).
/// 7. 0xf8 to 0xff: unused in 4-byte UTF-8.
///
/// See: [vis(3) manpage](https://www.freebsd.org/cgi/man.cgi?query=vis&sektion=3)
pub fn decode_syslog(line: &str) -> String {
    // UTF-8 values for \, M, -, ^.
    const BACKSLASH: u8 = 0x5c;
    const M: u8 = 0x4d;
    const DASH: u8 = 0x2d;
    const CARET: u8 = 0x5e;

    // Mask for the UTF-8 digit range.
    const NUM: u8 = 0x30;

    // returns true when `byte` is within the UTF-8 7-bit digit range (0x30 to 0x39).
    fn is_digit(byte: u8) -> bool {
        (byte & 0xf0) == NUM
    }

    // converts a three-digit ASCII (UTF-8) representation of an octal number `xyz` to an integer.
    fn decode_octal(x: u8, y: u8, z: u8) -> u8 {
        ((x & 0x3) << 6) | ((y & 0x7) << 3) | (z & 0x7)
    }

    let bytes = line.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != BACKSLASH || i > bytes.len() - 4 {
            // unmapped byte: copy as-is.
            out.push(bytes[i]);
            i += 1;
        } else {
            // mapped byte: decode next 4 bytes.
            if bytes[i + 1] == M && bytes[i + 2] == CARET {
                // \M^x form: bytes in range 0x80 to 0x9f.
                out.push((bytes[i + 3] & 0x7f) + 0x40);
                i += 4;
            } else if bytes[i + 1] == M && bytes[i + 2] == DASH {
                // \M-x form: bytes in range 0xa0 to 0xf7.
                out.push(bytes[i + 3] | 0x80);
                i += 4;
            } else if is_digit(bytes[i + 1]) && is_digit(bytes[i + 2]) && is_digit(bytes[i + 3]) {
                // \ddd form: octal representation (only used for \134 and \240).
                out.push(decode_octal(bytes[i + 1], bytes[i + 2], bytes[i + 3]));
                i += 4;
            } else {
                // unknown form: copy as-is.
                out.push(bytes[i]);
                i += 1;
            }
        }
    }

    // attempt to decode as UTF-8, fallback to original string if it fails
    String::from_utf8(out).unwrap_or_else(|_| line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_syslog() {
        let input = r"I \M-b\M^]\M-$\M-o\M-8\M^O syslog \M-B\M-/\134_(\M-c\M^C\M^D)_/\M-B\M-/ \M-l\M^F\240!";
        let expected = "I ❤️ syslog ¯\\_(ツ)_/¯ 솠!";
        let result = decode_syslog(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_decode_syslog_no_encoding() {
        let input = "This is a normal log line";
        let result = decode_syslog(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_decode_syslog_backslash() {
        let input = r"test\134backslash";
        let expected = r"test\backslash";
        let result = decode_syslog(input);
        assert_eq!(result, expected);
    }
}
