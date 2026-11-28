//! Fuzzy name matching: the typed letters in order, not necessarily adjacent.

// score {{{
/// Higher is better; `None` when `typed`'s letters are not all in `name` in order. Case and
/// spaces in `typed` are ignored.
pub fn score(typed: &str, name: &str) -> Option<i32> {
    let wanted: Vec<char> = typed
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if wanted.is_empty() {
        return Some(0);
    }
    let letters: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    let mut score = 0;
    let mut next = 0;
    let mut last: Option<usize> = None;
    for (i, c) in letters.iter().enumerate() {
        if next == wanted.len() {
            break;
        }
        if *c != wanted[next] {
            continue;
        }
        // one after another, the start of a word, the very start, and the less skipped the better
        let starts_word = i == 0 || !letters[i - 1].is_alphanumeric();
        score += match last {
            Some(at) if at + 1 == i => 8,
            Some(at) => -((i - at - 1).min(5) as i32),
            None => -(i.min(10) as i32),
        };
        if starts_word {
            score += 6;
        }
        if i == 0 {
            score += 4;
        }
        last = Some(i);
        next += 1;
    }
    // shorter names, where more of the name was typed, come first
    (next == wanted.len()).then(|| score * 4 - letters.len() as i32)
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_letters_typed_in_order_match() {
        assert!(score("dsg", "Design").is_some());
        assert!(score("DESIGN", "design").is_some());
        assert!(score("de sign", "Design").is_some());
        assert!(score("", "anything").is_some());
        assert!(score("gd", "Design").is_none());
        assert!(score("designs", "Design").is_none());
    }

    #[test]
    fn together_and_at_the_start_of_words_ranks_first() {
        let rank = |typed: &str, a: &str, b: &str| score(typed, a) > score(typed, b);
        assert!(rank("des", "Design", "Ordered sets"));
        assert!(rank("fb", "Fix bug", "Fabric"));
        assert!(rank("build", "Build", "Rebuild the site"));
        assert!(rank("api", "API docs", "Rapid iteration"));
    }
}
// }}}
