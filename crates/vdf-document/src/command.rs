//! Command system and undo/redo history.
//!
//! Rules (MASTER_PLAN.md §12):
//! - every document edit is a command
//! - commands carry their own inverse data; undo never snapshots
//! - a single stroke is one command; partial erase splits into reversible
//!   segment edits
//! - the serialized command form doubles as the recovery-log record, which
//!   is what keeps replay deterministic

use vdf_core::VdfResult;

use crate::document::Document;

/// A reversible edit against a document.
pub trait DocumentCommand: Send {
    /// Human-readable description for the history UI / logs.
    fn describe(&self) -> String;

    /// Applies the edit. Must record enough inverse data to undo later.
    /// Failing execute must leave the document unchanged.
    fn execute(&mut self, doc: &mut Document) -> VdfResult<()>;

    /// Reverses the edit using the inverse data recorded by `execute`.
    fn undo(&mut self, doc: &mut Document) -> VdfResult<()>;

    /// Re-applies after an undo. Default = execute; commands whose forward
    /// application consumes state override this.
    fn redo(&mut self, doc: &mut Document) -> VdfResult<()> {
        self.execute(doc)
    }
}

/// Bounded undo/redo history. Applying a command clears the redo stack.
#[derive(Default)]
pub struct History {
    undo: Vec<Box<dyn DocumentCommand>>,
    redo: Vec<Box<dyn DocumentCommand>>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Executes a command against `doc` and records it for undo.
    pub fn apply(
        &mut self,
        mut cmd: Box<dyn DocumentCommand>,
        doc: &mut Document,
    ) -> VdfResult<()> {
        cmd.execute(doc)?;
        self.undo.push(cmd);
        self.redo.clear();
        while self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        Ok(())
    }

    pub fn undo(&mut self, doc: &mut Document) -> VdfResult<bool> {
        let Some(mut cmd) = self.undo.pop() else {
            return Ok(false);
        };
        cmd.undo(doc)?;
        self.redo.push(cmd);
        Ok(true)
    }

    pub fn redo(&mut self, doc: &mut Document) -> VdfResult<bool> {
        let Some(mut cmd) = self.redo.pop() else {
            return Ok(false);
        };
        cmd.redo(doc)?;
        self.undo.push(cmd);
        Ok(true)
    }

    /// Description of the next command `undo` would reverse.
    pub fn next_undo_description(&self) -> Option<String> {
        self.undo.last().map(|c| c.describe())
    }

    /// Description of the next command `redo` would re-apply.
    pub fn next_redo_description(&self) -> Option<String> {
        self.redo.last().map(|c| c.describe())
    }
}
