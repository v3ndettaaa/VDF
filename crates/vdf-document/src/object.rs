//! Object model: the common shape shared by every VDF object.

use vdf_core::{Affine2, ObjectId, PageId, Rect};

/// Discriminator for object payloads. Payload structs per type land with
/// their milestones (Ink → M2, Shape/TextBox/Image/... → M3); M0 defines the
/// common envelope they all live in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectType {
    Ink,
    Shape,
    TextBox,
    Image,
    Highlight,
    Note,
    Stamp,
    /// Redaction *mark* — the annotation-shaped object. True underlying
    /// content removal (pdf_redact) is an M5 persistence operation.
    RedactionMark,
    /// Mirrors a real PDF annotation owned by the source file.
    PdfAnnotation,
    Group,
}

impl ObjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectType::Ink => "ink",
            ObjectType::Shape => "shape",
            ObjectType::TextBox => "text",
            ObjectType::Image => "image",
            ObjectType::Highlight => "highlight",
            ObjectType::Note => "note",
            ObjectType::Stamp => "stamp",
            ObjectType::RedactionMark => "redaction-mark",
            ObjectType::PdfAnnotation => "pdf-annotation",
            ObjectType::Group => "group",
        }
    }
}

/// Common envelope for every object in a document (MASTER_PLAN.md §7).
/// Bounds are cached in document space (PDF points, y-up) and recomputed by
/// commands that change geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectModel {
    pub id: ObjectId,
    pub ty: ObjectType,
    pub page: PageId,
    pub transform: Affine2,
    pub bounds: Rect,
    /// z-order sequence value — never an array position.
    pub z: u32,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub name: Option<String>,
}

impl ObjectModel {
    /// A minimal object with identity and placement; everything else defaults.
    pub fn new(id: ObjectId, ty: ObjectType, page: PageId, bounds: Rect) -> Self {
        Self {
            id,
            ty,
            page,
            transform: Affine2::IDENTITY,
            bounds,
            z: 0,
            visible: true,
            locked: false,
            opacity: 1.0,
            name: None,
        }
    }
}
