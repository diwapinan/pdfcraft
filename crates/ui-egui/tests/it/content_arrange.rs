//! diw edit: Arrange, Group, Align and Copy / Paste / Duplicate for added content, through the
//! real shell (egui_kittest): mouse selection and the keyboard shortcuts.

use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::{AddedContent, AddedText, Edit};
use pdfcraft_ui_egui::PdfCraftApp;

/// A blank 400 × 500 pt page.
fn blank() -> Vec<u8> {
    let objects =
        ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>", "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 500] >>"];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// Three added texts, A, B and C (display space, y up), in Edit text & images.
const BOXES: [(&str, [f64; 4]); 3] = [("A", [40.0, 400.0, 140.0, 420.0]), ("B", [200.0, 300.0, 300.0, 320.0]), ("C", [60.0, 100.0, 160.0, 120.0])];

fn harness() -> Harness<'static, PdfCraftApp> {
    harness_in("edit.edit_text")
}

fn harness_in(tool: &'static str) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        // The test's own clipboard: no system clipboard to race other tests on.
        app.system_clipboard = false;
        app.open_bytes("arrange.pdf", None, blank()).unwrap();
        let id = app.views[0].id;
        for (text, rect) in BOXES {
            app.session.apply(id, Edit::AddText { page: 0, text: AddedText { text: text.into(), rect, ..Default::default() } }).unwrap();
        }
        app.set_option("zoom", "100").unwrap();
        assert!(app.execute(tool));
        app
    });
    crate::support::settle(&mut h);
    h
}

/// The screen point of display-space point (x, y up) on the page.
fn at(h: &Harness<'static, PdfCraftApp>, x: f64, y: f64) -> Pos2 {
    let r = h.state().views[0].page_screen_rect(0).unwrap();
    let k = r.width() / 400.0;
    r.min + egui::vec2(x as f32 * k, (500.0 - y) as f32 * k)
}

fn shift_click(h: &mut Harness<'static, PdfCraftApp>, p: Pos2) {
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event_modifiers(
            egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::SHIFT },
            Modifiers::SHIFT,
        );
    }
    h.run_steps(4);
}

fn key(h: &mut Harness<'static, PdfCraftApp>, mods: Modifiers, key: Key) {
    h.key_press_modifiers(mods, key);
    h.run_steps(4);
}

fn added(h: &Harness<'static, PdfCraftApp>) -> Vec<pdfcraft_engine::Added> {
    let s = h.state();
    s.session.get(s.views[0].id).unwrap().added().to_vec()
}

fn texts(h: &Harness<'static, PdfCraftApp>) -> String {
    added(h)
        .iter()
        .map(|a| match &a.content {
            AddedContent::Text(t) => t.text.clone(),
            AddedContent::Image(_) => "?".into(),
        })
        .collect()
}

fn centre(i: usize) -> (f64, f64) {
    let r = BOXES[i].1;
    ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
}

fn select(h: &mut Harness<'static, PdfCraftApp>, items: &[usize]) {
    key(h, Modifiers::NONE, Key::Escape);
    for &i in items {
        let (x, y) = centre(i);
        let p = at(h, x, y);
        shift_click(h, p);
    }
}

#[test]
fn shortcuts_group_reorder_duplicate_and_align_selected_content() {
    let mut h = harness();
    select(&mut h, &[0, 1]);
    h.get_by_label("2 objects selected"); // A and B are selected

    key(&mut h, Modifiers::COMMAND, Key::G);
    let groups: Vec<_> = added(&h).iter().map(|a| a.group).collect();
    assert!(groups[0].is_some() && groups[0] == groups[1] && groups[2].is_none(), "A and B are grouped: {groups:?}");

    // Shift+click on one member selects the whole group.
    select(&mut h, &[1]);
    h.get_by_label("2 objects selected"); // a member brings its group

    key(&mut h, Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::OpenBracket);
    assert_eq!(texts(&h), "ABC", "already at the back: nothing moves");
    key(&mut h, Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::CloseBracket);
    assert_eq!(texts(&h), "CAB", "the group comes to the front together");
    h.get_by_label("2 objects selected"); // and stays selected where it now is

    key(&mut h, Modifiers::COMMAND, Key::D);
    let after = added(&h);
    assert_eq!(texts(&h), "CABAB", "Duplicate copies the whole group");
    assert!(after[3].group.is_some() && after[3].group == after[4].group && after[3].group != after[1].group, "the copies form a new group");
    assert!(after[3].content.rect()[0] > after[1].content.rect()[0], "the copies are stepped off the originals");

    // Align the selected copies' group with C: left edges meet.
    h.state_mut().apply_edit(Edit::DeleteContent { page: 0, index: 4 });
    h.state_mut().apply_edit(Edit::DeleteContent { page: 0, index: 3 });
    h.run_steps(4);
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    let edit = doc.align_content(0, &[0, 1], pdfcraft_engine::AlignBoxes::Left).unwrap();
    assert!(h.state_mut().apply_edit(edit));
    let left: Vec<f64> = added(&h).iter().map(|a| a.content.rect()[0]).collect();
    assert_eq!(left, [40.0, 40.0, 200.0], "C lines up with the group (A at 40); B keeps its place in the group");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Align"), "one undo step");
}

#[test]
fn copy_and_paste_selected_content_keeps_its_group() {
    let mut h = harness();
    select(&mut h, &[0, 2]);
    key(&mut h, Modifiers::COMMAND, Key::G);
    select(&mut h, &[0]);
    h.get_by_label("2 objects selected"); // two items are selected
    h.event(egui::Event::Copy);
    h.run_steps(4);
    // ⌘V comes as the release of V (egui-winit keeps the press).
    h.event(egui::Event::Key { key: Key::V, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(6);
    let after = added(&h);
    assert_eq!(texts(&h), "ABCAC", "Paste puts down the copied group");
    assert!(after[3].group.is_some() && after[3].group == after[4].group && after[3].group != after[0].group);
    h.get_by_label("2 objects selected"); // the pasted items are selected
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Paste"), "one undo step");
}

/// diw edit: the Add text tool selects several added items with Shift+click too, so they can be
/// grouped there; a plain click on the page still starts new text, and no area is drawn.
#[test]
fn the_add_text_tool_selects_several_items_with_shift_and_groups_them() {
    let mut h = harness_in("edit.text");
    select(&mut h, &[0, 1]);
    h.get_by_label("2 objects selected");
    key(&mut h, Modifiers::COMMAND, Key::G);
    let groups: Vec<_> = added(&h).iter().map(|a| a.group).collect();
    assert!(groups[0].is_some() && groups[0] == groups[1] && groups[2].is_none(), "{groups:?}");
    // Shift+click on empty page adds nothing and starts no text.
    let empty = at(&h, 350.0, 50.0);
    shift_click(&mut h, empty);
    assert!(h.state().views[0].content.draft.is_none(), "no new text from a Shift+click");
    assert_eq!(added(&h).len(), 3);
}
