//! Quiet spellcheck. No dictionary crate and no network.
//!
//! A word is a misspelling when it is a known slip, or two adjacent letters
//! of a common word swapped. Invented names and ordinary words stay clean.

/// Byte ranges of misspellings in `text`.
#[must_use]
pub fn misspelled_bytes(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut word = String::new();
    let mut start = 0usize;
    let mut byte = 0usize;
    for ch in text.chars() {
        let len = ch.len_utf8();
        if ch.is_alphabetic() {
            if word.is_empty() {
                start = byte;
            }
            word.push(ch);
        } else if !word.is_empty() {
            if slip(&word) {
                found.push((start, byte));
            }
            word.clear();
        }
        byte = byte.saturating_add(len);
    }
    if !word.is_empty() && slip(&word) {
        found.push((start, byte));
    }
    found
}

fn slip(word: &str) -> bool {
    if word.chars().any(char::is_uppercase) {
        return false;
    }
    SLIPS.contains(&word) || WORDS.iter().any(|known| swapped(word, known))
}

fn swapped(word: &str, known: &str) -> bool {
    let left: Vec<char> = word.chars().collect();
    let right: Vec<char> = known.chars().collect();
    if left.len() != right.len() || left.len() < 2 {
        return false;
    }
    let mut index = 0usize;
    while index.saturating_add(1) < left.len() {
        let next = index.saturating_add(1);
        if left[index] == right[next] && left[next] == right[index] && left[index] != left[next] {
            let mut same = true;
            let mut cursor = 0usize;
            while cursor < left.len() {
                if cursor != index && cursor != next && left[cursor] != right[cursor] {
                    same = false;
                    break;
                }
                cursor = cursor.saturating_add(1);
            }
            if same {
                return true;
            }
        }
        index = next;
    }
    false
}

const SLIPS: &[&str] = &["teh", "adn", "taht", "waht", "recieve", "seperate"];

const WORDS: &[&str] = &[
    "the", "and", "that", "what", "which", "their", "there", "word", "receive", "separate",
    "would", "could", "should", "about", "write", "first", "people", "because", "through",
];

#[cfg(test)]
mod tests {
    use super::misspelled_bytes;

    #[test]
    fn a_swap_and_a_known_slip_are_marked_and_a_name_is_not() {
        let text = "teh recieve Mira cat";
        let marks = misspelled_bytes(text);
        assert_eq!(marks.len(), 2);
        assert_eq!(&text[marks[0].0..marks[0].1], "teh");
        assert_eq!(&text[marks[1].0..marks[1].1], "recieve");
    }
}
