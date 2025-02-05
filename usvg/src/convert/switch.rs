// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// external
use svgdom;

// self
use crate::tree;
use super::prelude::*;


// Full list can be found here: https://www.w3.org/TR/SVG11/feature.html
static FEATURES: &[&str] = &[
    "http://www.w3.org/TR/SVG11/feature#SVGDOM-static",
    "http://www.w3.org/TR/SVG11/feature#SVG-static",
    "http://www.w3.org/TR/SVG11/feature#CoreAttribute", // no xml:base and xml:lang
    "http://www.w3.org/TR/SVG11/feature#Structure",
    "http://www.w3.org/TR/SVG11/feature#BasicStructure",
    // "http://www.w3.org/TR/SVG11/feature#ContainerAttribute", // `enable-background`, not yet
    "http://www.w3.org/TR/SVG11/feature#ConditionalProcessing",
    "http://www.w3.org/TR/SVG11/feature#Image",
    "http://www.w3.org/TR/SVG11/feature#Style",
    // "http://www.w3.org/TR/SVG11/feature#ViewportAttribute", // `clip` and `overflow`, not yet
    "http://www.w3.org/TR/SVG11/feature#Shape",
    "http://www.w3.org/TR/SVG11/feature#Text",
    "http://www.w3.org/TR/SVG11/feature#BasicText",
    "http://www.w3.org/TR/SVG11/feature#PaintAttribute", // no color-interpolation and color-rendering
    "http://www.w3.org/TR/SVG11/feature#BasicPaintAttribute", // no color-interpolation
    "http://www.w3.org/TR/SVG11/feature#OpacityAttribute",
    // "http://www.w3.org/TR/SVG11/feature#GraphicsAttribute",
    "http://www.w3.org/TR/SVG11/feature#BasicGraphicsAttribute",
    "http://www.w3.org/TR/SVG11/feature#Marker",
    // "http://www.w3.org/TR/SVG11/feature#ColorProfile", // not yet
    "http://www.w3.org/TR/SVG11/feature#Gradient",
    "http://www.w3.org/TR/SVG11/feature#Pattern",
    "http://www.w3.org/TR/SVG11/feature#Clip",
    "http://www.w3.org/TR/SVG11/feature#BasicClip",
    "http://www.w3.org/TR/SVG11/feature#Mask",
    // "http://www.w3.org/TR/SVG11/feature#Filter", // not yet
    "http://www.w3.org/TR/SVG11/feature#BasicFilter",
    "http://www.w3.org/TR/SVG11/feature#XlinkAttribute", // only xlink:href
    // "http://www.w3.org/TR/SVG11/feature#Font",
    // "http://www.w3.org/TR/SVG11/feature#BasicFont",
];


pub fn convert(
    node: &svgdom::Node,
    state: &State,
    parent: &mut tree::Node,
    tree: &mut tree::Tree,
) {
    let child = node.children().find(|n| is_condition_passed(&n, state.opt));
    let child = try_opt!(child);

    match super::convert_group(&node, state, false, parent, tree) {
        super::GroupKind::Keep(mut g) => {
            super::convert_element(&child, state, &mut g, tree);
        }
        super::GroupKind::Skip => {
            super::convert_element(&child, state, parent, tree);
        }
        super::GroupKind::Ignore => {}
    }
}

pub fn is_condition_passed(
    node: &svgdom::Node,
    opt: &Options,
) -> bool {
    if !node.is_svg_element() {
        return false;
    }

    let ref attrs = node.attributes();

    if attrs.contains(AId::RequiredExtensions) {
        return false;
    }

    // 'The value is a list of feature strings, with the individual values separated by white space.
    // Determines whether all of the named features are supported by the user agent.
    // Only feature strings defined in the Feature String appendix are allowed.
    // If all of the given features are supported, then the attribute evaluates to true;
    // otherwise, the current element and its children are skipped and thus will not be rendered.'
    if let Some(features) = attrs.get_str(AId::RequiredFeatures) {
        for feature in features.split(' ') {
            if !FEATURES.contains(&feature) {
                return false;
            }
        }
    }

    if !is_valid_sys_lang(attrs, opt) {
        return false;
    }

    true
}

/// SVG spec 5.8.5
fn is_valid_sys_lang(
    attrs: &svgdom::Attributes,
    opt: &Options,
) -> bool {
    // 'The attribute value is a comma-separated list of language names
    // as defined in BCP 47.'
    //
    // But we support only simple cases like `en` or `en-US`.
    // No one really uses this, especially with complex BCP 47 values.
    if let Some(langs) = attrs.get_str(AId::SystemLanguage) {
        let mut has_match = false;
        for lang in langs.split(',') {
            let lang = lang.trim();

            // 'Evaluates to `true` if one of the languages indicated by user preferences exactly
            // equals one of the languages given in the value of this parameter.'
            if opt.languages.iter().any(|v| v == lang) {
                has_match = true;
                break;
            }

            // 'If one of the languages indicated by user preferences exactly equals a prefix
            // of one of the languages given in the value of this parameter such that
            // the first tag character following the prefix is `-`.'
            if let Some(idx) = lang.bytes().position(|c| c == b'-') {
                let lang_prefix = &lang[..idx];
                if opt.languages.iter().any(|v| v == lang_prefix) {
                    has_match = true;
                    break;
                }
            }
        }

        return has_match;
    }

    true
}
