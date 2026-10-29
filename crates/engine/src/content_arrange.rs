//! diw edit: groups, Align and Distribute for added content, shared by the interface and the
//! automation tools so both arrange the same way.

use crate::{AddedContent, AlignBoxes, Document, Edit};

/// The batch name for Align and Distribute ([`crate::BATCH_LABELS`]).
pub const ALIGN_LABEL: &str = "Align";
pub const DISTRIBUTE_LABEL: &str = "Distribute";

impl Document {
    /// diw edit: added items `indices` of `page` with the rest of the groups they belong to, in
    /// drawing order, each once. Indexes past the page's items are dropped.
    pub fn content_with_groups(&self, page: usize, indices: &[usize]) -> Vec<usize> {
        let items: Vec<_> = self.added().iter().filter(|a| a.page == page).collect();
        let groups: Vec<u32> = indices.iter().filter_map(|i| items.get(*i)).filter_map(|a| a.group).collect();
        (0..items.len()).filter(|i| indices.contains(i) || items.get(*i).and_then(|a| a.group).is_some_and(|g| groups.contains(&g))).collect()
    }

    /// diw edit: the group added item `index` of `page` belongs to, if any.
    pub fn content_group(&self, page: usize, index: usize) -> Option<u32> {
        self.added().iter().filter(|a| a.page == page).nth(index).and_then(|a| a.group)
    }

    /// diw edit: the edit that aligns or distributes added items `indices` of `page` (with the
    /// rest of their groups), as one undo step. Each group moves as one unit, by its common box;
    /// a single unit lines up with the page. Refused when nothing would move.
    pub fn align_content(&self, page: usize, indices: &[usize], how: AlignBoxes) -> Result<Edit, String> {
        let info = self.info.pages.get(page).ok_or("the page no longer exists")?;
        let chosen = self.content_with_groups(page, indices);
        let items: Vec<_> = self.added().iter().filter(|a| a.page == page).collect();
        // Units: each group once (its members), and each ungrouped item on its own.
        let mut units: Vec<Vec<usize>> = Vec::new();
        let mut seen_groups: Vec<u32> = Vec::new();
        for &i in &chosen {
            match items.get(i).and_then(|a| a.group) {
                Some(g) if seen_groups.contains(&g) => {
                    if let Some(unit) = units.iter_mut().find(|u| u.first().and_then(|f| items.get(*f)).and_then(|a| a.group) == Some(g)) {
                        unit.push(i);
                    }
                }
                Some(g) => {
                    seen_groups.push(g);
                    units.push(vec![i]);
                }
                None => units.push(vec![i]),
            }
        }
        if units.len() < how.min_units() {
            return Err(if how.is_distribute() {
                "select three or more items (or groups) to distribute".into()
            } else {
                "select added content first".into()
            });
        }
        let boxes: Vec<[f64; 4]> = units
            .iter()
            .map(|u| {
                u.iter().filter_map(|i| items.get(*i)).map(|a| a.content.rect()).fold(
                    [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
                    |b, r| {
                        let r = [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])];
                        [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[2]), b[3].max(r[3])]
                    },
                )
            })
            .collect();
        let page_box = [0.0, 0.0, f64::from(info.width), f64::from(info.height)];
        let offsets = crate::added_align_offsets(&boxes, how, page_box);
        let mut edits = Vec::new();
        for (unit, d) in units.iter().zip(offsets) {
            if d[0].abs() < 1e-6 && d[1].abs() < 1e-6 {
                continue;
            }
            for &i in unit {
                let Some(a) = items.get(i) else { continue };
                let content: AddedContent = a.content.with_rect(crate::added_offset_rect(a.content.rect(), d));
                edits.push(Edit::UpdateContent { page, index: i, content });
            }
        }
        if edits.is_empty() {
            return Err("the selection is already lined up".into());
        }
        let label = if how.is_distribute() { DISTRIBUTE_LABEL } else { ALIGN_LABEL };
        Ok(Edit::Batch { label: label.into(), edits })
    }
}
