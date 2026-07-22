/// Extension de `str` pour découper une chaîne à partir d'une liste de positions.
pub trait StrExt {
    /// Découpe la chaîne en tronçons aux positions données (en nombre de
    /// caractères, pas d'octets). Les positions au-delà de la longueur de la
    /// chaîne sont ramenées à la fin de la chaîne.
    ///
    /// # Examples
    ///
    /// ```
    /// use legal_act::str_ext::StrExt;
    ///
    /// let parts = "hello world".split_at_positions(&[5, 8]);
    /// assert_eq!(parts, vec!["hello", " wo", "rld"]);
    /// ```
    fn split_at_positions(&self, positions: &[usize]) -> Vec<&str>;
}

impl StrExt for str {
    fn split_at_positions(&self, positions: &[usize]) -> Vec<&str> {
        let mut byte_offsets: Vec<usize> = positions.iter()
            .map(|&pos| self.char_indices().nth(pos).map(|(i, _)| i).unwrap_or(self.len()))
            .collect();

        byte_offsets.sort_unstable();
        byte_offsets.dedup();

        let mut parts = Vec::with_capacity(byte_offsets.len() + 1);
        let mut start = 0;

        for end in byte_offsets {
            parts.push(&self[start..end]);
            start = end;
        }

        parts.push(&self[start..]);
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::StrExt;

    #[test]
    fn splits_at_given_positions() {
        assert_eq!("hello world".split_at_positions(&[5, 8]), vec!["hello", " wo", "rld"]);
    }

    #[test]
    fn returns_whole_string_without_positions() {
        assert_eq!("hello".split_at_positions(&[]), vec!["hello"]);
    }

    #[test]
    fn ignores_out_of_bounds_positions() {
        assert_eq!("hi".split_at_positions(&[10]), vec!["hi", ""]);
    }

    #[test]
    fn sorts_and_dedups_unordered_positions() {
        assert_eq!("hello".split_at_positions(&[3, 1, 3]), vec!["h", "el", "lo"]);
    }

    #[test]
    fn splits_on_char_boundaries_with_multibyte_chars() {
        assert_eq!("héllo".split_at_positions(&[2]), vec!["hé", "llo"]);
    }
}
