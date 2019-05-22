// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::mem;

// external
use svgdom;

mod fk {
    pub use font_kit::handle::Handle;
    pub use font_kit::hinting::HintingOptions as Hinting;
    pub use font_kit::source::SystemSource;
}

// self
use crate::tree;
use crate::tree::prelude::*;
use crate::utils;
use super::prelude::*;

mod convert;
use self::convert::*;

mod shaper;
use self::shaper::OutlinedCluster;


// TODO: visibility on text and tspan
// TODO: group when Options::keep_named_groups is set


/// A text decoration span.
///
/// Basically a horizontal line, that will be used for underline, overline and line-through.
/// It doesn't have a height, since it depends on the font metrics.
#[derive(Clone, Copy)]
struct DecorationSpan {
    width: f64,
    transform: tree::Transform,
}


pub fn convert(
    node: &svgdom::Node,
    state: &State,
    parent: &mut tree::Node,
    tree: &mut tree::Tree,
) {
    let pos_list = resolve_positions_list(node, state);
    let rotate_list = resolve_rotate_list(node);
    let writing_mode = convert_writing_mode(node);
    let mut text_ts = node.attributes().get_transform(AId::Transform);

    let mut chunks = collect_text_chunks(node, &pos_list, state, tree);
    let mut char_offset = 0;
    let mut x = 0.0;
    let mut y = 0.0;
    let mut new_paths = Vec::new();
    for chunk in &mut chunks {
        x = chunk.x.unwrap_or(x);
        y = chunk.y.unwrap_or(y);

        let mut clusters = shaper::outline_chunk(&chunk, state);
        if clusters.is_empty() {
            continue;
        }

        shaper::apply_writing_mode(&chunk, writing_mode, &mut clusters);
        shaper::apply_letter_spacing(&chunk, &mut clusters);
        shaper::apply_word_spacing(&chunk, &mut clusters);
        let width = shaper::resolve_clusters_positions(
            &chunk.text, char_offset, &pos_list, &rotate_list, &mut clusters
        );

        match writing_mode {
            WritingMode::LeftToRight => {
                x -= process_anchor(chunk.anchor, width);
            }
            WritingMode::TopToBottom => {
                y -= process_anchor(chunk.anchor, width);
                text_ts.rotate_at(90.0, x, y);
            }
        }


        for span in &mut chunk.spans {
            let decoration_spans = collect_decoration_spans(span, &clusters);

            let mut span_ts = text_ts.clone();
            span_ts.translate(x, y - span.baseline_shift);

            if let Some(decoration) = span.decoration.underline.take() {
                new_paths.push(convert_decoration(
                    -span.font.underline_position(span.font_size),
                    &span, decoration, &decoration_spans, span_ts,
                ));
            }

            if let Some(decoration) = span.decoration.overline.take() {
                new_paths.push(convert_decoration(
                    -span.font.ascent(span.font_size),
                    &span, decoration, &decoration_spans, span_ts,
                ));
            }

            if let Some(path) = convert_span(span, &mut clusters, &span_ts, parent, false) {
                new_paths.push(path);
            }

            if let Some(decoration) = span.decoration.line_through.take() {
                new_paths.push(convert_decoration(
                    -span.font.x_height(span.font_size) / 2.0,
                    &span, decoration, &decoration_spans, span_ts,
                ));
            }
        }

        char_offset += chunk.text.chars().count();
        x += width;
    }

    let mut bbox = Rect::new_bbox();
    for path in &new_paths {
        if let Some(r) = utils::path_bbox(&path.segments, None, None) {
            bbox = bbox.expand(r);
        }
    }

    for mut path in new_paths {
        fix_obj_bounding_box(&mut path, bbox, tree);
        parent.append_kind(tree::NodeKind::Path(path));
    }
}

fn convert_span(
    span: &mut TextSpan,
    clusters: &mut [OutlinedCluster],
    text_ts: &tree::Transform,
    parent: &mut tree::Node,
    dump_clusters: bool,
) -> Option<tree::Path> {
    let mut segments = Vec::new();

    for cluster in clusters {
        if span.contains(cluster.byte_idx) {
            if dump_clusters {
                let mut ts = *text_ts;
                ts.append(&cluster.transform);
                dump_cluster(cluster, ts, parent);
            }

            let mut path = mem::replace(&mut cluster.path, Vec::new());
            transform_path(&mut path, &cluster.transform);

            segments.extend_from_slice(&path);
        }
    }

    if segments.is_empty() {
        return None;
    }

    let mut fill = span.fill.take();
    if let Some(ref mut fill) = fill {
        // fill-rule on text must always be `nonzero`,
        // otherwise overlapped characters will be clipped.
        fill.rule = tree::FillRule::NonZero;
    }

    let path = tree::Path {
        id: String::new(),
        transform: *text_ts,
        visibility: span.visibility,
        fill,
        stroke: span.stroke.take(),
        rendering_mode: tree::ShapeRendering::default(), // TODO: this
        segments,
    };

    Some(path)
}

/// Applies the transform to the path segments.
fn transform_path(segments: &mut [tree::PathSegment], ts: &tree::Transform) {
    for seg in segments {
        match seg {
            tree::PathSegment::MoveTo { x, y } => {
                ts.apply_to(x, y);
            }
            tree::PathSegment::LineTo { x, y } => {
                ts.apply_to(x, y);
            }
            tree::PathSegment::CurveTo { x1, y1, x2, y2, x,  y } => {
                ts.apply_to(x1, y1);
                ts.apply_to(x2, y2);
                ts.apply_to(x, y);
            }
            tree::PathSegment::ClosePath => {}
        }
    }
}

// Only for debug purposes.
fn dump_cluster(
    cluster: &OutlinedCluster,
    text_ts: tree::Transform,
    parent: &mut tree::Node,
) {
    fn new_stroke(color: tree::Color) -> Option<tree::Stroke> {
        Some(tree::Stroke {
            paint: tree::Paint::Color(color),
            width: tree::StrokeWidth::new(0.2),
            .. tree::Stroke::default()
        })
    }

    let mut base_path = tree::Path {
        transform: text_ts,
        .. tree::Path::default()
    };

    // Cluster bbox.
    let r = Rect::new(0.0, -cluster.ascent,
                      cluster.advance, cluster.height()).unwrap();
    base_path.stroke = new_stroke(tree::Color::blue());
    base_path.segments = utils::rect_to_path(r);
    parent.append_kind(tree::NodeKind::Path(base_path.clone()));

    // Baseline.
    base_path.stroke = new_stroke(tree::Color::red());
    base_path.segments = vec![
        tree::PathSegment::MoveTo { x: 0.0,             y: 0.0 },
        tree::PathSegment::LineTo { x: cluster.advance, y: 0.0 },
    ];
    parent.append_kind(tree::NodeKind::Path(base_path));
}

fn process_anchor(a: TextAnchor, text_width: f64) -> f64 {
    match a {
        TextAnchor::Start   => 0.0, // Nothing.
        TextAnchor::Middle  => text_width / 2.0,
        TextAnchor::End     => text_width,
    }
}

fn collect_decoration_spans(
    span: &TextSpan,
    clusters: &[OutlinedCluster],
) -> Vec<DecorationSpan> {
    let mut spans = Vec::new();

    let mut started = false;
    let mut width = 0.0;
    let mut transform = tree::Transform::default();
    for cluster in clusters {
        if span.contains(cluster.byte_idx) {
            if started && cluster.has_relative_shift {
                started = false;
                spans.push(DecorationSpan { width, transform });
            }

            if !started {
                width = cluster.advance;
                started = true;
                transform = cluster.transform;
            } else {
                width += cluster.advance;
            }
        } else if started {
            spans.push(DecorationSpan { width, transform });
            started = false;
        }
    }

    if started {
        spans.push(DecorationSpan { width, transform });
    }

    spans
}

fn convert_decoration(
    dy: f64,
    span: &TextSpan,
    mut decoration: TextDecorationStyle,
    decoration_spans: &[DecorationSpan],
    transform: tree::Transform,
) -> tree::Path {
    debug_assert!(!decoration_spans.is_empty());

    let mut segments = Vec::new();
    for dec_span in decoration_spans {
        let rect = Rect::new(
            0.0,
            0.0,
            dec_span.width,
            span.font.underline_thickness(span.font_size),
        ).unwrap();

        let start_idx = segments.len();
        add_rect_to_path(rect, &mut segments);

        let mut ts = dec_span.transform;
        ts.translate(0.0, dy);
        transform_path(&mut segments[start_idx..], &ts);
    }

    tree::Path {
        id: String::new(),
        transform,
        visibility: span.visibility,
        fill: decoration.fill.take(),
        stroke: decoration.stroke.take(),
        rendering_mode: tree::ShapeRendering::default(),
        segments,
    }
}

fn add_rect_to_path(rect: Rect, path: &mut Vec<tree::PathSegment>) {
    path.extend_from_slice(&[
        tree::PathSegment::MoveTo { x: rect.x(),     y: rect.y() },
        tree::PathSegment::LineTo { x: rect.right(), y: rect.y() },
        tree::PathSegment::LineTo { x: rect.right(), y: rect.bottom() },
        tree::PathSegment::LineTo { x: rect.x(),     y: rect.bottom() },
        tree::PathSegment::ClosePath,
    ]);
}

/// By the SVG spec, `tspan` doesn't have a bbox and uses the parent `text` bbox.
/// Since we converted `text` and `tspan` to `path`, we have to update
/// all linked paint servers (gradients and patterns) too.
fn fix_obj_bounding_box(
    path: &mut tree::Path,
    bbox: Rect,
    tree: &mut tree::Tree,
) {
    if let Some(ref mut fill) = path.fill {
        if let tree::Paint::Link(ref mut id) = fill.paint {
            if let Some(new_id) = paint_server_to_user_space_on_use(id, bbox, tree) {
                *id = new_id;
            }
        }
    }

    if let Some(ref mut stroke) = path.stroke {
        if let tree::Paint::Link(ref mut id) = stroke.paint {
            if let Some(new_id) = paint_server_to_user_space_on_use(id, bbox, tree) {
                *id = new_id;
            }
        }
    }
}

/// Converts a selected paint server's units to `UserSpaceOnUse`.
///
/// Creates a deep copy of a selected paint server and returns its ID.
///
/// Returns `None` if a paint server already uses `UserSpaceOnUse`.
fn paint_server_to_user_space_on_use(
    id: &str,
    bbox: Rect,
    tree: &mut tree::Tree,
) -> Option<String> {
    if let Some(ps) = tree.defs_by_id(id) {
        let is_obj_bbox = match *ps.borrow() {
            tree::NodeKind::LinearGradient(ref lg) => {
                lg.units == tree::Units::ObjectBoundingBox
            }
            tree::NodeKind::RadialGradient(ref rg) => {
                rg.units == tree::Units::ObjectBoundingBox
            }
            tree::NodeKind::Pattern(ref patt) => {
                patt.units == tree::Units::ObjectBoundingBox
            }
            _ => false,
        };

        // Do nothing.
        if !is_obj_bbox {
            return None;
        }


        // TODO: is `pattern` copying safe? Maybe we should reset id's on all `pattern` children.
        // We have to clone a paint server, in case some other element is already using it.
        // If not, the `convert` module will remove unused defs anyway.
        let mut new_ps = ps.clone().make_deep_copy();
        tree.defs().append(new_ps.clone());

        let new_id = gen_paint_server_id(tree);

        // Update id, transform and units.
        match *new_ps.borrow_mut() {
            tree::NodeKind::LinearGradient(ref mut lg) => {
                if lg.units == tree::Units::ObjectBoundingBox {
                    lg.id = new_id.clone();
                    lg.base.transform.prepend(&tree::Transform::from_bbox(bbox));
                    lg.base.units = tree::Units::UserSpaceOnUse;
                }
            }
            tree::NodeKind::RadialGradient(ref mut rg) => {
                if rg.units == tree::Units::ObjectBoundingBox {
                    rg.id = new_id.clone();
                    rg.base.transform.prepend(&tree::Transform::from_bbox(bbox));
                    rg.base.units = tree::Units::UserSpaceOnUse;
                }
            }
            tree::NodeKind::Pattern(ref mut patt) => {
                if patt.units == tree::Units::ObjectBoundingBox {
                    patt.id = new_id.clone();
                    patt.transform.prepend(&tree::Transform::from_bbox(bbox));
                    patt.units = tree::Units::UserSpaceOnUse;
                }
            }
            _ => return None,
        }

        Some(new_id)
    } else {
        None
    }
}

/// Creates a free id for a paint server.
fn gen_paint_server_id(
    tree: &tree::Tree,
) -> String {
    // TODO: speed up

    let mut idx = 1;
    let mut id = format!("usvg{}", idx);
    while tree.defs().children().any(|n| *n.id() == id) {
        idx += 1;
        id = format!("usvg{}", idx);
    }

    id
}
