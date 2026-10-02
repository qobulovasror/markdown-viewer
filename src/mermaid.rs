//! Mermaid diagrams drawn as Unicode box art: flowcharts and sequence diagrams.
//!
//! Unsupported diagrams (or ones wider than the available width) return `None`
//! so the caller can show the source instead.

use std::collections::HashMap;

use regex::Regex;
use unicode_width::UnicodeWidthStr;

/// Renders `src` into lines no wider than `width`, if supported.
pub fn render(src: &str, width: usize) -> Option<Vec<String>> {
    let lines: Vec<&str> = src
        .lines()
        .map(|l| l.split("%%").next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .collect();
    let header = lines.first()?;
    let kind = header.split_whitespace().next()?;
    let out = match kind {
        "graph" | "flowchart" => {
            let dir = header.split_whitespace().nth(1).unwrap_or("TD");
            let chart = parse_flowchart(&lines[1..])?;
            let horizontal = matches!(dir, "LR" | "RL");
            let reversed = matches!(dir, "BT" | "RL");
            let drawn = chart.draw(horizontal, reversed);
            // A wide left-to-right chart may still fit top-down.
            if horizontal && max_width(&drawn) > width {
                chart.draw(false, reversed)
            } else {
                drawn
            }
        }
        "sequenceDiagram" => parse_sequence(&lines[1..])?.draw(),
        _ => return None,
    };
    (max_width(&out) <= width && !out.is_empty()).then_some(out)
}

fn max_width(lines: &[String]) -> usize {
    lines.iter().map(|l| l.width()).max().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Canvas with mergeable line segments.

/// Upper bound for either canvas side, before the width check.
const MAX_CANVAS: usize = 2000;

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stroke {
    Thin,
    Dotted,
    Thick,
}

struct Canvas {
    w: usize,
    h: usize,
    text: Vec<Option<char>>,
    bits: Vec<u8>,
    stroke: Vec<Stroke>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            text: vec![None; w * h],
            bits: vec![0; w * h],
            stroke: vec![Stroke::Thin; w * h],
        }
    }

    fn put(&mut self, x: usize, y: usize, c: char) {
        if x < self.w && y < self.h {
            self.text[y * self.w + x] = Some(c);
        }
    }

    fn put_str(&mut self, x: usize, y: usize, s: &str) {
        let mut cx = x;
        for c in s.chars() {
            self.put(cx, y, c);
            cx += unicode_width::UnicodeWidthChar::width(c)
                .unwrap_or(1)
                .max(1);
        }
    }

    fn is_free(&self, x: usize, y: usize, len: usize) -> bool {
        (x..x + len).all(|cx| {
            cx < self.w
                && y < self.h
                && self.text[y * self.w + cx].is_none()
                && self.bits[y * self.w + cx] == 0
        })
    }

    fn link(&mut self, x: usize, y: usize, b: u8, s: Stroke) {
        if x < self.w && y < self.h {
            self.bits[y * self.w + x] |= b;
            if s != Stroke::Thin {
                self.stroke[y * self.w + x] = s;
            }
        }
    }

    fn hline(&mut self, x1: usize, x2: usize, y: usize, s: Stroke) {
        let (a, b) = (x1.min(x2), x1.max(x2));
        for x in a..=b {
            let mut bits = 0;
            if x > a {
                bits |= LEFT;
            }
            if x < b {
                bits |= RIGHT;
            }
            self.link(x, y, bits, s);
        }
    }

    fn vline(&mut self, x: usize, y1: usize, y2: usize, s: Stroke) {
        let (a, b) = (y1.min(y2), y1.max(y2));
        for y in a..=b {
            let mut bits = 0;
            if y > a {
                bits |= UP;
            }
            if y < b {
                bits |= DOWN;
            }
            self.link(x, y, bits, s);
        }
    }

    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, rounded: bool) {
        let (tl, tr, bl, br) = if rounded {
            ('╭', '╮', '╰', '╯')
        } else {
            ('┌', '┐', '└', '┘')
        };
        for cx in x + 1..x + w - 1 {
            self.put(cx, y, '─');
            self.put(cx, y + h - 1, '─');
        }
        for cy in y + 1..y + h - 1 {
            self.put(x, cy, '│');
            self.put(x + w - 1, cy, '│');
        }
        self.put(x, y, tl);
        self.put(x + w - 1, y, tr);
        self.put(x, y + h - 1, bl);
        self.put(x + w - 1, y + h - 1, br);
    }

    fn lines(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.h);
        for y in 0..self.h {
            let mut line = String::new();
            let mut skip = 0;
            for x in 0..self.w {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let i = y * self.w + x;
                let c = match self.text[i] {
                    Some(c) => c,
                    None => junction(self.bits[i], self.stroke[i]),
                };
                skip = unicode_width::UnicodeWidthChar::width(c)
                    .unwrap_or(1)
                    .saturating_sub(1);
                line.push(c);
            }
            out.push(line.trim_end().to_string());
        }
        while out.last().is_some_and(String::is_empty) {
            out.pop();
        }
        out
    }
}

fn junction(bits: u8, s: Stroke) -> char {
    let v = bits & (UP | DOWN) != 0;
    let h = bits & (LEFT | RIGHT) != 0;
    match (bits, s) {
        (0, _) => ' ',
        _ if v && !h => match s {
            Stroke::Thin => '│',
            Stroke::Dotted => '┆',
            Stroke::Thick => '┃',
        },
        _ if h && !v => match s {
            Stroke::Thin => '─',
            Stroke::Dotted => '┄',
            Stroke::Thick => '━',
        },
        _ => match bits {
            b if b == DOWN | RIGHT => '┌',
            b if b == DOWN | LEFT => '┐',
            b if b == UP | RIGHT => '└',
            b if b == UP | LEFT => '┘',
            b if b == UP | DOWN | RIGHT => '├',
            b if b == UP | DOWN | LEFT => '┤',
            b if b == DOWN | LEFT | RIGHT => '┬',
            b if b == UP | LEFT | RIGHT => '┴',
            _ => '┼',
        },
    }
}

// ---------------------------------------------------------------------------
// Flowcharts.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Box,
    Round,
    Decision,
}

struct Node {
    label: String,
    shape: Shape,
}

struct Edge {
    from: usize,
    to: usize,
    label: String,
    stroke: Stroke,
    arrow: bool,
}

struct Flowchart {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

fn parse_flowchart(lines: &[&str]) -> Option<Flowchart> {
    let mut chart = Flowchart {
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let mut ids: HashMap<String, usize> = HashMap::new();
    let edge_re = Regex::new(
        r"^\s*(?:(--|==|-\.)\s+([^|>]+?)\s+(-->|==>|\.->|---|===|\.-)|(<?(?:-{2,}|={2,}|-\.+-)[>ox]?)\s*(?:\|([^|]*)\|)?)",
    )
    .ok()?;
    const SKIP: &[&str] = &[
        "subgraph",
        "end",
        "classDef",
        "class",
        "style",
        "linkStyle",
        "click",
        "direction",
    ];
    for stmt in lines.iter().flat_map(|l| l.split(';')) {
        let stmt = stmt.trim();
        let first = stmt.split_whitespace().next().unwrap_or("");
        if stmt.is_empty() || SKIP.contains(&first) {
            continue;
        }
        let mut rest = stmt;
        let mut prev: Vec<usize> = Vec::new();
        let mut pending: Option<(String, Stroke, bool)> = None;
        loop {
            let (group, after) = parse_node_group(rest, &mut chart, &mut ids)?;
            if let Some((label, stroke, arrow)) = pending.take() {
                for &a in &prev {
                    for &b in &group {
                        chart.edges.push(Edge {
                            from: a,
                            to: b,
                            label: label.clone(),
                            stroke,
                            arrow,
                        });
                    }
                }
            }
            prev = group;
            rest = after;
            if rest.trim().is_empty() {
                break;
            }
            let caps = edge_re.captures(rest)?;
            let whole = caps.get(0)?;
            let (op, label) = match caps.get(3) {
                Some(close) => (
                    format!("{}{}", &caps[1], close.as_str()),
                    caps[2].to_string(),
                ),
                None => (
                    caps.get(4)?.as_str().to_string(),
                    caps.get(5)
                        .map_or(String::new(), |m| m.as_str().to_string()),
                ),
            };
            let stroke = if op.contains('=') {
                Stroke::Thick
            } else if op.contains('.') {
                Stroke::Dotted
            } else {
                Stroke::Thin
            };
            let arrow = op.ends_with('>') || op.ends_with('o') || op.ends_with('x');
            pending = Some((label.trim().trim_matches('"').to_string(), stroke, arrow));
            rest = &rest[whole.end()..];
        }
    }
    // Very large graphs are unreadable as text anyway.
    (!chart.nodes.is_empty() && chart.nodes.len() <= 100 && chart.edges.len() <= 300)
        .then_some(chart)
}

/// Parses `A`, `A[label]`, `A & B` ...; returns node indices and the rest.
fn parse_node_group<'a>(
    s: &'a str,
    chart: &mut Flowchart,
    ids: &mut HashMap<String, usize>,
) -> Option<(Vec<usize>, &'a str)> {
    let mut out = Vec::new();
    let mut rest = s.trim_start();
    loop {
        let (idx, after) = parse_node(rest, chart, ids)?;
        out.push(idx);
        rest = after.trim_start();
        match rest.strip_prefix('&') {
            Some(r) => rest = r.trim_start(),
            None => return Some((out, rest)),
        }
    }
}

fn parse_node<'a>(
    s: &'a str,
    chart: &mut Flowchart,
    ids: &mut HashMap<String, usize>,
) -> Option<(usize, &'a str)> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut end = 0;
    for (k, &(i, c)) in chars.iter().enumerate() {
        let next = chars.get(k + 1).map(|&(_, n)| n);
        let ok = c.is_alphanumeric()
            || c == '_'
            || (c == '-' && next.is_some_and(|n| n.is_alphanumeric()));
        if !ok {
            break;
        }
        end = i + c.len_utf8();
    }
    if end == 0 {
        return None;
    }
    let id = &s[..end];
    let mut rest = &s[end..];
    let mut shape_label = None;
    const SHAPES: &[(&str, &str, Shape)] = &[
        ("((", "))", Shape::Round),
        ("([", "])", Shape::Round),
        ("[[", "]]", Shape::Box),
        ("[(", ")]", Shape::Box),
        ("{{", "}}", Shape::Decision),
        ("[/", "/]", Shape::Box),
        ("[\\", "\\]", Shape::Box),
        ("[", "]", Shape::Box),
        ("(", ")", Shape::Round),
        ("{", "}", Shape::Decision),
        (">", "]", Shape::Box),
    ];
    for &(open, close, shape) in SHAPES {
        if let Some(body) = rest.strip_prefix(open) {
            let stop = body.find(close)?;
            let label = body[..stop]
                .trim()
                .trim_matches('"')
                .replace("<br>", " ")
                .replace("<br/>", " ");
            shape_label = Some((label, shape));
            rest = &body[stop + close.len()..];
            break;
        }
    }
    let idx = match ids.get(id) {
        Some(&i) => i,
        None => {
            chart.nodes.push(Node {
                label: id.to_string(),
                shape: Shape::Box,
            });
            ids.insert(id.to_string(), chart.nodes.len() - 1);
            chart.nodes.len() - 1
        }
    };
    if let Some((label, shape)) = shape_label {
        chart.nodes[idx].label = label;
        chart.nodes[idx].shape = shape;
    }
    Some((idx, rest))
}

/// A slot in a layer: a real node or a pass-through point of a long edge.
#[derive(Clone, Copy)]
enum Slot {
    Node(usize),
    Dummy,
}

struct Placed {
    slot: Slot,
    /// Position along the layer axis and size across both axes.
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Flowchart {
    /// Longest-path layering; back edges of cycles are ignored.
    fn layers(&self) -> Vec<usize> {
        let n = self.nodes.len();
        let mut layer = vec![0usize; n];
        let mut state = vec![0u8; n];
        let mut order = Vec::with_capacity(n);
        fn visit(v: usize, chart: &Flowchart, state: &mut [u8], order: &mut Vec<usize>) {
            state[v] = 1;
            for e in chart.edges.iter().filter(|e| e.from == v) {
                if state[e.to] == 0 {
                    visit(e.to, chart, state, order);
                }
            }
            state[v] = 2;
            order.push(v);
        }
        for v in 0..n {
            if state[v] == 0 {
                visit(v, self, &mut state, &mut order);
            }
        }
        order.reverse();
        let pos: Vec<usize> = {
            let mut p = vec![0; n];
            for (i, &v) in order.iter().enumerate() {
                p[v] = i;
            }
            p
        };
        for &v in &order {
            for e in self
                .edges
                .iter()
                .filter(|e| e.from == v && pos[e.to] > pos[v])
            {
                layer[e.to] = layer[e.to].max(layer[v] + 1);
            }
        }
        layer
    }

    fn draw(&self, horizontal: bool, reversed: bool) -> Vec<String> {
        let layer = self.layers();
        let depth = layer.iter().max().copied().unwrap_or(0) + 1;

        // Slots per layer, with dummies for edges spanning several layers.
        let mut slots: Vec<Vec<Slot>> = vec![Vec::new(); depth];
        for (v, &l) in layer.iter().enumerate() {
            slots[l].push(Slot::Node(v));
        }
        // Each forward edge becomes a chain of (layer, slot index) points;
        // edges going back up (cycles) are listed under the diagram instead.
        let mut chains: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
        let mut back = Vec::new();
        for (ei, e) in self.edges.iter().enumerate() {
            let (la, lb) = (layer[e.from], layer[e.to]);
            if lb <= la {
                back.push(e);
                continue;
            }
            let slot_of = |slots: &Vec<Vec<Slot>>, l: usize, v: usize| {
                slots[l]
                    .iter()
                    .position(|s| matches!(s, Slot::Node(n) if *n == v))
                    .expect("node is in its layer")
            };
            let mut chain = vec![(la, slot_of(&slots, la, e.from))];
            for (l, layer_slots) in slots.iter_mut().enumerate().take(lb).skip(la + 1) {
                layer_slots.push(Slot::Dummy);
                chain.push((l, layer_slots.len() - 1));
            }
            chain.push((lb, slot_of(&slots, lb, e.to)));
            chains.push((ei, chain));
        }

        // Size every slot.
        let size = |s: &Slot| -> (usize, usize) {
            match s {
                Slot::Node(v) => {
                    let lw = self.nodes[*v].label.width();
                    let pad = if self.nodes[*v].shape == Shape::Decision {
                        4
                    } else {
                        2
                    };
                    (lw + pad + 2, 3)
                }
                Slot::Dummy => (1, 1),
            }
        };
        // Gap between layers, wide enough for edge labels in LR mode.
        let label_w = self
            .edges
            .iter()
            .map(|e| e.label.width())
            .max()
            .unwrap_or(0);
        let gap = if horizontal { (label_w + 6).max(6) } else { 3 };

        let mut placed: Vec<Vec<Placed>> = Vec::new();
        // Extent of each layer across the flow direction.
        let extents: Vec<usize> = slots
            .iter()
            .map(|ls| {
                let sizes = ls.iter().map(size);
                if horizontal {
                    sizes.map(|(_, h)| h + 1).sum::<usize>().saturating_sub(1)
                } else {
                    sizes.map(|(w, _)| w + 2).sum::<usize>().saturating_sub(2)
                }
            })
            .collect();
        let span = extents.iter().max().copied().unwrap_or(0);
        let mut along = 0;
        let order: Vec<usize> = if reversed {
            (0..depth).rev().collect()
        } else {
            (0..depth).collect()
        };
        let mut layer_pos = vec![0; depth];
        let mut layer_len = vec![0; depth];
        for &l in &order {
            let thickness = slots[l]
                .iter()
                .map(|s| if horizontal { size(s).0 } else { size(s).1 })
                .max()
                .unwrap_or(1);
            layer_pos[l] = along;
            layer_len[l] = thickness;
            along += thickness + gap;
        }
        for (l, ls) in slots.iter().enumerate() {
            let mut across = (span - extents[l]) / 2;
            let mut row = Vec::new();
            for s in ls {
                let (w, h) = size(s);
                let (x, y) = if horizontal {
                    // Center narrower slots within the layer column.
                    (layer_pos[l] + (layer_len[l] - w) / 2, across)
                } else {
                    (across, layer_pos[l] + (layer_len[l] - h) / 2)
                };
                row.push(Placed {
                    slot: *s,
                    x,
                    y,
                    w,
                    h,
                });
                across += if horizontal { h + 1 } else { w + 2 };
            }
            placed.push(row);
        }

        let (cw, ch) = if horizontal {
            (along.saturating_sub(gap), span)
        } else {
            (span, along.saturating_sub(gap))
        };
        // Far too big to fit any terminal: let the caller show the source.
        if cw > MAX_CANVAS || ch > MAX_CANVAS {
            return Vec::new();
        }
        let mut c = Canvas::new(cw + 1, ch + 1);

        for row in &placed {
            for p in row {
                match p.slot {
                    Slot::Node(v) => {
                        let node = &self.nodes[v];
                        c.rect(p.x, p.y, p.w, p.h, node.shape == Shape::Round);
                        let label = if node.shape == Shape::Decision {
                            format!("◇ {}", node.label)
                        } else {
                            node.label.clone()
                        };
                        let lx = p.x + (p.w - label.width()) / 2;
                        c.put_str(lx, p.y + 1, &label);
                    }
                    Slot::Dummy => {}
                }
            }
        }

        for (ei, chain) in &chains {
            let e = &self.edges[*ei];
            for (k, pair) in chain.windows(2).enumerate() {
                let a = &placed[pair[0].0][pair[0].1];
                let b = &placed[pair[1].0][pair[1].1];
                let last = k + 2 == chain.len();
                let label = if last { e.label.as_str() } else { "" };
                route(&mut c, a, b, horizontal, e.stroke, e.arrow && last, label);
            }
        }
        let mut out = c.lines();
        for e in back {
            let label = if e.label.is_empty() {
                String::new()
            } else {
                format!(" ({})", e.label)
            };
            out.push(format!(
                "↺ {} → {}{label}",
                self.nodes[e.from].label, self.nodes[e.to].label
            ));
        }
        out
    }
}

/// Connects two placed slots across one layer gap.
fn route(
    c: &mut Canvas,
    a: &Placed,
    b: &Placed,
    horizontal: bool,
    s: Stroke,
    arrow: bool,
    label: &str,
) {
    let is_dummy = |p: &Placed| matches!(p.slot, Slot::Dummy);
    if horizontal {
        let forward = b.x > a.x;
        let by = b.y + b.h / 2;
        let mut ay = a.y + a.h / 2;
        if ay.abs_diff(by) == 1 && by > a.y && by + 1 < a.y + a.h {
            ay = by;
        }
        let (ax, bx) = if forward {
            (a.x + a.w, b.x.saturating_sub(1))
        } else {
            (a.x.saturating_sub(1), b.x + b.w)
        };
        if is_dummy(a) {
            c.hline(a.x, ax, ay, s);
        } else {
            c.put(
                if forward { a.x + a.w - 1 } else { a.x },
                ay,
                if forward { '├' } else { '┤' },
            );
        }
        let mid = (ax + bx) / 2;
        c.hline(ax, mid, ay, s);
        c.vline(mid, ay, by, s);
        c.hline(mid, bx, by, s);
        if is_dummy(b) {
            c.hline(bx, b.x, by, s);
        } else if arrow {
            c.put(bx, by, if forward { '▶' } else { '◀' });
        }
        if !label.is_empty() {
            // On the last horizontal run, just before the target.
            let lw = label.width();
            let (from, to) = (mid.min(bx), mid.max(bx));
            if to - from > lw + 1 {
                c.put_str(from + 1 + (to - from - 1 - lw) / 2, by, label);
            } else if by > 0 && c.is_free(from, by - 1, lw) {
                c.put_str(from, by - 1, label);
            }
        }
    } else {
        let down = b.y > a.y;
        let bx = b.x + b.w / 2;
        let mut ax = a.x + a.w / 2;
        // Avoid a one-column jog between boxes of different widths.
        if ax.abs_diff(bx) == 1 && bx > a.x && bx + 1 < a.x + a.w {
            ax = bx;
        }
        let (ay, by) = if down {
            (a.y + a.h, b.y.saturating_sub(1))
        } else {
            (a.y.saturating_sub(1), b.y + b.h)
        };
        if is_dummy(a) {
            c.vline(ax, a.y, ay, s);
        } else {
            c.put(
                ax,
                if down { a.y + a.h - 1 } else { a.y },
                if down { '┬' } else { '┴' },
            );
        }
        let mid = if down { ay + 1 } else { ay.saturating_sub(1) };
        c.vline(ax, ay, mid, s);
        c.hline(ax, bx, mid, s);
        c.vline(bx, mid, by, s);
        if is_dummy(b) {
            c.vline(bx, by, b.y, s);
        } else if arrow {
            c.put(bx, by, if down { '▼' } else { '▲' });
        }
        if !label.is_empty() {
            // Beside the arrow head, else on the other side.
            let lw = label.width();
            if c.is_free(bx + 2, by, lw) {
                c.put_str(bx + 2, by, label);
            } else if bx >= lw + 2 && c.is_free(bx - lw - 1, by, lw) {
                c.put_str(bx - lw - 1, by, label);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sequence diagrams.

enum Step {
    Message {
        from: usize,
        to: usize,
        text: String,
        dashed: bool,
        arrow: bool,
    },
    Note {
        at: usize,
        text: String,
    },
}

struct Sequence {
    actors: Vec<String>,
    steps: Vec<Step>,
}

fn parse_sequence(lines: &[&str]) -> Option<Sequence> {
    let mut seq = Sequence {
        actors: Vec::new(),
        steps: Vec::new(),
    };
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut actor = |seq: &mut Sequence, id: &str, label: Option<&str>| -> usize {
        let id = id.trim();
        let idx = *ids.entry(id.to_string()).or_insert_with(|| {
            seq.actors.push(id.to_string());
            seq.actors.len() - 1
        });
        if let Some(l) = label {
            seq.actors[idx] = l.trim().to_string();
        }
        idx
    };
    let msg_re =
        Regex::new(r"^(.+?)\s*(--?>>|--?>|--?x|--?\))\s*[+-]?\s*(.+?)\s*:\s*(.*)$").ok()?;
    for line in lines {
        let first = line.split_whitespace().next().unwrap_or("");
        if first == "participant" || first == "actor" {
            let rest = line[first.len()..].trim();
            match rest.split_once(" as ") {
                Some((id, label)) => actor(&mut seq, id, Some(label)),
                None => actor(&mut seq, rest, None),
            };
        } else if first.eq_ignore_ascii_case("note") {
            let Some((place, text)) = line.split_once(':') else {
                continue;
            };
            let target = place
                .split_whitespace()
                .last()
                .unwrap_or("")
                .split(',')
                .next()
                .unwrap_or("");
            if target.is_empty() {
                continue;
            }
            let at = actor(&mut seq, target, None);
            seq.steps.push(Step::Note {
                at,
                text: text.trim().to_string(),
            });
        } else if let Some(c) = msg_re.captures(line) {
            let from = actor(&mut seq, &c[1], None);
            let to = actor(&mut seq, &c[3], None);
            let op = &c[2];
            seq.steps.push(Step::Message {
                from,
                to,
                text: c[4].trim().to_string(),
                dashed: op.starts_with("--"),
                arrow: op.contains('>'),
            });
        }
        // loop/alt/opt/end/activate/autonumber ... are ignored.
    }
    (!seq.actors.is_empty()).then_some(seq)
}

impl Sequence {
    fn draw(&self) -> Vec<String> {
        let n = self.actors.len();
        let box_w: Vec<usize> = self.actors.iter().map(|a| a.width() + 4).collect();
        // Distance between neighbouring lifelines.
        let mut gaps: Vec<usize> = (0..n.saturating_sub(1))
            .map(|i| box_w[i] / 2 + box_w[i + 1] / 2 + 3)
            .collect();
        for s in &self.steps {
            if let Step::Message { from, to, text, .. } = s {
                let (a, b) = (*from.min(to), *from.max(to));
                if a == b {
                    continue;
                }
                let need = text.width() + 4;
                let have: usize = gaps[a..b].iter().sum();
                if have < need {
                    gaps[b - 1] += need - have;
                }
            }
        }
        let mut xs = vec![box_w[0] / 2];
        for g in &gaps {
            xs.push(xs.last().copied().unwrap_or(0) + g);
        }
        let self_w = self
            .steps
            .iter()
            .map(|s| match s {
                Step::Message { from, to, text, .. } if from == to => text.width() + 6,
                Step::Note { text, .. } => text.width() + 6,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        let width = xs[n - 1] + (box_w[n - 1] / 2 + 1).max(self_w) + 1;
        if width > MAX_CANVAS {
            return Vec::new();
        }
        let rows: usize = 4 + self
            .steps
            .iter()
            .map(|s| match s {
                Step::Message { from, to, .. } if from == to => 3,
                Step::Message { .. } => 2,
                Step::Note { .. } => 3,
            })
            .sum::<usize>();
        let mut c = Canvas::new(width + 1, rows + 3);

        let draw_boxes = |c: &mut Canvas, y: usize| {
            for (i, a) in self.actors.iter().enumerate() {
                let x = xs[i] - box_w[i] / 2;
                c.rect(x, y, box_w[i], 3, false);
                c.put_str(x + 2, y + 1, a);
            }
        };
        draw_boxes(&mut c, 0);
        let mut y = 3;
        let top = y;
        for s in &self.steps {
            match s {
                Step::Message {
                    from,
                    to,
                    text,
                    dashed,
                    arrow,
                } => {
                    let stroke = if *dashed {
                        Stroke::Dotted
                    } else {
                        Stroke::Thin
                    };
                    let (fx, tx) = (xs[*from], xs[*to]);
                    if from == to {
                        c.put_str(fx + 2, y, text);
                        c.hline(fx, fx + 3, y + 1, stroke);
                        c.vline(fx + 3, y + 1, y + 2, stroke);
                        c.hline(fx, fx + 3, y + 2, stroke);
                        if *arrow {
                            c.put(fx + 1, y + 2, '◀');
                        }
                        y += 3;
                    } else {
                        let (l, r) = (fx.min(tx), fx.max(tx));
                        let lx = l + (r - l).saturating_sub(text.width()) / 2;
                        c.put_str(lx, y, text);
                        c.hline(l, r, y + 1, stroke);
                        if *arrow {
                            if tx > fx {
                                c.put(tx - 1, y + 1, '▶');
                            } else {
                                c.put(tx + 1, y + 1, '◀');
                            }
                        }
                        y += 2;
                    }
                }
                Step::Note { at, text } => {
                    let x = xs[*at] + 2;
                    c.rect(x, y, text.width() + 4, 3, true);
                    c.put_str(x + 2, y + 1, text);
                    y += 3;
                }
            }
        }
        // Lifelines underneath everything drawn so far.
        for &x in &xs {
            c.vline(x, top, y, Stroke::Thin);
        }
        for &x in &xs {
            c.put(x, top - 1, '┬');
        }
        draw_boxes(&mut c, y + 1);
        for &x in &xs {
            c.put(x, y + 1, '┴');
        }
        c.lines()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_down_flowchart() {
        let out = render(
            "graph TD\n  A[Start] --> B{Ok?}\n  B -->|yes| C(Done)\n  B -->|no| A",
            80,
        )
        .unwrap();
        let text = out.join("\n");
        assert!(text.contains("Start") && text.contains("◇ Ok?") && text.contains("Done"));
        assert!(text.contains('▼'));
        assert!(text.contains("yes"));
        // The cycle back to Start is listed, not drawn.
        assert!(
            text.contains("↺ ◇ Ok? → Start") || text.contains("↺ Ok? → Start"),
            "{text}"
        );
    }

    #[test]
    fn left_right_flowchart() {
        let out = render("flowchart LR\n  A --> B --> C", 80).unwrap();
        assert!(out.iter().any(|l| l.contains('▶')));
        // Three boxes side by side on the label row.
        assert!(
            out.iter()
                .any(|l| l.contains('A') && l.contains('B') && l.contains('C'))
        );
    }

    #[test]
    fn long_edges_pass_through_layers() {
        let out = render("graph TD\n A --> B\n B --> C\n A --> C", 80).unwrap();
        assert_eq!(out.iter().filter(|l| l.contains('▼')).count(), 2);
    }

    #[test]
    fn sequence_diagram() {
        let out = render(
            "sequenceDiagram\n  participant A as Alice\n  A->>Bob: Hello\n  Bob-->>A: Hi\n  Note right of Bob: thinks",
            80,
        );
        let out = out.unwrap_or_default().join("\n");
        assert!(out.contains("Hello") && out.contains("Hi") && out.contains("thinks"));
        assert_eq!(out.matches("Alice").count(), 2, "{out}");
        assert!(out.contains('▶') && out.contains('◀'));
    }

    #[test]
    fn random_statements_do_not_panic() {
        const PARTS: &[&str] = &[
            "A",
            "B",
            "C1",
            "x-y",
            "-->",
            "---",
            "-.->",
            "==>",
            "<-->",
            "-- t -->",
            "|lbl|",
            "[box]",
            "(r)",
            "{d}",
            "((c))",
            "&",
            ";",
            "\n",
            " ",
            "A-->A",
            "é",
            "日本",
            "->>",
            "-->>",
            ": msg",
            "participant P as Q",
            "Note right of A: n",
            "subgraph s",
            "end",
        ];
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as usize
        };
        for header in [
            "graph TD",
            "graph LR",
            "graph BT",
            "flowchart RL",
            "sequenceDiagram",
        ] {
            for _ in 0..400 {
                let body: String = (0..next() % 30)
                    .map(|_| PARTS[next() % PARTS.len()])
                    .collect();
                for width in [8, 40, 120] {
                    if let Some(lines) = render(&format!("{header}\n{body}"), width) {
                        assert!(lines.iter().all(|l| l.width() <= width));
                    }
                }
            }
        }
    }

    #[test]
    fn huge_labels_are_rejected_cheaply() {
        let long = "x".repeat(5000);
        assert!(render(&format!("graph LR\n A[{long}] --> B[{long}]"), 80).is_none());
        assert!(render(&format!("sequenceDiagram\n A->>B: {long}"), 80).is_none());
        assert!(render("graph BT\n A --> B\n C --> A", 80).is_some());
    }

    #[test]
    fn unsupported_or_too_wide() {
        assert!(render("pie title Pets\n \"Dogs\" : 3", 80).is_none());
        assert!(
            render(
                "graph LR\n A[a very long label] --> B[another long label]",
                10
            )
            .is_none()
        );
    }
}
