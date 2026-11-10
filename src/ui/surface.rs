//! An in-memory grid of styled cells. Views draw into a `Surface`, and the
//! terminal backend flushes only the cells that changed since the last frame.

use crate::text::grapheme_width;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn hex(value: u32) -> Self {
        Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }

    pub fn luminance(self) -> f32 {
        0.299 * self.0 as f32 + 0.587 * self.1 as f32 + 0.114 * self.2 as f32
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Style {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Style {
    pub const fn new() -> Self {
        Style {
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    pub const fn fg(mut self, color: Rgb) -> Self {
        self.fg = Some(color);
        self
    }

    pub const fn bg(mut self, color: Rgb) -> Self {
        self.bg = Some(color);
        self
    }

    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Layers `other` on top of `self`: colors set in `other` win, text
    /// attributes add up.
    pub fn patch(self, other: Style) -> Style {
        Style {
            fg: other.fg.or(self.fg),
            bg: other.bg.or(self.bg),
            bold: self.bold || other.bold,
            italic: self.italic || other.italic,
            underline: self.underline || other.underline,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> u16 {
        self.x + self.w
    }

    pub fn bottom(&self) -> u16 {
        self.y + self.h
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn inner(&self, margin: u16) -> Rect {
        Rect {
            x: self.x + margin,
            y: self.y + margin,
            w: self.w.saturating_sub(margin * 2),
            h: self.h.saturating_sub(margin * 2),
        }
    }

    pub fn split_left(&self, width: u16) -> (Rect, Rect) {
        let width = width.min(self.w);
        (
            Rect::new(self.x, self.y, width, self.h),
            Rect::new(self.x + width, self.y, self.w - width, self.h),
        )
    }

    pub fn split_top(&self, height: u16) -> (Rect, Rect) {
        let height = height.min(self.h);
        (
            Rect::new(self.x, self.y, self.w, height),
            Rect::new(self.x, self.y + height, self.w, self.h - height),
        )
    }

    pub fn split_bottom(&self, height: u16) -> (Rect, Rect) {
        let height = height.min(self.h);
        (
            Rect::new(self.x, self.y, self.w, self.h - height),
            Rect::new(self.x, self.y + self.h - height, self.w, height),
        )
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cell {
    pub symbol: String,
    pub style: Style,
    /// The right half of a wide character drawn in the cell to the left.
    pub continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            symbol: " ".to_string(),
            style: Style::default(),
            continuation: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Surface {
    pub width: u16,
    pub height: u16,
    pub cells: Vec<Cell>,
}

impl Surface {
    pub fn new(width: u16, height: u16) -> Self {
        Surface {
            width,
            height,
            cells: vec![Cell::default(); width as usize * height as usize],
        }
    }

    pub fn area(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        (x < self.width && y < self.height).then(|| y as usize * self.width as usize + x as usize)
    }

    #[cfg(test)]
    pub fn cell(&self, x: u16, y: u16) -> Option<&Cell> {
        self.index(x, y).map(|i| &self.cells[i])
    }

    pub fn fill(&mut self, rect: Rect, style: Style) {
        for y in rect.y..rect.bottom().min(self.height) {
            for x in rect.x..rect.right().min(self.width) {
                if let Some(i) = self.index(x, y) {
                    self.cells[i] = Cell {
                        symbol: " ".to_string(),
                        style,
                        continuation: false,
                    };
                }
            }
        }
    }

    pub fn set_symbol(&mut self, x: u16, y: u16, symbol: &str, style: Style) {
        if let Some(i) = self.index(x, y) {
            let base = self.cells[i].style;
            self.cells[i] = Cell {
                symbol: symbol.to_string(),
                style: base.patch(style),
                continuation: false,
            };
        }
    }

    /// Draws `text` from `(x, y)`, never past `max_x`. Styles are layered on
    /// the existing cell styles. Returns the column after the last drawn cell.
    pub fn put_str(&mut self, x: u16, y: u16, text: &str, style: Style, max_x: u16) -> u16 {
        let max_x = max_x.min(self.width);
        let mut col = x;
        for grapheme in text.graphemes(true) {
            let width = grapheme_width(grapheme) as u16;
            if width == 0 {
                continue;
            }
            if col + width > max_x {
                break;
            }
            self.put_grapheme(col, y, grapheme, width, style);
            col += width;
        }
        col
    }

    pub fn put_grapheme(&mut self, x: u16, y: u16, grapheme: &str, width: u16, style: Style) {
        let Some(i) = self.index(x, y) else {
            return;
        };
        let base = self.cells[i].style;
        self.cells[i] = Cell {
            symbol: grapheme.to_string(),
            style: base.patch(style),
            continuation: false,
        };
        for offset in 1..width {
            if let Some(j) = self.index(x + offset, y) {
                let base = self.cells[j].style;
                self.cells[j] = Cell {
                    symbol: String::new(),
                    style: base.patch(style),
                    continuation: true,
                };
            }
        }
    }

    /// Draws `text` right-aligned so that it ends at `right`.
    pub fn put_str_right(&mut self, right: u16, y: u16, text: &str, style: Style) -> u16 {
        let width = crate::text::str_width(text) as u16;
        let x = right.saturating_sub(width);
        self.put_str(x, y, text, style, right);
        x
    }

    pub fn border(&mut self, rect: Rect, style: Style, title: Option<&str>) {
        if rect.w < 2 || rect.h < 2 {
            return;
        }
        let right = rect.right() - 1;
        let bottom = rect.bottom() - 1;
        for x in rect.x + 1..right {
            self.set_symbol(x, rect.y, "─", style);
            self.set_symbol(x, bottom, "─", style);
        }
        for y in rect.y + 1..bottom {
            self.set_symbol(rect.x, y, "│", style);
            self.set_symbol(right, y, "│", style);
        }
        self.set_symbol(rect.x, rect.y, "╭", style);
        self.set_symbol(right, rect.y, "╮", style);
        self.set_symbol(rect.x, bottom, "╰", style);
        self.set_symbol(right, bottom, "╯", style);
        if let Some(title) = title {
            let text = format!(" {title} ");
            self.put_str(rect.x + 2, rect.y, &text, style.bold(), right);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_characters_take_two_cells() {
        let mut surface = Surface::new(6, 1);
        let end = surface.put_str(0, 0, "a界b", Style::default(), 6);
        assert_eq!(end, 4);
        assert_eq!(surface.cell(1, 0).unwrap().symbol, "界");
        assert!(surface.cell(2, 0).unwrap().continuation);
        assert_eq!(surface.cell(3, 0).unwrap().symbol, "b");
    }

    #[test]
    fn text_is_clipped_at_max_x() {
        let mut surface = Surface::new(10, 1);
        let end = surface.put_str(0, 0, "abcdef", Style::default(), 3);
        assert_eq!(end, 3);
        assert_eq!(surface.cell(3, 0).unwrap().symbol, " ");
    }
}
