//! diw edit: Align and Distribute for added content, as a pure computation on boxes.
//!
//! Each unit is one item, or one group moved as a whole (its box is the union of its items'
//! boxes). Boxes are in display space: x to the right, y up, as `AddedText::rect` and
//! `AddedImage::rect` are. The result is the offset to move each unit by.

use super::layout::norm;

/// How Align and Distribute line up the units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignBoxes {
    Left,
    /// Horizontal centres on one vertical line.
    Center,
    Right,
    Top,
    /// Vertical centres on one horizontal line.
    Middle,
    Bottom,
    /// Equal gaps between neighbours, left to right; the outermost two stay put.
    DistributeH,
    /// Equal gaps between neighbours, top to bottom; the outermost two stay put.
    DistributeV,
}

impl AlignBoxes {
    pub const ALL: [AlignBoxes; 8] = [
        AlignBoxes::Left,
        AlignBoxes::Center,
        AlignBoxes::Right,
        AlignBoxes::Top,
        AlignBoxes::Middle,
        AlignBoxes::Bottom,
        AlignBoxes::DistributeH,
        AlignBoxes::DistributeV,
    ];

    pub fn is_distribute(self) -> bool {
        matches!(self, AlignBoxes::DistributeH | AlignBoxes::DistributeV)
    }

    /// How many units it takes: aligning one unit lines it up with the page instead, and
    /// distributing needs three (with two there is no gap to share).
    pub fn min_units(self) -> usize {
        if self.is_distribute() { 3 } else { 1 }
    }
}

/// The offset `[dx, dy]` (display space) that lines each of `units` up as `how` says. With one
/// unit, Align lines it up with `page` (the page's box) instead. Units with a box that isn't a
/// number stay where they are; fewer units than [`AlignBoxes::min_units`] leave every unit put.
pub fn align_offsets(units: &[[f64; 4]], how: AlignBoxes, page: [f64; 4]) -> Vec<[f64; 2]> {
    let mut out = vec![[0.0, 0.0]; units.len()];
    // Checked before `norm`: `f64::min` and `max` pass over a NaN, so a box with one would come out finite.
    let boxes: Vec<(usize, [f64; 4])> =
        units.iter().enumerate().filter(|(_, r)| r.iter().all(|v| v.is_finite())).map(|(i, r)| (i, norm(*r))).collect();
    if boxes.len() < how.min_units() {
        return out;
    }
    // What the units line up with: their common bounds, or the page for a single unit.
    let reference = if boxes.len() == 1 && !how.is_distribute() {
        norm(page)
    } else {
        boxes.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |a, (_, r)| {
            [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]
        })
    };
    if !reference.iter().all(|v| v.is_finite()) {
        return out;
    }
    let [left, bottom, right, top] = reference;
    let set = |out: &mut Vec<[f64; 2]>, i: usize, d: [f64; 2]| {
        if let Some(slot) = out.get_mut(i) {
            *slot = d;
        }
    };
    match how {
        AlignBoxes::Left => boxes.iter().for_each(|(i, r)| set(&mut out, *i, [left - r[0], 0.0])),
        AlignBoxes::Right => boxes.iter().for_each(|(i, r)| set(&mut out, *i, [right - r[2], 0.0])),
        AlignBoxes::Center => boxes.iter().for_each(|(i, r)| set(&mut out, *i, [(left + right) / 2.0 - (r[0] + r[2]) / 2.0, 0.0])),
        AlignBoxes::Top => boxes.iter().for_each(|(i, r)| set(&mut out, *i, [0.0, top - r[3]])),
        AlignBoxes::Bottom => boxes.iter().for_each(|(i, r)| set(&mut out, *i, [0.0, bottom - r[1]])),
        AlignBoxes::Middle => boxes.iter().for_each(|(i, r)| set(&mut out, *i, [0.0, (bottom + top) / 2.0 - (r[1] + r[3]) / 2.0])),
        AlignBoxes::DistributeH | AlignBoxes::DistributeV => {
            let horizontal = how == AlignBoxes::DistributeH;
            // Along the axis: the start and end of each box. Vertically, units go top to bottom.
            let span = |r: &[f64; 4]| if horizontal { (r[0], r[2]) } else { (-r[3], -r[1]) };
            let mut sorted = boxes.clone();
            sorted.sort_by(|a, b| {
                let (sa, ea) = span(&a.1);
                let (sb, eb) = span(&b.1);
                (sa + ea).total_cmp(&(sb + eb))
            });
            let (Some(first), Some(last)) = (sorted.first(), sorted.last()) else { return out };
            let start = span(&first.1).0;
            let end = span(&last.1).1;
            let sizes: f64 = sorted.iter().map(|(_, r)| span(r).1 - span(r).0).sum();
            let gaps = (sorted.len() - 1) as f64;
            let gap = (end - start - sizes) / gaps;
            if !gap.is_finite() {
                return out;
            }
            let mut at = start;
            for (i, r) in &sorted {
                let (s, e) = span(r);
                let d = at - s;
                set(&mut out, *i, if horizontal { [d, 0.0] } else { [0.0, -d] });
                at += (e - s) + gap;
            }
        }
    }
    out
}

/// `rect` moved by `d`.
pub fn offset_rect(rect: [f64; 4], d: [f64; 2]) -> [f64; 4] {
    [rect[0] + d[0], rect[1] + d[1], rect[2] + d[0], rect[3] + d[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: [f64; 4] = [0.0, 0.0, 600.0, 800.0];

    fn moved(units: &[[f64; 4]], how: AlignBoxes) -> Vec<[f64; 4]> {
        units.iter().zip(align_offsets(units, how, PAGE)).map(|(r, d)| offset_rect(*r, d)).collect()
    }

    #[test]
    fn aligning_lines_edges_and_centres_up_with_the_selection() {
        let units = [[10.0, 100.0, 60.0, 120.0], [30.0, 200.0, 130.0, 260.0]];
        assert_eq!(moved(&units, AlignBoxes::Left), [[10.0, 100.0, 60.0, 120.0], [10.0, 200.0, 110.0, 260.0]]);
        assert_eq!(moved(&units, AlignBoxes::Right), [[80.0, 100.0, 130.0, 120.0], [30.0, 200.0, 130.0, 260.0]]);
        assert_eq!(moved(&units, AlignBoxes::Center), [[45.0, 100.0, 95.0, 120.0], [20.0, 200.0, 120.0, 260.0]]);
        assert_eq!(moved(&units, AlignBoxes::Top), [[10.0, 240.0, 60.0, 260.0], [30.0, 200.0, 130.0, 260.0]]);
        assert_eq!(moved(&units, AlignBoxes::Bottom), [[10.0, 100.0, 60.0, 120.0], [30.0, 100.0, 130.0, 160.0]]);
        assert_eq!(moved(&units, AlignBoxes::Middle), [[10.0, 170.0, 60.0, 190.0], [30.0, 150.0, 130.0, 210.0]]);
    }

    #[test]
    fn one_unit_lines_up_with_the_page() {
        let unit = [[10.0, 100.0, 60.0, 120.0]];
        assert_eq!(moved(&unit, AlignBoxes::Center), [[275.0, 100.0, 325.0, 120.0]]);
        assert_eq!(moved(&unit, AlignBoxes::Top), [[10.0, 780.0, 60.0, 800.0]]);
        assert_eq!(moved(&unit, AlignBoxes::DistributeH), unit, "distributing one unit leaves it put");
    }

    #[test]
    fn distributing_shares_the_space_between_the_outermost_units() {
        // Widths 10, 20, 10 between x 0 and 100: the gaps become 30 each.
        let units = [[0.0, 0.0, 10.0, 5.0], [60.0, 0.0, 80.0, 5.0], [90.0, 0.0, 100.0, 5.0]];
        assert_eq!(moved(&units, AlignBoxes::DistributeH), [[0.0, 0.0, 10.0, 5.0], [40.0, 0.0, 60.0, 5.0], [90.0, 0.0, 100.0, 5.0]]);
        // Top to bottom: heights 10 each between y 100 (top) and 0; the middle one is centred.
        let units = [[0.0, 90.0, 5.0, 100.0], [0.0, 70.0, 5.0, 80.0], [0.0, 0.0, 5.0, 10.0]];
        assert_eq!(moved(&units, AlignBoxes::DistributeV), [[0.0, 90.0, 5.0, 100.0], [0.0, 45.0, 5.0, 55.0], [0.0, 0.0, 5.0, 10.0]]);
        assert_eq!(moved(&units[..2], AlignBoxes::DistributeV), units[..2], "two units have no gap to share");
    }

    #[test]
    fn boxes_that_are_not_numbers_stay_put() {
        let units = [[f64::NAN, 0.0, 10.0, 5.0], [20.0, 0.0, 30.0, 5.0], [50.0, 0.0, 60.0, 5.0]];
        let d = align_offsets(&units, AlignBoxes::Left, PAGE);
        assert_eq!(d[0], [0.0, 0.0]);
        assert_eq!(d[2], [-30.0, 0.0], "the others line up with each other");
        assert_eq!(align_offsets(&[], AlignBoxes::Left, PAGE), Vec::<[f64; 2]>::new());
    }
}
