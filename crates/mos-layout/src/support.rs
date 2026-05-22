use std::borrow::Cow;

use mos_core::{AttrValue, Node};

use crate::style::pt_to_f32;
use crate::{Page, PageStyle};

pub(crate) fn blank_page(number: u32, style: PageStyle) -> Page {
    Page {
        number,
        width_pt: style.width_pt,
        height_pt: style.height_pt,
        runs: Vec::new(),
        images: Vec::new(),
    }
}

pub(crate) fn read_level(section: &Node) -> Option<u8> {
    match section.attributes.get("level") {
        Some(AttrValue::Int(n)) if *n >= 1 => u8::try_from((*n).clamp(1, 255)).ok(),
        _ => None,
    }
}

pub(crate) fn read_str_attr<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node.attributes.get(key) {
        Some(AttrValue::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

pub(crate) fn read_int_attr(node: &Node, key: &str) -> Option<i64> {
    match node.attributes.get(key) {
        Some(AttrValue::Int(n)) => Some(*n),
        _ => None,
    }
}

pub(crate) fn read_length_attr(node: &Node, key: &str) -> Option<f32> {
    match node.attributes.get(key) {
        Some(AttrValue::Length(pt)) => Some(pt_to_f32(*pt)),
        _ => None,
    }
}

pub(crate) fn expand_tabs(line: &str, tab_width: usize) -> Cow<'_, str> {
    if !line.contains('\t') {
        return Cow::Borrowed(line);
    }

    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(line.len());
    let mut col = 0_usize;
    for ch in line.chars() {
        if ch == '\t' {
            let spaces = tab_width - (col % tab_width);
            out.extend(std::iter::repeat_n(' ', spaces));
            col += spaces;
        } else {
            out.push(ch);
            col += 1;
        }
    }
    Cow::Owned(out)
}
