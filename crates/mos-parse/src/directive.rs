use mos_core::Diagnostic;
use mos_core::codes;

use crate::parser::Parser;
use crate::support::{
    next_char_boundary, normalize_raw_text, skip_set_ws, skip_to_comma, strip_leading_label,
};
use crate::{DirectiveKind, Item, LengthUnit, RawBlockKind, SetArg, SetValue};

impl Parser<'_> {
    pub(crate) fn parse_directive_block(&mut self, kw: &'static str) {
        if kw == "set" {
            self.parse_set_block();
        } else if kw == "pre" || kw == "code" {
            self.parse_raw_block(kw);
        } else {
            self.parse_call_block(kw);
        }
    }

    fn parse_raw_block(&mut self, kw: &'static str) {
        let (line_start, _content_end, _line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        debug_assert!(self.src[line_start + 1..].starts_with(kw));
        let mut i = line_start + 1 + kw.len();
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let mut args = Vec::new();
        if i < bytes.len() && bytes[i] == b'(' {
            let Some(args_end) = self.scan_balanced_parens(i) else {
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0016,
                        None,
                        format!("unterminated `#{kw}(...)` block"),
                    )
                    .with_span(self.span(line_start, bytes.len())),
                );
                self.pos = bytes.len();
                return;
            };
            args = self.parse_set_body(i + 1, args_end - 1, true);
            i = args_end;
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
        }
        if i >= bytes.len() || bytes[i] != b'[' {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0013,
                    None,
                    format!(
                        "expected long-bracket raw body after `#{kw}` (for example `#{kw}[[...]]`)"
                    ),
                )
                .with_span(self.span(line_start, i)),
            );
            self.skip_line();
            return;
        }
        let Some((body_start, eq_count)) = self.scan_long_raw_open(i) else {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0013,
                    None,
                    format!("raw `#{kw}` blocks require long brackets like `#{kw}[[...]]`"),
                )
                .with_span(self.span(line_start, i + 1)),
            );
            self.skip_line();
            return;
        };
        if let Some((body_end, close_end)) = self.scan_long_raw_close(body_start, eq_count) {
            let text = normalize_raw_text(&self.src[body_start..body_end]);
            let (_, content_end, _) = self.line_bounds_from(close_end);
            let (after_label, parsed_label) = strip_leading_label(self.src, close_end, content_end);
            let label_span = parsed_label
                .as_ref()
                .map(|label| self.span(label.start, label.end));
            let label = parsed_label.map(|label| label.text);
            let kind = if kw == "code" {
                RawBlockKind::Code
            } else {
                RawBlockKind::Pre
            };
            self.items.push(Item::RawBlock {
                kind,
                args,
                text,
                label,
                label_span,
                span: self.span(line_start, after_label),
            });
            self.pos = after_label;
            while self.pos < bytes.len() && (bytes[self.pos] == b' ' || bytes[self.pos] == b'\t') {
                self.pos += 1;
            }
            if self.pos >= bytes.len() {
            } else if bytes[self.pos] == b'\n' {
                self.pos += 1;
            } else if bytes[self.pos] == b'\r' && bytes.get(self.pos + 1) == Some(&b'\n') {
                self.pos += 2;
            } else {
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0019,
                        None,
                        format!("unexpected trailing content after raw `#{kw}` block"),
                    )
                    .with_span(self.span(self.pos, content_end)),
                );
            }
        } else {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0016,
                    None,
                    format!("unterminated raw `#{kw}` long-bracket block"),
                )
                .with_span(self.span(line_start, bytes.len())),
            );
            self.pos = bytes.len();
        }
    }

    fn parse_set_block(&mut self) {
        let (line_start, _content_end, _line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        debug_assert!(self.src[line_start..].starts_with("#set"));
        let mut i = line_start + "#set".len();
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let name = self.src[name_start..i].to_owned();
        if name.is_empty() {
            self.diagnostics.push(
                Diagnostic::simple(&codes::MOS0010, None, "expected an identifier after `#set`")
                    .with_span(self.span(line_start, line_start + "#set".len())),
            );
            self.skip_line();
            return;
        }
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0013,
                    None,
                    format!("expected `(` after `#set {name}`"),
                )
                .with_span(self.span(name_start, i)),
            );
            self.skip_line();
            return;
        }
        self.finish_directive_block(line_start, i, DirectiveKind::Set, name, "set", false);
    }

    fn parse_call_block(&mut self, kw: &'static str) {
        let (line_start, _content_end, _line_end) = self.current_line_bounds();
        let bytes = self.src.as_bytes();
        debug_assert!(self.src[line_start + 1..].starts_with(kw));
        let mut i = line_start + 1 + kw.len();
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            self.diagnostics.push(
                Diagnostic::simple(&codes::MOS0013, None, format!("expected `(` after `#{kw}`"))
                    .with_span(self.span(line_start, i)),
            );
            self.skip_line();
            return;
        }
        let kind = match kw {
            "image" => DirectiveKind::Image,
            "figure" => DirectiveKind::Figure,
            other => {
                debug_assert!(false, "parse_call_block: unexpected keyword `{other}`");
                DirectiveKind::Set
            }
        };
        self.finish_directive_block(line_start, i, kind, kw.to_owned(), kw, true);
    }

    fn finish_directive_block(
        &mut self,
        line_start: usize,
        paren_pos: usize,
        kind: DirectiveKind,
        name: String,
        display_kw: &str,
        allow_positional: bool,
    ) {
        let bytes = self.src.as_bytes();
        if let Some(end) = self.scan_balanced_parens(paren_pos) {
            let args = self.parse_set_body(paren_pos + 1, end - 1, allow_positional);
            self.items.push(Item::Set {
                kind,
                name,
                args,
                span: self.span(line_start, end),
            });
            self.pos = end;
            while self.pos < bytes.len() && (bytes[self.pos] == b' ' || bytes[self.pos] == b'\t') {
                self.pos += 1;
            }
            if self.pos >= bytes.len() {
            } else if bytes[self.pos] == b'\n' {
                self.pos += 1;
            } else if bytes[self.pos] == b'\r' && bytes.get(self.pos + 1) == Some(&b'\n') {
                self.pos += 2;
            } else {
                let (_, content_end, _) = self.current_line_bounds();
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0019,
                        None,
                        format!("unexpected trailing content after `#{display_kw} ... )`"),
                    )
                    .with_span(self.span(self.pos, content_end)),
                );
            }
        } else {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0016,
                    None,
                    format!("unterminated `#{display_kw}(...)` block"),
                )
                .with_span(self.span(line_start, bytes.len())),
            );
            self.pos = bytes.len();
        }
    }

    fn scan_balanced_parens(&self, start: usize) -> Option<usize> {
        let bytes = self.src.as_bytes();
        debug_assert_eq!(bytes.get(start), Some(&b'('));
        let mut depth: u32 = 0;
        let mut i = start;
        let mut in_string = false;
        while i < bytes.len() {
            let b = bytes[i];
            if in_string {
                if b == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if b == b'"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn scan_long_raw_open(&self, start: usize) -> Option<(usize, usize)> {
        let bytes = self.src.as_bytes();
        debug_assert_eq!(bytes.get(start), Some(&b'['));
        let mut i = start + 1;
        while i < bytes.len() && bytes[i] == b'=' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'[' {
            return None;
        }
        Some((i + 1, i - start - 1))
    }

    fn scan_long_raw_close(&self, start: usize, eq_count: usize) -> Option<(usize, usize)> {
        let bytes = self.src.as_bytes();
        let mut i = start;
        while i < bytes.len() {
            if bytes[i] == b']' {
                let eq_start = i + 1;
                let eq_end = eq_start + eq_count;
                if eq_end < bytes.len()
                    && bytes[eq_start..eq_end].iter().all(|b| *b == b'=')
                    && bytes[eq_end] == b']'
                {
                    return Some((i, eq_end + 1));
                }
            }
            i += 1;
        }
        None
    }

    fn parse_set_body(&mut self, start: usize, end: usize, allow_positional: bool) -> Vec<SetArg> {
        let bytes = self.src.as_bytes();
        let mut args: Vec<SetArg> = Vec::new();
        let mut i = start;
        let mut first = true;
        loop {
            i = skip_set_ws(bytes, i, end);
            if i >= end {
                break;
            }
            if allow_positional && first && bytes[i] == b'"' {
                let value_start = i;
                let parsed = self.parse_set_value(&mut i, end);
                let value_span = self.span(value_start, i);
                if let Some(value) = parsed {
                    args.push(SetArg::Positional { value, value_span });
                }
                first = false;
                i = self.consume_arg_separator(bytes, i, end);
                continue;
            }
            first = false;
            let key_start = i;
            while i < end && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-')) {
                i += 1;
            }
            if i == key_start {
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0025,
                        None,
                        "expected `key: value` in directive arguments",
                    )
                    .with_span(self.span(i, (i + 1).min(end))),
                );
                i = skip_to_comma(bytes, i, end);
                if i < end && bytes[i] == b',' {
                    i += 1;
                }
                continue;
            }
            let key = self.src[key_start..i].to_owned();
            let key_span = self.span(key_start, i);
            i = skip_set_ws(bytes, i, end);
            if i >= end || bytes[i] != b':' {
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0025,
                        None,
                        format!("expected `:` after `{key}` in directive arguments"),
                    )
                    .with_span(key_span.clone()),
                );
                i = skip_to_comma(bytes, i, end);
                if i < end && bytes[i] == b',' {
                    i += 1;
                }
                continue;
            }
            i += 1;
            i = skip_set_ws(bytes, i, end);
            let value_start = i;
            let parsed = self.parse_set_value(&mut i, end);
            let value_span = self.span(value_start, i);
            if let Some(value) = parsed {
                args.push(SetArg::Named {
                    key,
                    value,
                    key_span,
                    value_span,
                });
            }
            i = self.consume_arg_separator(bytes, i, end);
        }
        args
    }

    fn consume_arg_separator(&mut self, bytes: &[u8], mut i: usize, end: usize) -> usize {
        i = skip_set_ws(bytes, i, end);
        if i < end {
            if bytes[i] == b',' {
                i += 1;
            } else {
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0025,
                        None,
                        "expected `,` or `)` between directive arguments",
                    )
                    .with_span(self.span(i, (i + 1).min(end))),
                );
                i = skip_to_comma(bytes, i, end);
                if i < end && bytes[i] == b',' {
                    i += 1;
                }
            }
        }
        i
    }

    fn parse_set_value(&mut self, i: &mut usize, end: usize) -> Option<SetValue> {
        let bytes = self.src.as_bytes();
        if *i >= end {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0022,
                    None,
                    "expected a value in directive arguments",
                )
                .with_span(self.span(*i, *i)),
            );
            return None;
        }
        let b = bytes[*i];
        if b == b'"' {
            return self.parse_string_value(i, end);
        }
        if b == b'-' || b.is_ascii_digit() {
            return self.parse_number_value(i, end);
        }
        if b.is_ascii_alphabetic() {
            let id_start = *i;
            while *i < end
                && (bytes[*i].is_ascii_alphanumeric() || matches!(bytes[*i], b'_' | b'-'))
            {
                *i += 1;
            }
            return Some(SetValue::Ident(self.src[id_start..*i].to_owned()));
        }
        self.diagnostics.push(
            Diagnostic::simple(
                &codes::MOS0022,
                None,
                format!("unexpected character `{}` in directive value", b as char),
            )
            .with_span(self.span(*i, *i + 1)),
        );
        *i += 1;
        None
    }

    fn parse_string_value(&mut self, i: &mut usize, end: usize) -> Option<SetValue> {
        let bytes = self.src.as_bytes();
        let start = *i;
        *i += 1;
        let mut out = String::new();
        while *i < end {
            let c = bytes[*i];
            if c == b'\\' && *i + 1 < end {
                let esc = bytes[*i + 1];
                match esc {
                    b'\\' => out.push('\\'),
                    b'"' => out.push('"'),
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    _ => {
                        let esc_start = *i + 1;
                        let esc_end = next_char_boundary(self.src, esc_start);
                        self.diagnostics.push(
                            Diagnostic::simple(
                                &codes::MOS0022,
                                None,
                                format!(
                                    "unknown escape sequence `\\{}` in string",
                                    &self.src[esc_start..esc_end]
                                ),
                            )
                            .with_span(self.span(*i, esc_end)),
                        );
                        out.push_str(&self.src[esc_start..esc_end]);
                        *i = esc_end;
                        continue;
                    }
                }
                *i += 2;
                continue;
            }
            if c == b'"' {
                *i += 1;
                return Some(SetValue::Str(out));
            }
            let ch_start = *i;
            let ch_end = next_char_boundary(self.src, ch_start);
            out.push_str(&self.src[ch_start..ch_end]);
            *i = ch_end;
        }
        self.diagnostics.push(
            Diagnostic::simple(&codes::MOS0022, None, "unterminated string literal")
                .with_span(self.span(start, end)),
        );
        None
    }

    fn parse_number_value(&mut self, i: &mut usize, end: usize) -> Option<SetValue> {
        let bytes = self.src.as_bytes();
        let num_start = *i;
        if bytes[*i] == b'-' {
            *i += 1;
        }
        let int_start = *i;
        while *i < end && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        let mut is_float = false;
        if *i < end && bytes[*i] == b'.' && *i + 1 < end && bytes[*i + 1].is_ascii_digit() {
            is_float = true;
            *i += 1;
            while *i < end && bytes[*i].is_ascii_digit() {
                *i += 1;
            }
        }
        if *i == int_start {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0022,
                    None,
                    "expected a number after `-` in directive value",
                )
                .with_span(self.span(num_start, *i)),
            );
            return None;
        }
        let num_end = *i;
        let unit_start = *i;
        while *i < end && bytes[*i].is_ascii_alphabetic() {
            *i += 1;
        }
        let unit = &self.src[unit_start..*i];
        if unit.is_empty() {
            let text = &self.src[num_start..num_end];
            if is_float {
                return text.parse::<f64>().ok().map(SetValue::Float).or_else(|| {
                    self.diagnostics.push(
                        Diagnostic::simple(
                            &codes::MOS0022,
                            None,
                            format!("malformed number `{text}`"),
                        )
                        .with_span(self.span(num_start, num_end)),
                    );
                    None
                });
            }
            return text.parse::<i64>().ok().map(SetValue::Int).or_else(|| {
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0022,
                        None,
                        format!("malformed integer `{text}`"),
                    )
                    .with_span(self.span(num_start, num_end)),
                );
                None
            });
        }
        let length_unit = match unit {
            "mm" => LengthUnit::Mm,
            "pt" => LengthUnit::Pt,
            "em" => LengthUnit::Em,
            _ => {
                self.diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0022,
                        None,
                        format!("unknown length unit `{unit}` (expected mm, pt, or em)"),
                    )
                    .with_span(self.span(unit_start, *i)),
                );
                return None;
            }
        };
        let value = self.src[num_start..num_end].parse::<f64>().ok();
        value.map(|v| SetValue::Length(v, length_unit)).or_else(|| {
            self.diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0022,
                    None,
                    format!("malformed length value `{}`", &self.src[num_start..num_end]),
                )
                .with_span(self.span(num_start, num_end)),
            );
            None
        })
    }
}
