//! Undo and redo. Every edit of a document is a transaction of changes;
//! typing and deleting in a row merge into one transaction.

use super::selection::Selection;
use std::time::{Duration, Instant};

const MERGE_WINDOW: Duration = Duration::from_millis(1200);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedChange {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Other,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub changes: Vec<AppliedChange>,
    pub before: Selection,
    pub after: Selection,
    kind: EditKind,
    at: Instant,
}

#[derive(Default)]
pub struct History {
    transactions: Vec<Transaction>,
    /// Number of transactions currently applied.
    position: usize,
    saved_position: Option<usize>,
}

impl History {
    pub fn record(
        &mut self,
        changes: Vec<AppliedChange>,
        before: Selection,
        after: Selection,
        kind: EditKind,
    ) {
        if changes.is_empty() {
            return;
        }
        self.transactions.truncate(self.position);
        if self
            .saved_position
            .is_some_and(|saved| saved > self.position)
        {
            self.saved_position = None;
        }
        let can_merge = kind != EditKind::Other && self.saved_position != Some(self.position);
        if can_merge
            && let Some(last) = self.transactions.last_mut()
            && last.kind == kind
            && last.at.elapsed() < MERGE_WINDOW
            && last.after == before
            && !changes.iter().any(|change| change.inserted.contains('\n'))
        {
            last.changes.extend(changes);
            last.after = after;
            last.at = Instant::now();
            return;
        }
        self.transactions.push(Transaction {
            changes,
            before,
            after,
            kind,
            at: Instant::now(),
        });
        self.position = self.transactions.len();
    }

    /// Ends the current merge group, e.g. when the cursor moves.
    pub fn seal(&mut self) {
        if let Some(last) = self.transactions.last_mut() {
            last.kind = EditKind::Other;
        }
    }

    pub fn undo(&mut self) -> Option<&Transaction> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        self.transactions.get(self.position)
    }

    pub fn redo(&mut self) -> Option<&Transaction> {
        let transaction = self.transactions.get(self.position)?;
        self.position += 1;
        Some(transaction)
    }

    pub fn mark_saved(&mut self) {
        self.saved_position = Some(self.position);
        self.seal();
    }

    pub fn is_saved(&self) -> bool {
        self.saved_position == Some(self.position)
    }

    pub fn clear(&mut self) {
        self.transactions.clear();
        self.position = 0;
        self.saved_position = Some(0);
    }
}
