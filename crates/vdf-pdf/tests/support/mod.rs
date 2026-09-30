//! Test fixture generator: hand-crafted, fully valid PDFs with known
//! content (page-numbered text + one page-unique colored mark), used by
//! rendering/golden/large-document tests. No external PDF tools needed.
//!
//! Coordinate note: content is emitted in PDF user space (y-up). MuPDF page
//! space (what `vdf-pdf::render_region` consumes) is y-down: a mark at user
//! (x, y) with page height H appears at page space (x, H − y).

/// Generates a valid N-page PDF. Each page has:
/// - "Page i" text at 72,700 (user space)
/// - a unique-color square (64×64) at user (484, 700) → page-space top-right
///
/// Colors cycle through a distinct palette so pages are visually separable.
pub fn make_pdf(pages: usize) -> Vec<u8> {
    assert!(pages >= 1);
    let mut objects: Vec<Vec<u8>> = Vec::new();

    // 1: catalog
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    // 2: page tree (kids filled after we know the count)
    // 3..: per page: [page dict, content stream]
    // last: font

    let font_obj_num = (3 + pages * 2) as u32;
    let mut kids = String::from("[");
    for i in 0..pages as u32 {
        let page_num = 3 + i * 2;
        kids.push_str(&format!("{page_num} 0 R "));
    }
    kids.push(']');
    objects.push(format!("<< /Type /Pages /Kids {kids} /Count {pages} >>").into_bytes());

    for i in 0..pages {
        let page_num = 3 + i * 2;
        let content_num = page_num + 1;
        let (r, g, b) = palette(i);
        let content = format!(
            "BT /F1 36 Tf 72 700 Td (Page {page}) Tj ET\n{r} {g} {b} rg 484 700 64 64 re f\n",
            page = i + 1,
        );
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                 /Contents {content_num} 0 R /Resources << /Font << /F1 {font_obj_num} 0 R >> >> >>"
            )
            .into_bytes(),
        );
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{content}endstream",
                content.len()
            )
            .into_bytes(),
        );
    }
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());

    // Assemble with a correct xref table.
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::with_capacity(objects.len() + 1);
    for (idx, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj", idx + 1).as_bytes());
        out.push(b'\n');
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len();
    let count = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

/// Distinct per-page mark color (deterministic, well-separated).
pub fn palette(i: usize) -> (f32, f32, f32) {
    const COLORS: [(f32, f32, f32); 8] = [
        (1.0, 0.0, 0.0),  // red
        (0.0, 0.55, 0.0), // green
        (0.0, 0.0, 1.0),  // blue
        (1.0, 0.6, 0.0),  // orange
        (0.5, 0.0, 0.5),  // purple
        (0.0, 0.7, 0.7),  // teal
        (0.8, 0.8, 0.0),  // yellow-ish
        (0.5, 0.25, 0.1), // brown
    ];
    COLORS[i % COLORS.len()]
}
