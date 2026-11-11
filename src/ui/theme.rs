//! Themes: a palette of named colors used by the views, and the styles of
//! tree-sitter captures, which refer to palette colors by name. The default
//! is Tokyo Night storm, matching the tmux and terminal colors in env-setup;
//! `yate.json` picks another preset and overrides colors and syntax styles.

use super::surface::{Rgb, Style};
use crate::json::Json;
use crate::syntax::languages::LanguageConfig;
use std::collections::HashMap;

macro_rules! palette {
    ($($field:ident),* $(,)?) => {
        #[derive(Clone, Debug)]
        pub struct Theme {
            pub name: String,
            $(pub $field: Rgb,)*
            syntax: HashMap<String, StyleSpec>,
        }

        /// The color names of the palette, as written in `yate.json`.
        pub const COLOR_NAMES: &[&str] = &[$(stringify!($field)),*];

        impl Theme {
            pub fn color(&self, name: &str) -> Option<Rgb> {
                match name {
                    $(stringify!($field) => Some(self.$field),)*
                    _ => None,
                }
            }

            fn set_color(&mut self, name: &str, color: Rgb) -> bool {
                match name {
                    $(stringify!($field) => self.$field = color,)*
                    _ => return false,
                }
                true
            }
        }
    };
}

palette!(
    bg,
    bg_dark,
    bg_inactive,
    bg_highlight,
    bg_visual,
    bg_search,
    fg,
    fg_dark,
    fg_gutter,
    dark3,
    dark5,
    comment,
    blue,
    blue1,
    blue5,
    blue6,
    cyan,
    magenta,
    purple,
    orange,
    yellow,
    green,
    green1,
    red,
    red1,
    git_add,
    git_change,
    git_delete,
);

pub const PRESETS: &[&str] = &[
    "tokyonight-storm",
    "tokyonight-night",
    "tokyonight-moon",
    "tokyonight-day",
    "gruvbox-dark",
    "gruvbox-light",
    "nord",
    "solarized-light",
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum ColorRef {
    Named(&'static str),
    Rgb(Rgb),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct StyleSpec {
    fg: Option<ColorRef>,
    bg: Option<ColorRef>,
    bold: bool,
    italic: bool,
    underline: bool,
}

/// Default capture styles: scopes (separated by `|`), palette color, flags.
const DEFAULT_SYNTAX: &[(&str, &str, &str)] = &[
    ("comment", "comment", "italic"),
    ("comment.documentation", "dark5", "italic"),
    ("keyword", "purple", "italic"),
    (
        "keyword.function|keyword.return|keyword.conditional|keyword.repeat|keyword.import|keyword.exception|keyword.coroutine|conditional|repeat|include|exception|constructor|string.escape|escape",
        "magenta",
        "",
    ),
    (
        "keyword.operator|operator|punctuation|punctuation.delimiter|punctuation.special|tag.delimiter",
        "blue5",
        "",
    ),
    (
        "keyword.directive|preproc|module|namespace|text.literal|markup.raw",
        "cyan",
        "",
    ),
    (
        "function|function.call|function.method|method|label",
        "blue",
        "",
    ),
    (
        "function.builtin|function.macro|type|type.builtin|type.definition|special",
        "blue1",
        "",
    ),
    ("storageclass|type.qualifier", "purple", ""),
    ("string|character", "green", ""),
    ("string.special|string.regexp|string.regex", "blue6", ""),
    (
        "number|float|number.float|boolean|constant|constant.builtin|markup.list|punctuation.list_marker",
        "orange",
        "",
    ),
    ("variable|embedded", "fg", ""),
    ("variable.builtin|tag", "red", ""),
    ("variable.parameter|parameter|attribute", "yellow", ""),
    ("variable.member|property|field|tag.attribute", "green1", ""),
    ("punctuation.bracket", "fg_dark", ""),
    ("text.title|markup.heading|title", "blue", "bold"),
    (
        "text.uri|markup.link.url|markup.link|text.reference",
        "green1",
        "underline",
    ),
    ("text.emphasis|markup.italic", "", "italic"),
    ("text.strong|markup.bold", "", "bold"),
    ("markup.quote", "dark5", "italic"),
    ("diff.plus|text.diff.add", "git_add", ""),
    ("diff.minus|text.diff.delete|error", "red1", ""),
    ("diff.delta", "git_change", ""),
];

fn rgb(value: u32) -> Rgb {
    Rgb::hex(value)
}

fn storm() -> Theme {
    let mut theme = Theme {
        name: "tokyonight-storm".to_string(),
        bg: rgb(0x24283b),
        bg_dark: rgb(0x1f2335),
        bg_inactive: rgb(0x16161e),
        bg_highlight: rgb(0x292e42),
        bg_visual: rgb(0x364a82),
        bg_search: rgb(0x3d59a1),
        fg: rgb(0xc0caf5),
        fg_dark: rgb(0xa9b1d6),
        fg_gutter: rgb(0x3b4261),
        dark3: rgb(0x545c7e),
        dark5: rgb(0x737aa2),
        comment: rgb(0x565f89),
        blue: rgb(0x7aa2f7),
        blue1: rgb(0x2ac3de),
        blue5: rgb(0x89ddff),
        blue6: rgb(0xb4f9f8),
        cyan: rgb(0x7dcfff),
        magenta: rgb(0xbb9af7),
        purple: rgb(0x9d7cd8),
        orange: rgb(0xff9e64),
        yellow: rgb(0xe0af68),
        green: rgb(0x9ece6a),
        green1: rgb(0x73daca),
        red: rgb(0xf7768e),
        red1: rgb(0xdb4b4b),
        git_add: rgb(0x449dab),
        git_change: rgb(0x6183bb),
        git_delete: rgb(0x914c54),
        syntax: HashMap::new(),
    };
    for (scopes, color, flags) in DEFAULT_SYNTAX {
        let spec = StyleSpec {
            fg: COLOR_NAMES
                .iter()
                .find(|name| **name == *color)
                .map(|name| ColorRef::Named(name)),
            bg: None,
            bold: flags.contains("bold"),
            italic: flags.contains("italic"),
            underline: flags.contains("underline"),
        };
        for scope in scopes.split('|') {
            theme.syntax.insert(scope.to_string(), spec);
        }
    }
    theme
}

fn with_colors(mut theme: Theme, name: &str, colors: &[(&str, u32)]) -> Theme {
    theme.name = name.to_string();
    for (color, value) in colors {
        let known = theme.set_color(color, rgb(*value));
        debug_assert!(known, "unknown preset color {color}");
    }
    theme
}

pub fn preset(name: &str) -> Option<Theme> {
    Some(match name {
        "tokyonight-storm" => storm(),
        "tokyonight-night" => with_colors(
            storm(),
            name,
            &[
                ("bg", 0x1a1b26),
                ("bg_dark", 0x16161e),
                ("bg_inactive", 0x111118),
                ("bg_visual", 0x283457),
            ],
        ),
        "tokyonight-moon" => with_colors(
            storm(),
            name,
            &[
                ("bg", 0x222436),
                ("bg_dark", 0x1e2030),
                ("bg_inactive", 0x191a2a),
                ("bg_highlight", 0x2f334d),
                ("bg_visual", 0x2d3f76),
                ("bg_search", 0x3e68d7),
                ("fg", 0xc8d3f5),
                ("fg_dark", 0x828bb8),
                ("comment", 0x636da6),
                ("blue", 0x82aaff),
                ("blue1", 0x65bcff),
                ("cyan", 0x86e1fc),
                ("magenta", 0xc099ff),
                ("purple", 0xfca7ea),
                ("orange", 0xff966c),
                ("yellow", 0xffc777),
                ("green", 0xc3e88d),
                ("green1", 0x4fd6be),
                ("red", 0xff757f),
                ("red1", 0xc53b53),
                ("git_add", 0xb8db87),
                ("git_change", 0x7ca1f2),
                ("git_delete", 0xe26a75),
            ],
        ),
        "tokyonight-day" => with_colors(
            storm(),
            name,
            &[
                ("bg", 0xe1e2e7),
                ("bg_dark", 0xd0d5e3),
                ("bg_inactive", 0xd5d6db),
                ("bg_highlight", 0xc4c8da),
                ("bg_visual", 0xb7c1e3),
                ("bg_search", 0x7890dd),
                ("fg", 0x3760bf),
                ("fg_dark", 0x6172b0),
                ("fg_gutter", 0xa8aecb),
                ("dark3", 0x8990b3),
                ("dark5", 0x68709a),
                ("comment", 0x848cb5),
                ("blue", 0x2e7de9),
                ("blue1", 0x188092),
                ("blue5", 0x006a83),
                ("blue6", 0x2e5857),
                ("cyan", 0x007197),
                ("magenta", 0x9854f1),
                ("purple", 0x7847bd),
                ("orange", 0xb15c00),
                ("yellow", 0x8c6c3e),
                ("green", 0x587539),
                ("green1", 0x387068),
                ("red", 0xf52a65),
                ("red1", 0xc64343),
                ("git_add", 0x4197a4),
                ("git_change", 0x506d9c),
                ("git_delete", 0xc47981),
            ],
        ),
        "gruvbox-dark" => with_colors(
            storm(),
            name,
            &[
                ("bg", 0x282828),
                ("bg_dark", 0x1d2021),
                ("bg_inactive", 0x171819),
                ("bg_highlight", 0x3c3836),
                ("bg_visual", 0x504945),
                ("bg_search", 0x79740e),
                ("fg", 0xebdbb2),
                ("fg_dark", 0xd5c4a1),
                ("fg_gutter", 0x504945),
                ("dark3", 0x665c54),
                ("dark5", 0xa89984),
                ("comment", 0x928374),
                ("blue", 0x83a598),
                ("blue1", 0x8ec07c),
                ("blue5", 0xbdae93),
                ("blue6", 0x689d6a),
                ("cyan", 0x8ec07c),
                ("magenta", 0xd3869b),
                ("purple", 0xb16286),
                ("orange", 0xfe8019),
                ("yellow", 0xfabd2f),
                ("green", 0xb8bb26),
                ("green1", 0x8ec07c),
                ("red", 0xfb4934),
                ("red1", 0xcc241d),
                ("git_add", 0x98971a),
                ("git_change", 0x458588),
                ("git_delete", 0xcc241d),
            ],
        ),
        "gruvbox-light" => with_colors(
            storm(),
            name,
            &[
                ("bg", 0xfbf1c7),
                ("bg_dark", 0xf2e5bc),
                ("bg_inactive", 0xebdbb2),
                ("bg_highlight", 0xebdbb2),
                ("bg_visual", 0xd5c4a1),
                ("bg_search", 0xf9d77e),
                ("fg", 0x3c3836),
                ("fg_dark", 0x504945),
                ("fg_gutter", 0xbdae93),
                ("dark3", 0xa89984),
                ("dark5", 0x7c6f64),
                ("comment", 0x928374),
                ("blue", 0x076678),
                ("blue1", 0x427b58),
                ("blue5", 0x665c54),
                ("blue6", 0x689d6a),
                ("cyan", 0x427b58),
                ("magenta", 0x8f3f71),
                ("purple", 0xb16286),
                ("orange", 0xaf3a03),
                ("yellow", 0xb57614),
                ("green", 0x79740e),
                ("green1", 0x427b58),
                ("red", 0x9d0006),
                ("red1", 0xcc241d),
                ("git_add", 0x79740e),
                ("git_change", 0x076678),
                ("git_delete", 0xcc241d),
            ],
        ),
        "nord" => with_colors(
            storm(),
            name,
            &[
                ("bg", 0x2e3440),
                ("bg_dark", 0x272c36),
                ("bg_inactive", 0x222730),
                ("bg_highlight", 0x3b4252),
                ("bg_visual", 0x434c5e),
                ("bg_search", 0x5e81ac),
                ("fg", 0xd8dee9),
                ("fg_dark", 0xb8c0cd),
                ("fg_gutter", 0x4c566a),
                ("dark3", 0x5c677d),
                ("dark5", 0x7b88a1),
                ("comment", 0x616e88),
                ("blue", 0x88c0d0),
                ("blue1", 0x8fbcbb),
                ("blue5", 0x81a1c1),
                ("blue6", 0xebcb8b),
                ("cyan", 0x8fbcbb),
                ("magenta", 0xb48ead),
                ("purple", 0x81a1c1),
                ("orange", 0xd08770),
                ("yellow", 0xebcb8b),
                ("green", 0xa3be8c),
                ("green1", 0x8fbcbb),
                ("red", 0xbf616a),
                ("red1", 0xbf616a),
                ("git_add", 0xa3be8c),
                ("git_change", 0x81a1c1),
                ("git_delete", 0xbf616a),
            ],
        ),
        "solarized-light" => with_colors(
            storm(),
            name,
            &[
                ("bg", 0xfdf6e3),
                ("bg_dark", 0xeee8d5),
                ("bg_inactive", 0xe6e0cc),
                ("bg_highlight", 0xe9e2cb),
                ("bg_visual", 0xd6d1bb),
                ("bg_search", 0xf1d68a),
                ("fg", 0x586e75),
                ("fg_dark", 0x657b83),
                ("fg_gutter", 0xc8c2ad),
                ("dark3", 0x93a1a1),
                ("dark5", 0x708387),
                ("comment", 0x93a1a1),
                ("blue", 0x268bd2),
                ("blue1", 0x2aa198),
                ("blue5", 0x657b83),
                ("blue6", 0xdc322f),
                ("cyan", 0x2aa198),
                ("magenta", 0xd33682),
                ("purple", 0x6c71c4),
                ("orange", 0xcb4b16),
                ("yellow", 0xb58900),
                ("green", 0x859900),
                ("green1", 0x2aa198),
                ("red", 0xdc322f),
                ("red1", 0xdc322f),
                ("git_add", 0x859900),
                ("git_change", 0x268bd2),
                ("git_delete", 0xdc322f),
            ],
        ),
        _ => return None,
    })
}

impl Default for Theme {
    fn default() -> Self {
        storm()
    }
}

pub fn parse_hex(text: &str) -> Option<Rgb> {
    let hex = text.strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        3 => {
            let digit = |i: usize| value(&hex[i..i + 1]).map(|v| v * 17);
            Some(Rgb(digit(0)?, digit(1)?, digit(2)?))
        }
        6 => Some(Rgb(
            value(&hex[0..2])?,
            value(&hex[2..4])?,
            value(&hex[4..6])?,
        )),
        _ => None,
    }
}

impl Theme {
    /// Background of panels; dimmed when the terminal (or tmux pane) has
    /// no focus, like the `window-style` dimming in tmux.conf.
    pub fn background(&self, focused: bool) -> Rgb {
        if focused { self.bg } else { self.bg_inactive }
    }

    /// The palette name of a color in upper case with its value
    /// (`BLUE #7aa2f7`), or only the value. Style snapshots of the
    /// end-to-end tests use it.
    pub fn color_name(&self, color: Rgb) -> String {
        let hex = format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2);
        COLOR_NAMES
            .iter()
            .find(|name| self.color(name) == Some(color))
            .map(|name| format!("{} {hex}", name.to_uppercase()))
            .unwrap_or(hex)
    }

    fn resolve(&self, color: ColorRef) -> Rgb {
        match color {
            ColorRef::Named(name) => self.color(name).unwrap_or(self.fg),
            ColorRef::Rgb(rgb) => rgb,
        }
    }

    /// Style for a capture such as `function.method.call`: the longest
    /// prefix with a style decides.
    pub fn capture_style(&self, name: &str) -> Option<Style> {
        let mut candidate = name;
        loop {
            if let Some(spec) = self.syntax.get(candidate) {
                return Some(Style {
                    fg: spec.fg.map(|c| self.resolve(c)),
                    bg: spec.bg.map(|c| self.resolve(c)),
                    bold: spec.bold,
                    italic: spec.italic,
                    underline: spec.underline,
                });
            }
            let dot = candidate.rfind('.')?;
            candidate = &candidate[..dot];
        }
    }

    /// Styles of a language's captures, indexed like its query captures.
    pub fn capture_styles(&self, language: &LanguageConfig) -> Vec<Option<Style>> {
        language
            .highlighting()
            .map(|h| {
                h.query
                    .capture_names()
                    .iter()
                    .map(|name| self.capture_style(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn color_ref(&self, value: &Json, at: &str, errors: &mut Vec<String>) -> Option<ColorRef> {
        let text = match value {
            Json::String(text) => text,
            Json::Null => return None,
            other => {
                errors.push(format!("{at}: expected a color, not {}", other.type_name()));
                return None;
            }
        };
        if let Some(rgb) = parse_hex(text) {
            return Some(ColorRef::Rgb(rgb));
        }
        if let Some(name) = COLOR_NAMES.iter().find(|name| **name == text.as_str()) {
            return Some(ColorRef::Named(name));
        }
        errors.push(format!(
            "{at}: \"{text}\" is not a #rrggbb color or a palette color ({})",
            COLOR_NAMES.join(", ")
        ));
        None
    }

    /// Builds a theme from the `theme` section of `yate.json`:
    /// `{ "preset": "...", "colors": { "bg": "#1a1b26" }, "syntax": { "keyword": "magenta" } }`.
    /// Returns the theme and the problems found.
    pub fn from_json(section: Option<&Json>) -> (Theme, Vec<String>) {
        let mut errors = Vec::new();
        let Some(section) = section else {
            return (Theme::default(), errors);
        };
        if !matches!(section, Json::Object(_)) {
            errors.push(format!(
                "theme: expected an object, not {}",
                section.type_name()
            ));
            return (Theme::default(), errors);
        }
        let mut theme = match section.get("preset") {
            None => Theme::default(),
            Some(Json::String(name)) => preset(name).unwrap_or_else(|| {
                errors.push(format!(
                    "theme.preset: unknown preset \"{name}\" (known: {})",
                    PRESETS.join(", ")
                ));
                Theme::default()
            }),
            Some(other) => {
                errors.push(format!(
                    "theme.preset: expected a string, not {}",
                    other.type_name()
                ));
                Theme::default()
            }
        };
        for (key, _) in section.entries() {
            if !matches!(key.as_str(), "preset" | "colors" | "syntax") {
                errors.push(format!(
                    "theme.{key}: unknown key (known: preset, colors, syntax)"
                ));
            }
        }
        for (name, value) in section.get("colors").map(Json::entries).unwrap_or_default() {
            let at = format!("theme.colors.{name}");
            if !COLOR_NAMES.contains(&name.as_str()) {
                errors.push(format!(
                    "{at}: unknown color (known: {})",
                    COLOR_NAMES.join(", ")
                ));
                continue;
            }
            if let Some(color) = theme.color_ref(value, &at, &mut errors) {
                let rgb = theme.resolve(color);
                theme.set_color(name, rgb);
            }
        }
        for (scope, value) in section.get("syntax").map(Json::entries).unwrap_or_default() {
            let at = format!("theme.syntax.{scope}");
            let spec = match value {
                Json::Object(_) => {
                    let mut spec = StyleSpec::default();
                    for (key, field) in value.entries() {
                        match (key.as_str(), field) {
                            ("fg", _) => spec.fg = theme.color_ref(field, &at, &mut errors),
                            ("bg", _) => spec.bg = theme.color_ref(field, &at, &mut errors),
                            ("bold", Json::Bool(on)) => spec.bold = *on,
                            ("italic", Json::Bool(on)) => spec.italic = *on,
                            ("underline", Json::Bool(on)) => spec.underline = *on,
                            _ => errors.push(format!(
                                "{at}.{key}: expected fg, bg (colors) or bold, italic, underline (true/false)"
                            )),
                        }
                    }
                    spec
                }
                _ => StyleSpec {
                    fg: theme.color_ref(value, &at, &mut errors),
                    ..StyleSpec::default()
                },
            };
            theme.syntax.insert(scope.clone(), spec);
        }
        (theme, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    #[test]
    fn longest_capture_prefix_wins() {
        let theme = Theme::default();
        assert_eq!(
            theme.capture_style("function.method.call"),
            theme.capture_style("function.method")
        );
        assert_eq!(
            theme.capture_style("keyword.return"),
            Some(Style::new().fg(theme.magenta))
        );
        assert_eq!(theme.capture_style("unknown.thing"), None);
    }

    #[test]
    fn json_overrides_colors_and_syntax_that_refers_to_them() {
        let section = json::parse(
            r##"{
              "preset": "tokyonight-night",
              "colors": { "magenta": "#ff00ff", "bg_visual": "blue" },
              "syntax": { "string": { "fg": "#00ff00", "bold": true }, "comment": "dark3" }
            }"##,
        )
        .unwrap();
        let (theme, errors) = Theme::from_json(Some(&section));
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(theme.bg, Rgb::hex(0x1a1b26));
        assert_eq!(theme.bg_visual, theme.blue);
        assert_eq!(
            theme.capture_style("keyword.return").unwrap().fg,
            Some(Rgb::hex(0xff00ff))
        );
        assert_eq!(
            theme.capture_style("string.quoted"),
            Some(Style::new().fg(Rgb::hex(0x00ff00)).bold())
        );
        assert_eq!(
            theme.capture_style("comment").unwrap().fg,
            Some(theme.dark3)
        );
    }

    #[test]
    fn mistakes_are_reported_and_ignored() {
        let section = json::parse(
            r##"{ "preset": "solarized", "colors": { "bgg": "#000000", "fg": "#12" }, "font": 1 }"##,
        )
        .unwrap();
        let (theme, errors) = Theme::from_json(Some(&section));
        assert_eq!(theme.fg, Theme::default().fg);
        assert_eq!(errors.len(), 4, "{errors:#?}");
        assert!(errors[0].contains("unknown preset \"solarized\""));
    }

    #[test]
    fn presets_outside_tokyo_night_set_every_color() {
        let storm = storm();
        for name in PRESETS
            .iter()
            .filter(|name| !name.starts_with("tokyonight"))
        {
            let theme = preset(name).unwrap();
            let kept: Vec<_> = COLOR_NAMES
                .iter()
                .filter(|color| theme.color(color) == storm.color(color))
                .collect();
            assert!(kept.is_empty(), "{name} keeps storm colors {kept:?}");
        }
    }

    #[test]
    fn every_preset_exists() {
        for name in PRESETS {
            assert_eq!(preset(name).unwrap().name, *name);
        }
    }
}
