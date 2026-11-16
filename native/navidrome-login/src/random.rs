// Random tokens, salts and ids. The operating system is the only source: a
// guessable token would let anything on this machine open the page.

/// Fills `out` from the operating system, or ends the process.
///
/// There is no weaker fallback on purpose: a clock-seeded token is no guard.
pub fn fill(out: &mut [u8]) {
    if let Err(error) = getrandom::fill(out) {
        eprintln!("navidrome-login: cannot read random bytes: {error}");
        std::process::exit(1);
    }
}

/// `count` characters drawn evenly from `alphabet`.
pub fn pick(alphabet: &[u8], count: usize) -> String {
    assert!(!alphabet.is_empty(), "an alphabet needs at least one character");

    // Bytes at or above `limit` would favour the first characters of the
    // alphabet, so they are drawn again instead.
    let limit = 256 - (256 % alphabet.len());
    let mut out = String::with_capacity(count);
    let mut bytes = [0u8; 16];
    while out.len() < count {
        fill(&mut bytes);
        for byte in bytes {
            if out.len() == count {
                break;
            }
            if usize::from(byte) < limit {
                out.push(alphabet[usize::from(byte) % alphabet.len()] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

    #[test]
    fn picks_the_asked_for_length() {
        for count in [0, 1, 10, 32] {
            assert_eq!(pick(CHARS, count).len(), count);
        }
    }

    #[test]
    fn picks_only_from_the_alphabet() {
        for character in pick(CHARS, 512).chars() {
            assert!(CHARS.contains(&(character as u8)), "{character} is not in the alphabet");
        }
    }

    #[test]
    fn two_draws_differ() {
        assert_ne!(pick(CHARS, 32), pick(CHARS, 32));
    }

    #[test]
    fn odd_alphabet_sizes_are_handled() {
        // 3 does not divide 256; the tail of the byte range is skipped.
        let drawn = pick(b"abc", 300);
        assert_eq!(drawn.len(), 300);
        assert!(drawn.chars().all(|character| "abc".contains(character)));
    }
}
