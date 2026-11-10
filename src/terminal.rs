//! Terminal setup and teardown, and the frame flush that writes only the
//! cells that changed since the previous frame.

use crate::ui::surface::{Rgb, Style, Surface};
use crossterm::cursor::{Hide, MoveTo, SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::style::{
    Attribute, Color, Print, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen, SetTitle,
};
use crossterm::{execute, queue};
use std::io::{self, Stdout, Write};

pub struct Terminal {
    out: Stdout,
    previous: Option<Surface>,
    keyboard_enhanced: bool,
}

impl Terminal {
    pub fn enter() -> io::Result<Terminal> {
        install_panic_hook();
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            EnableFocusChange,
            Hide
        )?;
        let keyboard_enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false);
        if keyboard_enhanced {
            execute!(
                out,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                )
            )?;
        }
        Ok(Terminal {
            out,
            previous: None,
            keyboard_enhanced,
        })
    }

    pub fn size(&self) -> (u16, u16) {
        terminal::size().unwrap_or((80, 24))
    }

    pub fn set_title(&mut self, title: &str) {
        let _ = execute!(self.out, SetTitle(title));
    }

    /// Forces a full redraw on the next flush.
    pub fn invalidate(&mut self) {
        self.previous = None;
    }

    /// Writes raw bytes, e.g. an OSC 52 clipboard sequence.
    pub fn write_raw(&mut self, bytes: &[u8]) {
        let _ = self.out.write_all(bytes);
        let _ = self.out.flush();
    }

    pub fn flush(&mut self, frame: &Surface, cursor: Option<(u16, u16)>) -> io::Result<()> {
        let full = match &self.previous {
            Some(previous) => previous.width != frame.width || previous.height != frame.height,
            None => true,
        };
        queue!(self.out, BeginSynchronizedUpdate, Hide)?;
        if full {
            queue!(
                self.out,
                SetAttribute(Attribute::Reset),
                Clear(ClearType::All)
            )?;
        }
        let mut current_style: Option<Style> = None;
        let mut expected_position: Option<(u16, u16)> = None;
        for y in 0..frame.height {
            for x in 0..frame.width {
                let index = y as usize * frame.width as usize + x as usize;
                let cell = &frame.cells[index];
                if cell.continuation {
                    continue;
                }
                let unchanged = !full
                    && self
                        .previous
                        .as_ref()
                        .is_some_and(|previous| previous.cells[index] == *cell)
                    && !next_is_changed_continuation(self.previous.as_ref(), frame, index);
                if unchanged {
                    continue;
                }
                if expected_position != Some((x, y)) {
                    queue!(self.out, MoveTo(x, y))?;
                }
                if current_style != Some(cell.style) {
                    write_style(&mut self.out, cell.style)?;
                    current_style = Some(cell.style);
                }
                queue!(self.out, Print(&cell.symbol))?;
                let width = crate::text::grapheme_width(&cell.symbol).max(1) as u16;
                expected_position = Some((x + width, y));
            }
        }
        queue!(self.out, SetAttribute(Attribute::Reset))?;
        if let Some((x, y)) = cursor {
            queue!(self.out, MoveTo(x, y), SetCursorStyle::BlinkingBar, Show)?;
        }
        queue!(self.out, EndSynchronizedUpdate)?;
        self.out.flush()?;
        self.previous = Some(frame.clone());
        Ok(())
    }
}

fn next_is_changed_continuation(previous: Option<&Surface>, frame: &Surface, index: usize) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    frame
        .cells
        .get(index + 1)
        .is_some_and(|cell| cell.continuation && previous.cells.get(index + 1) != Some(cell))
}

fn color(rgb: Rgb) -> Color {
    Color::Rgb {
        r: rgb.0,
        g: rgb.1,
        b: rgb.2,
    }
}

fn write_style(out: &mut Stdout, style: Style) -> io::Result<()> {
    queue!(out, SetAttribute(Attribute::Reset))?;
    if let Some(fg) = style.fg {
        queue!(out, SetForegroundColor(color(fg)))?;
    }
    if let Some(bg) = style.bg {
        queue!(out, SetBackgroundColor(color(bg)))?;
    }
    if style.bold {
        queue!(out, SetAttribute(Attribute::Bold))?;
    }
    if style.italic {
        queue!(out, SetAttribute(Attribute::Italic))?;
    }
    if style.underline {
        queue!(out, SetAttribute(Attribute::Underlined))?;
    }
    Ok(())
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.keyboard_enhanced {
            let _ = execute!(self.out, PopKeyboardEnhancementFlags);
        }
        restore();
    }
}

fn restore() {
    let mut out = io::stdout();
    let _ = execute!(
        out,
        SetAttribute(Attribute::Reset),
        SetCursorStyle::DefaultUserShape,
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    );
    let _ = terminal::disable_raw_mode();
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        default_hook(info);
    }));
}
