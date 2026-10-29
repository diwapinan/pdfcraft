//! diw edit: Arrange (layer order), Group, Align and Distribute, and Copy / Paste / Duplicate for
//! added content (Edit a PDF ▸ Add content).
//!
//! The panel, the page's context menu and the keyboard all ask for the same [`ContentAction`];
//! the app runs it on the active document's selected added items, with the rest of their groups.

use std::sync::Arc;

use pdfcraft_engine::{AddedContent, AlignBoxes, ContentLayer, Document, Edit, ObjectKind};

use crate::canvas::DocView;

/// What the Arrange controls, the context menu and the shortcuts ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContentAction {
    Layer(ContentLayer),
    Group,
    Ungroup,
    Align(AlignBoxes),
    Copy,
    Paste,
    Duplicate,
}

/// One copied item: as it was, with its picture's file for an image, and its group then.
#[derive(Clone)]
pub(crate) struct ClipItem {
    content: AddedContent,
    file: Option<(String, Arc<Vec<u8>>)>,
    group: Option<u32>,
}

/// What Copy put away for Paste. `text` is what Copy also put on the system clipboard: Paste uses
/// these items while the system clipboard still holds it, and leaves a newer copy (a screenshot, text
/// from another program) to the paste it is for.
#[derive(Clone, Default)]
pub(crate) struct ContentClip {
    items: Vec<ClipItem>,
    text: String,
}

impl ContentClip {
    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// The added items a view has selected: one (the Add content selection) or several (Edit text &
/// images, with Shift or an area). The page and their indexes among its added items.
pub(crate) fn selected_added(view: &DocView) -> Option<(usize, Vec<usize>)> {
    if view.content.draft.is_some() {
        return None;
    }
    if let Some((page, index)) = view.content.selected {
        return Some((page, vec![index]));
    }
    view.objects.added_selection()
}

/// Group members selected together: a click on one item of a group (the single Add content
/// selection) selects the whole group, in Edit text & images, where several objects can be
/// selected, so it moves, copies and deletes as one. A paste's items are selected once they are
/// on the page.
pub(crate) fn follow_groups(view: &mut DocView, doc: &Document, multi: bool) {
    if view.content.draft.is_some() {
        return;
    }
    if !multi {
        // Without several-object selection (the Add text tool), the first item is selected.
        if let Some((page, indices)) = view.content_select_after.take() {
            view.content.selected = indices.first().map(|i| (page, *i));
        }
        return;
    }
    // Several objects selected: a group joins whole (Shift+click or an area over one member
    // selects its group) and leaves whole (Shift+click on a selected member deselects the group).
    if view.content_select_after.is_none() {
        let current = view.objects.added_selection();
        if current != view.content_group_seen {
            if let Some((page, now)) = &current {
                let before = view.content_group_seen.as_ref().filter(|(p, _)| p == page).map(|(_, b)| b.clone()).unwrap_or_default();
                let removed: Vec<usize> = before.iter().copied().filter(|i| !now.contains(i)).collect();
                let left_groups: Vec<u32> = removed.iter().filter_map(|i| doc.content_group(*page, *i)).collect();
                let drop: Vec<usize> =
                    now.iter().copied().filter(|i| doc.content_group(*page, *i).is_some_and(|g| left_groups.contains(&g))).collect();
                let keep: Vec<usize> = now.iter().copied().filter(|i| !drop.contains(i)).collect();
                let whole = doc.content_with_groups(*page, &keep);
                let add: Vec<usize> = whole.iter().copied().filter(|i| !now.contains(i)).collect();
                if (!add.is_empty() || !drop.is_empty())
                    && let Ok(objects) = doc.editable_objects(*page)
                {
                    let add = objects.into_iter().filter(|o| o.target.kind == ObjectKind::Added && add.contains(&o.target.index)).collect();
                    view.objects.regroup(add, &drop);
                }
            }
            view.content_group_seen = view.objects.added_selection();
        }
    }
    let want = match view.content_select_after.take() {
        Some(after) => Some(after),
        None => view.content.selected.and_then(|(page, index)| {
            doc.content_group(page, index)?;
            Some((page, doc.content_with_groups(page, &[index])))
        }),
    };
    let Some((page, indices)) = want.filter(|(_, i)| !i.is_empty()) else { return };
    if indices.len() == 1 {
        view.content.selected = indices.first().map(|i| (page, *i));
        return;
    }
    let Ok(objects) = doc.editable_objects(page) else { return };
    let chosen: Vec<_> = objects.into_iter().filter(|o| o.target.kind == ObjectKind::Added && indices.contains(&o.target.index)).collect();
    if chosen.len() == indices.len() {
        view.content.selected = None;
        view.objects.select_objects(page, chosen);
        view.content_group_seen = view.objects.added_selection();
    }
}

/// The wording of each action, for buttons and menus (translated where it is shown).
pub(crate) fn label(action: ContentAction) -> &'static str {
    match action {
        ContentAction::Layer(ContentLayer::Front) => "Bring to Front",
        ContentAction::Layer(ContentLayer::Forward) => "Bring Forward",
        ContentAction::Layer(ContentLayer::Backward) => "Send Backward",
        ContentAction::Layer(ContentLayer::Back) => "Send to Back",
        ContentAction::Group => "Group",
        ContentAction::Ungroup => "Ungroup",
        ContentAction::Align(AlignBoxes::Left) => "Align left",
        ContentAction::Align(AlignBoxes::Center) => "Align centers",
        ContentAction::Align(AlignBoxes::Right) => "Align right",
        ContentAction::Align(AlignBoxes::Top) => "Align top",
        ContentAction::Align(AlignBoxes::Middle) => "Align middles",
        ContentAction::Align(AlignBoxes::Bottom) => "Align bottom",
        ContentAction::Align(AlignBoxes::DistributeH) => "Distribute horizontally",
        ContentAction::Align(AlignBoxes::DistributeV) => "Distribute vertically",
        ContentAction::Copy => "Copy",
        ContentAction::Paste => "Paste",
        ContentAction::Duplicate => "Duplicate",
    }
}

/// The shortcut each action has, as [`keys`] reads it.
pub(crate) fn shortcut(action: ContentAction) -> Option<egui::KeyboardShortcut> {
    use egui::{Key, KeyboardShortcut as K, Modifiers as M};
    let shift = M::COMMAND.plus(M::SHIFT);
    Some(match action {
        ContentAction::Layer(ContentLayer::Front) => K::new(shift, Key::CloseBracket),
        ContentAction::Layer(ContentLayer::Forward) => K::new(M::COMMAND, Key::CloseBracket),
        ContentAction::Layer(ContentLayer::Backward) => K::new(M::COMMAND, Key::OpenBracket),
        ContentAction::Layer(ContentLayer::Back) => K::new(shift, Key::OpenBracket),
        ContentAction::Group => K::new(M::COMMAND, Key::G),
        ContentAction::Ungroup => K::new(shift, Key::G),
        ContentAction::Duplicate => K::new(M::COMMAND, Key::D),
        _ => return None,
    })
}

/// The arrange and group shortcuts, while added content is selected and nothing else has the
/// keyboard. Copy and Paste come as events (egui turns ⌘C and ⌘V into them) and are read by the app.
pub(crate) fn keys(ctx: &egui::Context, view: &mut DocView) {
    if ctx.egui_wants_keyboard_input() || view.pending_edit.is_some() || selected_added(view).is_none() {
        return;
    }
    let actions = [
        // Shift first: a plain ⌘ pattern also matches ⌘⇧.
        ContentAction::Layer(ContentLayer::Front),
        ContentAction::Layer(ContentLayer::Back),
        ContentAction::Ungroup,
        ContentAction::Layer(ContentLayer::Forward),
        ContentAction::Layer(ContentLayer::Backward),
        ContentAction::Group,
        ContentAction::Duplicate,
    ];
    for action in actions {
        if let Some(s) = shortcut(action)
            && ctx.input_mut(|i| i.consume_shortcut(&s))
        {
            view.content_action = Some(action);
            return;
        }
    }
}

/// The page's context menu over selected added content: Copy, Paste, Duplicate, Arrange, Group and
/// Align. Returns whether it showed anything.
pub(crate) fn context_menu(ui: &mut egui::Ui, view: &mut DocView, can_modify: bool, can_paste: bool) -> bool {
    let Some((_, indices)) = selected_added(view) else {
        if can_paste && can_modify {
            if ui.button(tl!("Paste")).clicked() {
                view.content_action = Some(ContentAction::Paste);
                ui.close();
            }
            ui.separator();
        }
        return false;
    };
    let several = indices.len() > 1;
    let mut pick = |ui: &mut egui::Ui, action: ContentAction, enabled: bool| {
        let key = shortcut(action).map(|s| ui.ctx().format_shortcut(&s)).unwrap_or_default();
        if ui.add_enabled(enabled, egui::Button::new(tl!(label(action))).shortcut_text(key)).clicked() {
            view.content_action = Some(action);
            ui.close();
        }
    };
    pick(ui, ContentAction::Copy, true);
    pick(ui, ContentAction::Paste, can_paste && can_modify);
    pick(ui, ContentAction::Duplicate, can_modify);
    ui.separator();
    ui.menu_button(tl!("Arrange"), |ui| {
        for layer in [ContentLayer::Front, ContentLayer::Forward, ContentLayer::Backward, ContentLayer::Back] {
            pick(ui, ContentAction::Layer(layer), can_modify);
        }
    });
    ui.menu_button(tl!("Align"), |ui| {
        for how in AlignBoxes::ALL {
            pick(ui, ContentAction::Align(how), can_modify && (!how.is_distribute() || indices.len() >= 3));
        }
    });
    pick(ui, ContentAction::Group, can_modify && several);
    pick(ui, ContentAction::Ungroup, can_modify);
    ui.separator();
    true
}

/// The Arrange section of the right panel for selected added content.
pub(crate) fn panel(ui: &mut egui::Ui, view: &mut DocView, can_modify: bool, can_paste: bool) {
    let Some((_, indices)) = selected_added(view) else { return };
    let count = indices.len();
    crate::widgets::section_title(ui, tl!("Arrange"));
    let mut pick = |ui: &mut egui::Ui, action: ContentAction, enabled: bool, width: f32| {
        let mut tip = tl!(label(action)).to_string();
        if let Some(s) = shortcut(action) {
            tip = format!("{tip} ({})", ui.ctx().format_shortcut(&s));
        }
        if ui.add_enabled(enabled, egui::Button::new(tl!(label(action))).min_size(egui::vec2(width, 26.0))).on_hover_text(tip).clicked() {
            view.content_action = Some(action);
        }
    };
    let half = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(60.0);
    let third = ((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(40.0);
    ui.label(egui::RichText::new(tl!("Layer")).small());
    egui::Grid::new("content-layer").num_columns(2).show(ui, |ui| {
        pick(ui, ContentAction::Layer(ContentLayer::Front), can_modify, half);
        pick(ui, ContentAction::Layer(ContentLayer::Back), can_modify, half);
        ui.end_row();
        pick(ui, ContentAction::Layer(ContentLayer::Forward), can_modify, half);
        pick(ui, ContentAction::Layer(ContentLayer::Backward), can_modify, half);
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tl!("Align")).small());
    egui::Grid::new("content-align").num_columns(3).show(ui, |ui| {
        for row in [[AlignBoxes::Left, AlignBoxes::Center, AlignBoxes::Right], [AlignBoxes::Top, AlignBoxes::Middle, AlignBoxes::Bottom]] {
            for how in row {
                pick(ui, ContentAction::Align(how), can_modify, third);
            }
            ui.end_row();
        }
    });
    if count == 1 {
        ui.label(egui::RichText::new(tl!("One item lines up with the page.")).small().weak());
    }
    egui::Grid::new("content-distribute").num_columns(2).show(ui, |ui| {
        pick(ui, ContentAction::Align(AlignBoxes::DistributeH), can_modify && count >= 3, half);
        pick(ui, ContentAction::Align(AlignBoxes::DistributeV), can_modify && count >= 3, half);
        ui.end_row();
    });
    ui.add_space(4.0);
    egui::Grid::new("content-group").num_columns(2).show(ui, |ui| {
        pick(ui, ContentAction::Group, can_modify && count > 1, half);
        pick(ui, ContentAction::Ungroup, can_modify, half);
        ui.end_row();
    });
    ui.add_space(4.0);
    egui::Grid::new("content-clip").num_columns(3).show(ui, |ui| {
        pick(ui, ContentAction::Copy, true, third);
        pick(ui, ContentAction::Paste, can_modify && can_paste, third);
        pick(ui, ContentAction::Duplicate, can_modify, third);
        ui.end_row();
    });
    ui.add_space(6.0);
    ui.separator();
}

/// Whether clipboard text `a` is what Copy put there, `b`, whatever line ends the system gave it.
fn same_text(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace("\r\n", "\n").trim_end().to_string();
    norm(a) == norm(b)
}

/// The items of `indices` on `page` as Copy keeps them. Refused when an image's picture can't be read.
fn copy_items(doc: &Document, page: usize, indices: &[usize]) -> Result<Vec<ClipItem>, String> {
    let items: Vec<_> = doc.added().iter().filter(|a| a.page == page).collect();
    indices
        .iter()
        .filter_map(|i| items.get(*i).map(|a| (*i, *a)))
        .map(|(i, a)| {
            let file = match &a.content {
                AddedContent::Text(_) => None,
                AddedContent::Image(_) => {
                    let (ext, bytes) = doc.added_image_file(page, i)?;
                    Some((format!("Pasted image.{ext}"), Arc::new(bytes)))
                }
            };
            Ok(ClipItem { content: a.content.clone(), file, group: a.group })
        })
        .collect()
}

/// What Copy puts on the system clipboard for `items`: the copied text, so it pastes as text
/// elsewhere too, or a name when only pictures were copied.
fn clipboard_text(items: &[ClipItem]) -> String {
    let text: Vec<&str> = items
        .iter()
        .filter_map(|c| match &c.content {
            AddedContent::Text(t) => Some(t.text.as_str()),
            AddedContent::Image(_) => None,
        })
        .collect();
    if text.is_empty() { format!("PdfCraft content ({} items)", items.len()) } else { text.join("\n") }
}

/// The edit that pastes `items` on `page` of `doc` (one undo step), and the indexes the pasted items
/// will have. The items keep their places among themselves; together they go where they were, or
/// as many paste steps down and to the right as it takes not to land on copies of them. Copied
/// groups become new groups.
fn paste_edit(doc: &Document, page: usize, items: &[ClipItem], label: &str) -> Option<(Edit, Vec<usize>)> {
    let first = items.first()?;
    let info = doc.info.pages.get(page)?;
    let size = [f64::from(info.width), f64::from(info.height)];
    let on_page: Vec<_> = doc.added().iter().filter(|a| a.page == page).collect();
    let placed = pdfcraft_engine::added_paste_place(on_page.iter().map(|a| &a.content), &first.content, size);
    let from = first.content.rect();
    let d = [placed[0].min(placed[2]) - from[0].min(from[2]), placed[1].max(placed[3]) - from[1].max(from[3])];
    let d = if d.iter().all(|v| v.is_finite()) { d } else { [0.0, 0.0] };
    let before = on_page.len();
    let mut edits: Vec<Edit> = items
        .iter()
        .map(|c| Edit::PasteContent {
            page,
            content: c.content.with_rect(pdfcraft_engine::added_offset_rect(c.content.rect(), d)),
            file: c.file.clone(),
        })
        .collect();
    let pasted: Vec<usize> = (before..before + items.len()).collect();
    let mut groups: Vec<u32> = items.iter().filter_map(|c| c.group).collect();
    groups.sort_unstable();
    groups.dedup();
    for g in groups {
        let members: Vec<usize> = items.iter().zip(&pasted).filter(|(c, _)| c.group == Some(g)).map(|(_, i)| *i).collect();
        if members.len() > 1 {
            edits.push(Edit::GroupContent { page, indices: members, group: true });
        }
    }
    let edit = if edits.len() == 1 { edits.pop()? } else { Edit::Batch { label: label.into(), edits } };
    Some((edit, pasted))
}

impl crate::PdfCraftApp {
    /// Run the content action the active view asked for this frame (panel, menu or keys).
    pub(crate) fn content_actions(&mut self, ctx: &egui::Context) {
        let Some(i) = self.active else { return };
        let Some(action) = self.views.get_mut(i).and_then(|v| v.content_action.take()) else { return };
        self.run_content_action(i, action, ctx);
    }

    /// ⌘V with this app's copied content still on the system clipboard pastes the content; returns
    /// whether it did (otherwise the paste is left to the picture and text pastes).
    pub(crate) fn paste_content_key(&mut self, i: usize, ctx: &egui::Context, pasted: bool) -> bool {
        let free = self.dialog.is_none() && !self.palette_open && !ctx.egui_wants_keyboard_input();
        if !pasted || !free || self.content_clip.is_empty() {
            return false;
        }
        let Some(view) = self.views.get(i) else { return false };
        if view.organize || view.forms.focus.is_some() || self.is_preparing() {
            return false;
        }
        let clip = match self.clipboard_override.clone() {
            Some(c) => Some(c),
            #[cfg(not(target_arch = "wasm32"))]
            None if self.system_clipboard => crate::create_ui::read_clipboard(),
            None => None,
        };
        // Without a system clipboard (the web, tests) the copied content is the clipboard.
        let ours = match clip {
            // Line ends as the system keeps them (Windows may hand back \r\n).
            Some(crate::create_ui::Clip::Text(t)) => same_text(&t, &self.content_clip.text),
            Some(crate::create_ui::Clip::Image { .. }) => false,
            None => true,
        };
        if !ours {
            return false;
        }
        self.run_content_action(i, ContentAction::Paste, ctx);
        true
    }

    fn run_content_action(&mut self, i: usize, action: ContentAction, ctx: &egui::Context) {
        let Some(view) = self.views.get(i) else { return };
        let id = view.id;
        let Some(doc) = self.session.get(id) else { return };
        let selection = selected_added(view).map(|(page, indices)| (page, doc.content_with_groups(page, &indices)));
        match action {
            ContentAction::Copy | ContentAction::Duplicate => {
                let Some((page, indices)) = selection else { return };
                // Copying takes content out of the document, which its security may forbid;
                // duplicating keeps it in the document.
                if action == ContentAction::Copy && !self.may_copy(id) {
                    return;
                }
                let Some(doc) = self.session.get(id) else { return };
                let items = match copy_items(doc, page, &indices) {
                    Ok(items) => items,
                    Err(e) => return self.notify(e),
                };
                if action == ContentAction::Copy {
                    let text = clipboard_text(&items);
                    ctx.copy_text(text.clone());
                    self.content_clip = ContentClip { items, text };
                    return;
                }
                if let Some((edit, pasted)) = paste_edit(doc, page, &items, "Duplicate") {
                    self.apply_content_edit(i, edit, page, pasted);
                }
            }
            ContentAction::Paste => {
                let page = selection.map(|(p, _)| p).or(self.views.get(i).map(|v| v.current)).unwrap_or(0);
                let items = self.content_clip.items.clone();
                let Some((edit, pasted)) = self.session.get(id).and_then(|doc| paste_edit(doc, page, &items, "Paste")) else { return };
                self.apply_content_edit(i, edit, page, pasted);
            }
            ContentAction::Layer(layer) => {
                let Some((page, indices)) = selection else { return };
                // The items stay selected where the reorder puts them.
                let n = doc.added().iter().filter(|a| a.page == page).count();
                let after = pdfcraft_engine::arranged_content_positions(n, &indices, layer);
                self.apply_content_edit(i, Edit::ArrangeContent { page, indices, layer }, page, after);
            }
            ContentAction::Group | ContentAction::Ungroup => {
                let Some((page, indices)) = selection else { return };
                let group = action == ContentAction::Group;
                self.apply_content_edit(
                    i,
                    Edit::GroupContent { page, indices: indices.clone(), group },
                    page,
                    if group { indices } else { Vec::new() },
                );
            }
            ContentAction::Align(how) => {
                let Some((page, indices)) = selection else { return };
                match doc.align_content(page, &indices, how) {
                    Ok(edit) => self.apply_content_edit(i, edit, page, indices),
                    Err(e) => self.notify(tl!(&e).to_string()),
                }
            }
        }
    }

    /// Apply a content edit; afterwards `select` is selected on `page` (a paste's items, the
    /// reordered or aligned items, where they now are). An edit refused says why.
    fn apply_content_edit(&mut self, i: usize, edit: Edit, page: usize, select: Vec<usize>) {
        if self.apply_edit(edit)
            && let Some(view) = self.views.get_mut(i)
            && !select.is_empty()
        {
            view.content.selected = None;
            view.objects.select_objects(page, Vec::new());
            view.content_select_after = Some((page, select));
        }
    }
}
