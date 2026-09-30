//! Document and page structures, plus the two M0 concrete commands.

use std::collections::HashMap;

use vdf_core::{DocumentId, IdGenerator, ObjectId, PageId, Rect, Revision, VdfError, VdfResult};

use crate::command::DocumentCommand;
use crate::object::{ObjectModel, ObjectType};

/// One page of a document. PDF content itself stays inside the PDF engine
/// (`vdf-pdf`, M1); the model stores identity, geometry, and view metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct PageModel {
    pub id: PageId,
    /// Width/height in PDF points before rotation.
    pub size: (f64, f64),
    pub rotation: vdf_core::Rotation,
    pub label: Option<String>,
}

impl PageModel {
    pub fn new(id: PageId, size: (f64, f64)) -> Self {
        Self {
            id,
            size,
            rotation: vdf_core::Rotation::D0,
            label: None,
        }
    }
}

/// The in-memory VDF document (MASTER_PLAN.md §7).
///
/// Mutation goes exclusively through [`crate::History::apply`] with
/// [`DocumentCommand`]s; the helper methods here are the low-level levers
/// those commands use.
pub struct Document {
    pub id: DocumentId,
    pub title: String,
    pages: Vec<PageModel>,
    objects: HashMap<ObjectId, ObjectModel>,
    z_counter: u32,
    ids: IdGenerator,
    revision: Revision,
}

impl Document {
    pub fn new(title: impl Into<String>) -> Self {
        let ids = IdGenerator::new();
        let id = ids.document_id();
        Self {
            id,
            title: title.into(),
            pages: Vec::new(),
            objects: HashMap::new(),
            z_counter: 0,
            ids,
            revision: 0,
        }
    }

    pub fn add_page(&mut self, size: (f64, f64)) -> PageId {
        let id = self.ids.next_page_id();
        self.pages.push(PageModel::new(id, size));
        self.revision += 1;
        id
    }

    pub fn pages(&self) -> &[PageModel] {
        &self.pages
    }

    pub fn page(&self, id: PageId) -> Option<&PageModel> {
        self.pages.iter().find(|p| p.id == id)
    }

    pub fn object(&self, id: ObjectId) -> Option<&ObjectModel> {
        self.objects.get(&id)
    }

    pub fn object_mut(&mut self, id: ObjectId) -> Option<&mut ObjectModel> {
        self.objects.get_mut(&id)
    }

    pub fn objects(&self) -> impl Iterator<Item = &ObjectModel> {
        self.objects.values()
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Inserts an object and assigns it the next z-order value; used by
    /// `AddObject::execute`. Returns an error if the id is already taken —
    /// ids are never reused.
    pub(crate) fn insert_object(&mut self, mut obj: ObjectModel) -> VdfResult<()> {
        if self.objects.contains_key(&obj.id) {
            return Err(VdfError::Document(format!(
                "object id {} already exists; ids must never be reused",
                obj.id
            )));
        }
        self.z_counter += 1;
        obj.z = self.z_counter;
        self.objects.insert(obj.id, obj);
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn remove_object(&mut self, id: ObjectId) -> VdfResult<ObjectModel> {
        let obj = self
            .objects
            .remove(&id)
            .ok_or_else(|| VdfError::Document(format!("object {id} not found")))?;
        self.revision += 1;
        Ok(obj)
    }

    /// Test/demo helper: makes a plain ink-mark object envelope on a page.
    pub fn make_envelope(&mut self, page: PageId, bounds: Rect) -> ObjectModel {
        let id = self.ids.next_object_id();
        ObjectModel::new(id, ObjectType::Ink, page, bounds)
    }
}

/// Adds a new object to the document. Undo removes it (identity preserved,
/// so a redo after undo restores the same id).
pub struct AddObject {
    /// The object while it is not in the document (before execute / after undo).
    pub payload: Option<ObjectModel>,
    /// Identity of the object while it lives in the document (after execute).
    pub inserted_id: Option<ObjectId>,
}

impl AddObject {
    pub fn new(obj: ObjectModel) -> Self {
        Self {
            payload: Some(obj),
            inserted_id: None,
        }
    }
}

impl DocumentCommand for AddObject {
    fn describe(&self) -> String {
        match (&self.payload, self.inserted_id) {
            (Some(o), _) => format!("add {} {}", o.ty.as_str(), o.id),
            (None, Some(id)) => format!("add object {id}"),
            (None, None) => "add object".into(),
        }
    }

    fn execute(&mut self, doc: &mut Document) -> VdfResult<()> {
        let obj = self
            .payload
            .take()
            .ok_or_else(|| VdfError::Document("AddObject executed twice".into()))?;
        self.inserted_id = Some(obj.id);
        doc.insert_object(obj)
    }

    fn undo(&mut self, doc: &mut Document) -> VdfResult<()> {
        let id = self
            .inserted_id
            .ok_or_else(|| VdfError::Document("AddObject undone before execute".into()))?;
        let removed = doc.remove_object(id)?;
        self.payload = Some(removed);
        self.inserted_id = None;
        Ok(())
    }
}

/// Sets an object's opacity, recording the previous value for undo.
pub struct SetObjectOpacity {
    pub id: ObjectId,
    pub new_opacity: f32,
    pub old_opacity: Option<f32>,
}

impl SetObjectOpacity {
    pub fn new(id: ObjectId, new_opacity: f32) -> Self {
        Self {
            id,
            new_opacity,
            old_opacity: None,
        }
    }
}

impl DocumentCommand for SetObjectOpacity {
    fn describe(&self) -> String {
        format!("set opacity of {} to {:.3}", self.id, self.new_opacity)
    }

    fn execute(&mut self, doc: &mut Document) -> VdfResult<()> {
        let obj = doc
            .object_mut(self.id)
            .ok_or_else(|| VdfError::Document(format!("object {} not found", self.id)))?;
        if self.old_opacity.is_none() {
            self.old_opacity = Some(obj.opacity);
        }
        obj.opacity = self.new_opacity.clamp(0.0, 1.0);
        Ok(())
    }

    fn undo(&mut self, doc: &mut Document) -> VdfResult<()> {
        let old = self
            .old_opacity
            .ok_or_else(|| VdfError::Document("SetObjectOpacity undone before execute".into()))?;
        let obj = doc
            .object_mut(self.id)
            .ok_or_else(|| VdfError::Document(format!("object {} not found", self.id)))?;
        obj.opacity = old;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::History;

    #[test]
    fn add_page_and_objects() {
        let mut doc = Document::new("test");
        let p1 = doc.add_page((612.0, 792.0));
        let p2 = doc.add_page((595.0, 842.0));
        assert_eq!(doc.pages().len(), 2);
        assert_ne!(p1, p2);
        let o1 = doc.make_envelope(p1, Rect::from_center_size(100.0, 100.0, 10.0, 10.0));
        let id = o1.id;
        doc.insert_object(o1).unwrap();
        assert_eq!(doc.object_count(), 1);
        assert!(doc.object(id).is_some());
        // ids are never reused
        let dup = ObjectModel::new(id, ObjectType::Ink, p1, Rect::default());
        assert!(doc.insert_object(dup).is_err());
    }

    #[test]
    fn z_order_is_assigned_monotonically() {
        let mut doc = Document::new("t");
        let page = doc.add_page((612.0, 792.0));
        let a = doc.make_envelope(page, Rect::default());
        let b = doc.make_envelope(page, Rect::default());
        let c = doc.make_envelope(page, Rect::default());
        doc.insert_object(a).unwrap();
        doc.insert_object(b).unwrap();
        doc.insert_object(c).unwrap();
        let mut zs: Vec<u32> = doc.objects().map(|o| o.z).collect();
        assert_eq!(zs.len(), 3);
        zs.sort_unstable();
        assert!(
            zs.windows(2).all(|w| w[0] < w[1]),
            "z values must be unique and increasing (sorted: {zs:?})"
        );
    }

    #[test]
    fn history_undo_redo_opacity() {
        let mut doc = Document::new("t");
        let page = doc.add_page((612.0, 792.0));
        let obj = doc.make_envelope(page, Rect::from_center_size(1.0, 1.0, 5.0, 5.0));
        let id = obj.id;

        let mut hist = History::new(100);
        hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
        assert_eq!(doc.object(id).unwrap().opacity, 1.0);

        hist.apply(Box::new(SetObjectOpacity::new(id, 0.25)), &mut doc)
            .unwrap();
        assert_eq!(doc.object(id).unwrap().opacity, 0.25);

        assert!(hist.undo(&mut doc).unwrap());
        assert_eq!(doc.object(id).unwrap().opacity, 1.0);

        assert!(hist.redo(&mut doc).unwrap());
        assert_eq!(doc.object(id).unwrap().opacity, 0.25);
    }

    #[test]
    fn add_object_undo_restores_same_identity() {
        let mut doc = Document::new("t");
        let page = doc.add_page((612.0, 792.0));
        let obj = doc.make_envelope(page, Rect::from_center_size(0.0, 0.0, 1.0, 1.0));
        let id = obj.id;

        let mut hist = History::new(10);
        hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
        assert_eq!(doc.object_count(), 1);
        assert!(hist.undo(&mut doc).unwrap());
        assert_eq!(doc.object_count(), 0);
        assert!(hist.redo(&mut doc).unwrap());
        assert_eq!(doc.object_count(), 1);
        assert!(
            doc.object(id).is_some(),
            "redo must restore the same persistent id"
        );
    }

    #[test]
    fn applying_command_clears_redo_stack() {
        let mut doc = Document::new("t");
        let page = doc.add_page((100.0, 100.0));
        let obj = doc.make_envelope(page, Rect::default());
        let id = obj.id;
        let mut hist = History::new(10);
        hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
        hist.apply(Box::new(SetObjectOpacity::new(id, 0.5)), &mut doc)
            .unwrap();
        assert!(hist.undo(&mut doc).unwrap());
        assert_eq!(hist.redo_len(), 1);
        hist.apply(Box::new(SetObjectOpacity::new(id, 0.9)), &mut doc)
            .unwrap();
        assert_eq!(hist.redo_len(), 0, "new edit must clear redo");
        assert_eq!(doc.object(id).unwrap().opacity, 0.9);
    }

    #[test]
    fn history_trims_to_limit() {
        let mut doc = Document::new("t");
        let page = doc.add_page((100.0, 100.0));
        let obj = doc.make_envelope(page, Rect::default());
        let id = obj.id;
        let mut hist = History::new(3);
        hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
        for v in [0.1, 0.2, 0.3, 0.4, 0.5] {
            hist.apply(Box::new(SetObjectOpacity::new(id, v)), &mut doc)
                .unwrap();
        }
        assert_eq!(hist.undo_len(), 3, "history must respect its limit");
        // trimmed commands are AddObject, SetOpacity(0.1), SetOpacity(0.2);
        // the oldest surviving command is SetOpacity(0.3) whose inverse is 0.2
        while hist.undo(&mut doc).unwrap() {}
        assert_eq!(doc.object(id).unwrap().opacity, 0.2);
        assert!(!hist.can_undo());
    }

    #[test]
    fn failed_execute_leaves_history_untouched() {
        let mut doc = Document::new("t");
        let page = doc.add_page((100.0, 100.0));
        let obj = doc.make_envelope(page, Rect::default());
        let mut hist = History::new(10);
        hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
        let before = hist.undo_len();
        // command against a nonexistent object must fail cleanly
        let err = hist.apply(
            Box::new(SetObjectOpacity::new(vdf_core::ObjectId(9999), 0.5)),
            &mut doc,
        );
        assert!(err.is_err());
        assert_eq!(
            hist.undo_len(),
            before,
            "failed execute must not record history"
        );
    }

    #[test]
    fn revision_advances_with_mutations() {
        let mut doc = Document::new("t");
        let r0 = doc.revision();
        doc.add_page((100.0, 100.0));
        assert!(doc.revision() > r0);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::command::History;
    use proptest::prelude::*;

    proptest! {
        /// Random command sequences must be undoable back to the pristine
        /// state (undoing everything also removes the added object), with
        /// history exhausted and no errors.
        #[test]
        fn commands_undo_to_stable_state(ops in proptest::collection::vec(0.0f32..1.0, 1..32)) {
            let mut doc = Document::new("prop");
            let page = doc.add_page((612.0, 792.0));
            let obj = doc.make_envelope(page, Rect::default());
            let id = obj.id;
            let mut hist = History::new(1000);
            hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();

            for v in ops.iter().copied() {
                hist.apply(Box::new(SetObjectOpacity::new(id, v)), &mut doc).unwrap();
            }
            while hist.undo(&mut doc).unwrap() {}
            prop_assert_eq!(doc.object_count(), 0);
            prop_assert!(!hist.can_undo());

            // redoing the full history must reconstruct the object with the
            // same identity and the last applied opacity
            while hist.redo(&mut doc).unwrap() {}
            prop_assert_eq!(doc.object_count(), 1);
            prop_assert_eq!(doc.object(id).unwrap().opacity, *ops.last().expect("ops is non-empty"));
        }

        /// execute → undo → redo must be value-equivalent to execute alone.
        #[test]
        fn undo_redo_equivalence(v in 0.0f32..1.0) {
            let run = |redo: bool| {
                let mut doc = Document::new("prop");
                let page = doc.add_page((612.0, 792.0));
                let obj = doc.make_envelope(page, Rect::default());
                let id = obj.id;
                let mut hist = History::new(10);
                hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
                hist.apply(Box::new(SetObjectOpacity::new(id, v)), &mut doc).unwrap();
                if redo {
                    hist.undo(&mut doc).unwrap();
                    hist.redo(&mut doc).unwrap();
                }
                doc.object(id).unwrap().opacity
            };
            prop_assert_eq!(run(false), run(true));
        }
    }
}
