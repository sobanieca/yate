//! The emoji the emoji picker offers: `emoji.txt`, made from the Unicode
//! `emoji-test.txt` (fully-qualified emoji up to Emoji 15.1, which fonts
//! and `unicode-width` know, without the skin tone variants). A line
//! without a tab starts a group; the others are the emoji, a tab and its
//! name.

const LIST: &str = include_str!("emoji.txt");

pub struct Emoji {
    pub group: &'static str,
    pub symbol: &'static str,
    pub name: &'static str,
}

pub fn all() -> Vec<Emoji> {
    let mut group = "";
    let mut list = Vec::new();
    for line in LIST.lines() {
        match line.split_once('\t') {
            Some((symbol, name)) => list.push(Emoji {
                group,
                symbol,
                name,
            }),
            None => group = line,
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text;
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn every_emoji_is_one_grapheme_two_columns_wide() {
        let list = all();
        assert!(list.len() > 1_500);
        for emoji in &list {
            assert!(!emoji.group.is_empty(), "{} has no group", emoji.name);
            assert_eq!(emoji.symbol.graphemes(true).count(), 1, "{}", emoji.name);
            assert_eq!(
                text::str_width(emoji.symbol),
                2,
                "{} {}",
                emoji.symbol,
                emoji.name
            );
        }
    }
}
