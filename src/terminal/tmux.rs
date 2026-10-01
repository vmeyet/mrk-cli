/// `sequence` handed through tmux to the terminal outside it, its ESCs doubled; tmux drops it unless
/// `allow-passthrough` is on.
pub fn passthrough(sequence: &str) -> String {
    format!("\x1bPtmux;{}\x1b\\", sequence.replace('\x1b', "\x1b\x1b"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_esc_inside_is_doubled() {
        assert_eq!(passthrough("\x1b_Ga=q;AAAA\x1b\\"), "\x1bPtmux;\x1b\x1b_Ga=q;AAAA\x1b\x1b\\\x1b\\");
    }
}
