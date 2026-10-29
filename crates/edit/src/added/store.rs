use super::draw::{draw, params};
use super::layout::{first_undrawable, has_thai, is_thai, nums, shaped_arabic, thai_error};
use super::types::{Added, AddedImage, AddedText, Align, CHAR_SPACING_LIMIT, Family, Layer, ListStyle, MarkerScan, TAG};
use super::{Content, Dict, Document, EditError, GlyphError, MarkKind, ObjRef, Object, Stream, check, contents, page_list, place_tagged};

fn parse(doc: &Document, d: &Dict) -> Option<Content> {
    let r = nums(doc, d.get(b"Rect"));
    let rect: [f64; 4] = r.try_into().ok()?;
    match d.name(b"Kind")? {
        b"Text" => {
            let (family, bold, italic) = Family::from_base(d.name(b"Font").unwrap_or(b"Helvetica"));
            let c = nums(doc, d.get(b"Color"));
            Some(Content::Text(AddedText {
                rect,
                text: d.get(b"Text").and_then(|t| doc.resolve(t).as_string().map(|s| s.to_text())).unwrap_or_default(),
                family,
                bold,
                italic,
                size: d.get(b"Size").and_then(Object::as_f64).unwrap_or(12.0),
                color: if c.len() == 3 { [c[0], c[1], c[2]] } else { [0.0; 3] },
                align: match d.int(b"Align") {
                    Some(1) => Align::Center,
                    Some(2) => Align::Right,
                    Some(3) => Align::Justify,
                    _ => Align::Left,
                },
                // Items written before character spacing have none.
                char_spacing: d.get(b"CharSpacing").and_then(Object::as_f64).filter(|v| v.is_finite()).unwrap_or(0.0),
                // Items written before backgrounds, or with a malformed one, have none.
                background: <[f64; 3]>::try_from(nums(doc, d.get(b"Background")))
                    .ok()
                    .filter(|c| c.iter().all(|v| v.is_finite()))
                    .map(|c| c.map(|v| v.clamp(0.0, 1.0))),
                // Items written before lists, or with a number this build doesn't know, are no list.
                list: d.int(b"List").map_or(ListStyle::None, ListStyle::from_code),
            }))
        }
        b"Image" => {
            let crop = nums(doc, d.get(b"Crop"));
            Some(Content::Image(AddedImage {
                rect,
                image: d.get(b"Image")?.as_ref()?,
                rotation: (d.int(b"Rotate").unwrap_or(0).rem_euclid(360) / 90) as u8,
                flip_h: matches!(d.get(b"FlipH"), Some(Object::Bool(true))),
                flip_v: matches!(d.get(b"FlipV"), Some(Object::Bool(true))),
                crop: crop.try_into().unwrap_or([0.0; 4]),
            }))
        }
        _ => None,
    }
}

fn with_resolved<T>(doc: &Document, object: &Object, f: impl FnOnce(&Object) -> T) -> T {
    match object {
        Object::Ref(reference) => {
            let resolved = doc.get(*reference);
            f(&resolved)
        }
        direct => f(direct),
    }
}

fn for_each_content_entry(doc: &Document, page: &pdfcraft_model::Page, mut visit: impl FnMut(&Object)) {
    let Some(contents) = page.dict.get(b"Contents") else { return };
    with_resolved(doc, contents, |resolved| match resolved {
        Object::Array(entries) => {
            for entry in entries {
                visit(entry);
            }
        }
        _ => visit(contents),
    });
}

/// Finds page marks and editable added content in one pass over the page content entries.
pub fn scan_markers(doc: &Document) -> MarkerScan {
    let mut scan = MarkerScan::default();
    for (pi, page) in page_list(doc).iter().enumerate() {
        for_each_content_entry(doc, page, |entry| {
            with_resolved(doc, entry, |resolved| {
                let Some(dict) = resolved.as_dict() else { return };
                let tag = dict.name(b"PCMark");
                let kind = if tag == Some(&b"HeaderFooter"[..]) {
                    Some(MarkKind::HeaderFooter)
                } else if tag == Some(&b"Watermark"[..]) {
                    Some(MarkKind::Watermark)
                } else if tag == Some(&b"Background"[..]) {
                    Some(MarkKind::Background)
                } else {
                    None
                };
                if let Some(kind) = kind
                    && !scan.marks.contains(&kind)
                {
                    scan.marks.push(kind);
                }

                let Some(obj) = Object::as_ref(entry) else { return };
                if tag != Some(TAG.as_bytes()) {
                    return;
                }
                let Some(params_object) = dict.get(b"PCAdded") else { return };
                with_resolved(doc, params_object, |resolved_params| {
                    let Some(params) = resolved_params.as_dict() else { return };
                    if let Some(content) = parse(doc, params) {
                        // Display points depend on the page's /UserUnit. An item records the one it was
                        // written for; none means 1, as for items written before display space followed
                        // /UserUnit.
                        let written = params
                            .get(b"UserUnit")
                            .and_then(Object::as_f64)
                            .filter(|u| u.is_finite() && *u >= 1.0)
                            .map_or(1.0, |u| u.min(pdfcraft_model::MAX_USER_UNIT));
                        scan.added.push(Added { page: pi, obj, content: content.scaled(page.user_unit(doc) / written), group: group_of(params) });
                    }
                });
            });
        });
    }
    scan
}

/// The group recorded in an item's parameters: a whole number from 1 up, else none.
fn group_of(params: &Dict) -> Option<u32> {
    params.int(b"Group").and_then(|g| u32::try_from(g).ok()).filter(|g| *g > 0)
}

/// The group recorded on the item whose stream is `obj`, if any.
fn stream_group(doc: &Document, obj: ObjRef) -> Option<u32> {
    match &*doc.get(obj) {
        Object::Stream(s) => s.dict.get(b"PCAdded").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().and_then(group_of)),
        _ => None,
    }
}

fn validate(c: &Content) -> Result<(), EditError> {
    let r = c.rect();
    if !r.iter().all(|v| v.is_finite()) {
        return Err(EditError::Invalid("invalid position".into()));
    }
    match c {
        Content::Text(t) => {
            if t.text.trim().is_empty() {
                return Err(EditError::Invalid("type some text first".into()));
            }
            if !(t.size.is_finite() && (1.0..=500.0).contains(&t.size)) {
                return Err(EditError::Invalid("the font size must be between 1 and 500 points".into()));
            }
            if !(t.char_spacing.is_finite() && (-CHAR_SPACING_LIMIT..=CHAR_SPACING_LIMIT).contains(&t.char_spacing)) {
                return Err(EditError::Invalid(format!(
                    "the character spacing must be between -{CHAR_SPACING_LIMIT} and {CHAR_SPACING_LIMIT} points"
                )));
            }
            if t.background.is_some_and(|c| !c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))) {
                return Err(EditError::Invalid("the background colour must have components between 0 and 1".into()));
            }
            if (r[2] - r[0]).abs() < 1.0 {
                return Err(EditError::Invalid("the text box is too narrow".into()));
            }
            if has_thai(t) && pdfcraft_fonts::document_thai_font().is_none() {
                return Err(thai_error(GlyphError::NoFont, &t.text));
            }
            // Without the Arabic face, `draw` decides for the Arabic letters themselves (#403): a new
            // item is refused with what's missing, and one that already holds Arabic stays movable.
            // Everything else in it is drawn in the standard font, so it still has to fit.
            let undrawable = if t.text.chars().any(shaped_arabic) && pdfcraft_fonts::document_arabic_font().is_none() {
                t.text.split('\n').find_map(|line| {
                    line.chars()
                        .find(|c| !(shaped_arabic(*c) || is_thai(*c) && pdfcraft_fonts::thai_has(*c) || pdfcraft_fonts::win_ansi_encodable(*c)))
                })
            } else {
                first_undrawable(t)
            };
            if let Some(c) = undrawable {
                return Err(crate::undrawable(c));
            }
        }
        Content::Image(_) => {
            if (r[2] - r[0]).abs() < 1.0 || (r[3] - r[1]).abs() < 1.0 {
                return Err(EditError::Invalid("the image is too small".into()));
            }
        }
    }
    Ok(())
}

/// Write an item's stream (new, or replacing `obj`) and make its resources available to the page.
fn write(doc: &mut Document, page: usize, c: &Content, obj: Option<ObjRef>) -> Result<ObjRef, EditError> {
    write_with(doc, page, c, obj, false)
}

/// [`write`], drawing shaped glyphs as filled paths with `paths` instead of Type 3 text.
fn write_with(doc: &mut Document, page: usize, c: &Content, obj: Option<ObjRef>, paths: bool) -> Result<ObjRef, EditError> {
    validate(c)?;
    let all = page_list(doc);
    check(&[page], all.len())?;
    let p = &all[page];
    let view = p.view_matrix(doc);
    let user_unit = p.user_unit(doc);
    // The page gets its own copy of its resources with the item's fonts and images added, and
    // without the Arabic fonts of the version it replaces.
    let mut pres = p.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    if let Some(r) = obj {
        drop_fonts(doc, &mut pres, &own_fonts(doc, r));
    }
    let taken = pres.get(b"Font").map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned()).unwrap_or_default();
    let (content, res) = draw(doc, c, view, &taken, obj.is_some(), paths)?;
    if obj.is_none() {
        crate::check_on_top(doc, p)?;
    }
    for (k, v) in res.iter() {
        let mut sub = pres.get(k).map(|s| doc.resolve(s)).and_then(|s| s.as_dict().cloned()).unwrap_or_default();
        for (n2, o) in v.as_dict().into_iter().flat_map(|d| d.iter()) {
            sub.set(n2.clone(), o.clone());
        }
        pres.set(k.clone(), Object::Dict(sub));
    }
    doc.update_dict(p.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(pres)))?;
    let mut sd = Dict::new();
    sd.set(b"PCMark".to_vec(), Object::name(TAG));
    let mut added = params(c);
    // A replaced item stays in its group (moving, retyping or restyling it keeps it grouped).
    if let Some(group) = obj.and_then(|r| stream_group(doc, r)) {
        added.set(b"Group".to_vec(), Object::Int(i64::from(group)));
    }
    if user_unit != 1.0 {
        added.set(b"UserUnit".to_vec(), Object::Real(user_unit));
    }
    sd.set(b"PCAdded".to_vec(), Object::Dict(added));
    let stream = Stream::flate(sd, &content);
    match obj {
        Some(r) => {
            doc.set(r, Object::Stream(stream));
            Ok(r)
        }
        None => {
            // place_tagged appends a new stream; recover its reference.
            let p = page_list(doc).swap_remove(page);
            place_tagged(doc, &p, TAG, content.clone(), false)?;
            let p = page_list(doc).swap_remove(page);
            let r = contents(doc, &p)?.last().and_then(Object::as_ref).ok_or_else(|| EditError::Invalid("could not add the content".into()))?;
            doc.set(r, Object::Stream(stream));
            Ok(r)
        }
    }
}

/// Every added item, page by page, in drawing order.
pub fn list_added(doc: &Document) -> Vec<Added> {
    scan_markers(doc).added
}

/// Add an item to a page (0-based). Returns its index among the page's added items.
pub fn add_content(doc: &mut Document, page: usize, c: &Content) -> Result<usize, EditError> {
    write(doc, page, c, None)?;
    Ok(list_added(doc).iter().filter(|a| a.page == page).count() - 1)
}

fn find(doc: &Document, page: usize, index: usize) -> Result<Added, EditError> {
    list_added(doc)
        .into_iter()
        .filter(|a| a.page == page)
        .nth(index)
        .ok_or_else(|| EditError::Invalid(format!("page {} has no added item {}", page + 1, index + 1)))
}

/// Replace item `index` of `page` (moved, resized, retyped or reformatted).
pub fn update_content(doc: &mut Document, page: usize, index: usize, c: &Content) -> Result<(), EditError> {
    let a = find(doc, page, index)?;
    if std::mem::discriminant(&a.content) != std::mem::discriminant(c) {
        return Err(EditError::Invalid("an item can't change between text and image".into()));
    }
    write(doc, page, c, Some(a.obj))?;
    Ok(())
}

/// Remove item `index` of `page`.
pub fn delete_content(doc: &mut Document, page: usize, index: usize) -> Result<(), EditError> {
    let a = find(doc, page, index)?;
    let p = page_list(doc).swap_remove(page);
    let list: Vec<Object> = contents(doc, &p)?.into_iter().filter(|o| o.as_ref() != Some(a.obj)).collect();
    let fonts = own_fonts(doc, a.obj);
    // Only the page's own resources carry an item's fonts (`write` gives the page a copy).
    let res = match p.dict.get(b"Resources") {
        Some(Object::Dict(d)) if !fonts.is_empty() => Some(d.clone()),
        _ => None,
    };
    let res = res.map(|mut r| {
        drop_fonts(doc, &mut r, &fonts);
        r
    });
    doc.update_dict(p.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Array(list));
        if let Some(r) = res {
            d.set(b"Resources".to_vec(), Object::Dict(r));
        }
    })?;
    Ok(())
}

/// The added items of `page`, in drawing order, and the indexes asked for: in range, each once,
/// in order. Refused when one is out of range or none is asked for.
fn page_items(doc: &Document, page: usize, indices: &[usize]) -> Result<(Vec<Added>, Vec<usize>), EditError> {
    let items: Vec<Added> = list_added(doc).into_iter().filter(|a| a.page == page).collect();
    let mut wanted: Vec<usize> = indices.to_vec();
    wanted.sort_unstable();
    wanted.dedup();
    if let Some(bad) = wanted.iter().find(|i| **i >= items.len()) {
        return Err(EditError::Invalid(format!("page {} has no added item {}", page + 1, bad.saturating_add(1))));
    }
    if wanted.is_empty() {
        return Err(EditError::Invalid("select added content first".into()));
    }
    Ok((items, wanted))
}

/// diw edit: put added items `indices` of `page` together in group `group`, or take them out of
/// their groups (`None`). Only the items' recorded parameters change; nothing is redrawn.
pub fn set_content_group(doc: &mut Document, page: usize, indices: &[usize], group: Option<u32>) -> Result<(), EditError> {
    let (items, wanted) = page_items(doc, page, indices)?;
    for index in wanted {
        let Some(item) = items.get(index) else { continue };
        let Object::Stream(mut stream) = (*doc.get(item.obj)).clone() else {
            return Err(EditError::Invalid("an added item's content is missing".into()));
        };
        let mut params = stream.dict.get(b"PCAdded").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().cloned()).unwrap_or_default();
        match group {
            Some(g) => params.set(b"Group".to_vec(), Object::Int(i64::from(g))),
            None => {
                params.remove(b"Group");
            }
        }
        stream.dict.set(b"PCAdded".to_vec(), Object::Dict(params));
        doc.set(item.obj, Object::Stream(stream));
    }
    Ok(())
}

/// diw edit: a group number no added item on `page` uses yet.
pub fn new_content_group(doc: &Document, page: usize) -> u32 {
    list_added(doc).iter().filter(|a| a.page == page).filter_map(|a| a.group).max().map_or(1, |g| g.saturating_add(1))
}

/// diw edit: Arrange ▸ Bring to Front, Bring Forward, Send Backward, Send to Back for added
/// items `indices` of `page`, which keep their order among themselves. Only the order of the
/// page's added items changes: the items trade the places in the page's content they hold, so
/// each still draws from the clean graphics state it was written for, and the page's own content
/// stays behind them. Returns the items' new indexes; refused when nothing would move.
pub fn arrange_content(doc: &mut Document, page: usize, indices: &[usize], layer: Layer) -> Result<Vec<usize>, EditError> {
    let (items, wanted) = page_items(doc, page, indices)?;
    let n = items.len();
    let order = arranged_order(n, &wanted, layer);
    if order.iter().copied().eq(0..n) {
        return Err(EditError::Invalid(
            match layer {
                Layer::Front | Layer::Forward => "the selection is already in front",
                Layer::Back | Layer::Backward => "the selection is already at the back",
            }
            .into(),
        ));
    }
    let p = page_list(doc).swap_remove(page);
    let mut list = contents(doc, &p)?;
    // The places the items hold in the page's content, in drawing order.
    let slots: Vec<usize> = items.iter().filter_map(|a| list.iter().position(|o| o.as_ref() == Some(a.obj))).collect();
    let distinct = slots.len() == n && list.iter().filter(|o| items.iter().any(|a| o.as_ref() == Some(a.obj))).count() == n;
    if !distinct {
        return Err(EditError::Invalid("this page's added content can't be reordered".into()));
    }
    for (slot, old) in slots.iter().zip(&order) {
        let Some(item) = items.get(*old) else { continue };
        if let Some(entry) = list.get_mut(*slot) {
            *entry = Object::Ref(item.obj);
        }
    }
    doc.update_dict(p.obj, |d| d.set(b"Contents".to_vec(), Object::Array(list)))?;
    Ok(arranged_positions(n, &wanted, layer))
}

/// diw edit: the drawing order (as old indexes) that `layer` gives `n` added items when items
/// `wanted` are chosen. Indexes out of range are ignored.
fn arranged_order(n: usize, wanted: &[usize], layer: Layer) -> Vec<usize> {
    let chosen: Vec<bool> = (0..n).map(|i| wanted.contains(&i)).collect();
    let is = |i: usize| chosen.get(i).copied().unwrap_or(false);
    let mut order: Vec<usize> = (0..n).collect();
    match layer {
        Layer::Front => order = (0..n).filter(|i| !is(*i)).chain((0..n).filter(|i| is(*i))).collect(),
        Layer::Back => order = (0..n).filter(|i| is(*i)).chain((0..n).filter(|i| !is(*i))).collect(),
        // Each chosen item passes the next item above it that isn't chosen; a run of chosen
        // items moves as one.
        Layer::Forward => {
            for i in (0..n.saturating_sub(1)).rev() {
                if let (Some(&a), Some(&b)) = (order.get(i), order.get(i + 1))
                    && is(a)
                    && !is(b)
                {
                    order.swap(i, i + 1);
                }
            }
        }
        Layer::Backward => {
            for i in 1..n {
                if let (Some(&a), Some(&b)) = (order.get(i), order.get(i - 1))
                    && is(a)
                    && !is(b)
                {
                    order.swap(i, i - 1);
                }
            }
        }
    }
    order
}

/// diw edit: where the chosen items `wanted` of `n` added items are after [`arrange_content`]
/// with `layer`: their new indexes, in drawing order. For an interface to keep them selected.
pub fn arranged_positions(n: usize, wanted: &[usize], layer: Layer) -> Vec<usize> {
    arranged_order(n, wanted, layer).iter().enumerate().filter(|(_, old)| wanted.contains(old)).map(|(new, _)| new).collect()
}

/// diw edit: redraw every added text that is drawn with Type 3 fonts (Thai, Arabic, Hebrew) with
/// its glyphs as filled paths instead, for a copy of the document that is only printed. Windows'
/// PDF renderer, which prints on Windows, crashes on those Type 3 fonts. The pages look the same;
/// the copy's added text is no longer text (not searchable), which a print doesn't need. Returns
/// how many items were redrawn.
pub fn shaped_text_as_paths(doc: &mut Document) -> Result<usize, EditError> {
    let mut redrawn = 0usize;
    for item in list_added(doc) {
        let Content::Text(t) = &item.content else { continue };
        if own_fonts(doc, item.obj).is_empty() {
            continue;
        }
        // Written for its page's /UserUnit; `list_added` scaled it to display points already.
        write_with(doc, item.page, &Content::Text(t.clone()), Some(item.obj), true)?;
        redrawn = redrawn.saturating_add(1);
    }
    Ok(redrawn)
}

/// Leave only item `index` on `page`: its contents become the item's own stream and its
/// annotations go, so a render of the page (on a transparent backdrop) is the item's pixels
/// alone. For previews on a copy of the document; the page's resources stay as they are.
pub fn isolate_content(doc: &mut Document, page: usize, index: usize) -> Result<(), EditError> {
    let a = find(doc, page, index)?;
    let p = page_list(doc).swap_remove(page);
    // An item's stream starts from the page's initial graphics state: `place_tagged` closes
    // everything drawn before it, so it draws the same on its own.
    doc.update_dict(p.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Array(vec![Object::Ref(a.obj)]));
        d.remove(b"Annots");
    })?;
    Ok(())
}

/// The Type 3 fonts an Arabic item's stream shows text with. Each item has its own (`draw_shaped`
/// names them so), so they go when the item is replaced or deleted.
fn own_fonts(doc: &Document, obj: ObjRef) -> Vec<Vec<u8>> {
    let Object::Stream(s) = &*doc.get(obj) else { return Vec::new() };
    let Ok(data) = s.decoded() else { return Vec::new() };
    data.split(u8::is_ascii_whitespace).filter_map(|w| w.strip_prefix(b"/")).filter(|w| w.starts_with(b"PCAr")).map(<[u8]>::to_vec).collect()
}

fn drop_fonts(doc: &Document, res: &mut Dict, names: &[Vec<u8>]) {
    if names.is_empty() {
        return;
    }
    let Some(mut fonts) = res.get(b"Font").map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned()) else { return };
    for name in names {
        fonts.remove(name);
    }
    res.set(b"Font".to_vec(), Object::Dict(fonts));
}
