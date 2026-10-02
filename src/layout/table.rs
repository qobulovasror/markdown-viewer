//! Table layout: column sizing, cell wrapping and box drawing.

use ratatui::style::Style;

use super::wrap::{self, Seg, wrap};
use super::{Layouter, Line, Span, push_span};
use crate::parser::{Align, Inline};

impl Layouter<'_> {
    pub(super) fn table(
        &mut self,
        aligns: &[Align],
        header: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        width: usize,
    ) -> Vec<Line> {
        let t = self.theme;
        let ncols = rows
            .iter()
            .map(Vec::len)
            .chain([header.len()])
            .max()
            .unwrap_or(0);
        if ncols == 0 {
            return Vec::new();
        }

        let mut flat = |cells: &[Vec<Inline>], style: Style| -> Vec<Vec<Seg>> {
            (0..ncols)
                .map(|c| {
                    let mut segs = Vec::new();
                    if let Some(cell) = cells.get(c) {
                        self.flatten(cell, style, None, &mut segs);
                    }
                    segs
                })
                .collect()
        };
        let head = flat(header, t.table_header);
        let body: Vec<_> = rows.iter().map(|r| flat(r, t.text)).collect();

        let mut natural = vec![1usize; ncols];
        for row in std::iter::once(&head).chain(body.iter()) {
            for (c, segs) in row.iter().enumerate() {
                natural[c] = natural[c].max(wrap::natural_width(segs));
            }
        }
        let widths = fit_columns(&natural, width.saturating_sub(3 * ncols + 1));

        let b = t.table_border;
        let border = |l: &str, m: &str, r: &str| {
            let mut s = String::from(l);
            for (i, w) in widths.iter().enumerate() {
                s.push_str(&"─".repeat(w + 2));
                s.push_str(if i + 1 == ncols { r } else { m });
            }
            Line::new(vec![Span::new(s, b)])
        };

        let mut out = vec![border("╭", "┬", "╮")];
        if header.iter().any(|c| !c.is_empty()) {
            out.extend(row_lines(head, &widths, aligns, b));
            out.push(border("├", "┼", "┤"));
        }
        for row in body {
            out.extend(row_lines(row, &widths, aligns, b));
        }
        out.push(border("╰", "┴", "╯"));
        out
    }
}

fn row_lines(cells: Vec<Vec<Seg>>, widths: &[usize], aligns: &[Align], border: Style) -> Vec<Line> {
    let wrapped: Vec<Vec<Vec<Span>>> = cells
        .into_iter()
        .zip(widths)
        .map(|(segs, &w)| wrap(segs, w))
        .collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
    (0..height)
        .map(|r| {
            let mut spans = vec![Span::new("│", border)];
            for (c, cell) in wrapped.iter().enumerate() {
                let content = cell.get(r).cloned().unwrap_or_default();
                let free = widths[c].saturating_sub(wrap::width(&content));
                let (left, right) = match aligns.get(c) {
                    Some(Align::Right) => (free, 0),
                    Some(Align::Center) => (free / 2, free - free / 2),
                    _ => (0, free),
                };
                push_span(&mut spans, Span::new(" ".repeat(left + 1), Style::new()));
                for s in content {
                    push_span(&mut spans, s);
                }
                push_span(&mut spans, Span::new(" ".repeat(right + 1), Style::new()));
                spans.push(Span::new("│", border));
            }
            Line::new(spans)
        })
        .collect()
}

/// Shrinks column widths to fit `avail`, keeping narrow columns intact.
fn fit_columns(natural: &[usize], avail: usize) -> Vec<usize> {
    if natural.iter().sum::<usize>() <= avail {
        return natural.to_vec();
    }
    let mut order: Vec<usize> = (0..natural.len()).collect();
    order.sort_by_key(|&c| natural[c]);
    let mut widths = vec![0; natural.len()];
    let mut remaining = avail;
    for (k, &c) in order.iter().enumerate() {
        let share = remaining / (natural.len() - k);
        widths[c] = natural[c].min(share).max(1);
        remaining = remaining.saturating_sub(widths[c]);
    }
    widths
}

#[cfg(test)]
mod tests {
    use super::fit_columns;

    #[test]
    fn fits_when_room() {
        assert_eq!(fit_columns(&[3, 5], 20), [3, 5]);
    }

    #[test]
    fn shrinks_wide_columns_first() {
        assert_eq!(fit_columns(&[3, 50, 40], 30), [3, 14, 13]);
    }
}
