//! The cursor and selection of a document, as char indices in the rope.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Selection {
    /// Where the selection started; equals `head` when nothing is selected.
    pub anchor: usize,
    /// The cursor.
    pub head: usize,
}

impl Selection {
    pub fn point(at: usize) -> Self {
        Selection {
            anchor: at,
            head: at,
        }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Selection { anchor, head }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn range(&self) -> std::ops::Range<usize> {
        self.start()..self.end()
    }

    /// Moves the cursor, extending the selection or collapsing it.
    pub fn move_to(self, head: usize, extend: bool) -> Self {
        if extend {
            Selection {
                anchor: self.anchor,
                head,
            }
        } else {
            Selection::point(head)
        }
    }

    pub fn clamp(self, len: usize) -> Self {
        Selection {
            anchor: self.anchor.min(len),
            head: self.head.min(len),
        }
    }
}
