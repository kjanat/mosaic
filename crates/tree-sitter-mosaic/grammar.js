/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/**
 * Tree-sitter grammar for the Mosaic `.mos` document language.
 *
 * Mirrors `mosaic.ebnf` (also rendered in `EBNF.md`) 1:1 in structure. The
 * four tokens that regex-only lexing cannot express cleanly (`blank_line`
 * and raw `#pre`/`#code` long-bracket delimiters/content) are emitted by
 * the external scanner in `src/scanner.c`.
 *
 * @file Mosaic grammar for Tree-sitter
 * @author Kaj Kowalski <info@kajkowalski.nl>
 * @license MIT
 */

const PREC = {
	emphasis: 1,
	strong: 2,
	strong_emphasis: 3,
	linebreak_call: 2,
	hash_call: 1,
	attribute: 1,
	heading_marker: 1,
	list_marker: 1,
};

export default grammar({
	name: 'mosaic',

	// oxlint-disable-next-line no-unused-vars
	extras: $ => [
		/[\t ]+/,
	],

	externals: $ => [
		$.blank_line,
		$.raw_body_open,
		$.raw_body_content,
		$.raw_body_close,
		$._error_sentinel,
	],

	word: $ => $.identifier,

	supertypes: $ => [
		$._block,
		$._inline,
		$._expression,
	],

	precedences: $ => [
		[$.strong_emphasis, $.strong, $.emphasis],
	],

	conflicts: $ => [
		[$.block_call, $.inline_call],
		[$.paragraph, $.soft_break],
	],

	rules: {
		// -------------------------------------------------------------------
		// Document
		// -------------------------------------------------------------------

		source_file: $ =>
			seq(
				optional($.shebang),
				repeat(choice($.blank_line, $.section, $._block, $._line_end)),
			),

		section: $ =>
			choice(
				$.section1,
				$.section2,
				$.section3,
				$.section4,
				$.section5,
				$.section6,
			),

		section1: $ =>
			prec.right(seq(
				alias($._heading1, $.heading),
				repeat(choice($.blank_line, $.section2, $.section3, $.section4, $.section5, $.section6, $._block, $._line_end)),
			)),

		section2: $ =>
			prec.right(seq(
				alias($._heading2, $.heading),
				repeat(choice($.blank_line, $.section3, $.section4, $.section5, $.section6, $._block, $._line_end)),
			)),

		section3: $ =>
			prec.right(seq(
				alias($._heading3, $.heading),
				repeat(choice($.blank_line, $.section4, $.section5, $.section6, $._block, $._line_end)),
			)),

		section4: $ =>
			prec.right(
				seq(alias($._heading4, $.heading), repeat(choice($.blank_line, $.section5, $.section6, $._block, $._line_end))),
			),

		section5: $ =>
			prec.right(seq(alias($._heading5, $.heading), repeat(choice($.blank_line, $.section6, $._block, $._line_end)))),

		section6: $ => prec.right(seq(alias($._heading6, $.heading), repeat(choice($.blank_line, $._block, $._line_end)))),

		_block: $ =>
			choice(
				$.comment,
				$.set_directive,
				$.import_directive,
				$.include_directive,
				$.list,
				$.verse_block,
				$.pre_block,
				$.code_block,
				$.block_call,
				$.paragraph,
			),

		_line_end: _ => choice('\n', '\r\n', '\r'),

		// Hidden helper: one or more line endings used inside multi-line
		// expression contexts (`argument_list`, `array`, `object`). Wrapped
		// in a single rule so tree-sitter can reason about each insertion
		// site as one symbol rather than adjacent repeats.
		_nl: $ => prec.right(repeat1($._line_end)),

		// -------------------------------------------------------------------
		// Lexical trivia
		// -------------------------------------------------------------------

		comment: _ =>
			token(choice(
				seq('//', /[^\n\r]*/),
				seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
			)),

		shebang: _ => token(prec(2, seq('#!', /[^\n\r]*/))),

		_hash_bang_text: _ => token(prec(1, seq('#!', /[^\n\r]*/))),

		// -------------------------------------------------------------------
		// Block directives
		// -------------------------------------------------------------------

		set_directive: $ =>
			prec.right(seq(
				'#set',
				field('target', $.identifier),
				field('arguments', $.argument_list),
				optional($._line_end),
			)),

		import_directive: $ =>
			prec.right(seq(
				'#import',
				field('path', $.string),
				optional(seq(':', field('items', $.import_items))),
				optional($._line_end),
			)),

		import_items: $ => seq($.identifier, repeat(seq(',', $.identifier))),

		include_directive: $ =>
			prec.right(seq(
				'#include',
				field('path', $.string),
				optional($._line_end),
			)),

		// -------------------------------------------------------------------
		// Headings
		// -------------------------------------------------------------------

		_heading1: $ =>
			prec.right(seq(
				field('marker', alias($.heading_marker1, $.heading_marker)),
				field('content', alias($._inline_sequence, $.inline_sequence)),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		_heading2: $ =>
			prec.right(seq(
				field('marker', alias($.heading_marker2, $.heading_marker)),
				field('content', alias($._inline_sequence, $.inline_sequence)),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		_heading3: $ =>
			prec.right(seq(
				field('marker', alias($.heading_marker3, $.heading_marker)),
				field('content', alias($._inline_sequence, $.inline_sequence)),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		_heading4: $ =>
			prec.right(seq(
				field('marker', alias($.heading_marker4, $.heading_marker)),
				field('content', alias($._inline_sequence, $.inline_sequence)),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		_heading5: $ =>
			prec.right(seq(
				field('marker', alias($.heading_marker5, $.heading_marker)),
				field('content', alias($._inline_sequence, $.inline_sequence)),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		_heading6: $ =>
			prec.right(seq(
				field('marker', alias($.heading_marker6, $.heading_marker)),
				field('content', alias($._inline_sequence, $.inline_sequence)),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		// EBNF semantic restriction is 1..6 `=`; folded the required hspace1
		// into the token so a bare run of `=` cannot start a heading.
		heading_marker1: _ => token(prec(PREC.heading_marker, /=[ \t]+/)),
		heading_marker2: _ => token(prec(PREC.heading_marker, /==[ \t]+/)),
		heading_marker3: _ => token(prec(PREC.heading_marker, /===[ \t]+/)),
		heading_marker4: _ => token(prec(PREC.heading_marker, /====[ \t]+/)),
		heading_marker5: _ => token(prec(PREC.heading_marker, /=====[ \t]+/)),
		heading_marker6: _ => token(prec(PREC.heading_marker, /======[ \t]+/)),

		// -------------------------------------------------------------------
		// Lists
		// -------------------------------------------------------------------

		list: $ => prec.right(seq($.list_item, repeat(seq($._line_end, $.list_item)))),

		list_item: $ =>
			seq(
				field('marker', choice($.unordered_list_marker, $.ordered_list_marker)),
				field('content', alias($._inline_sequence, $.inline_sequence)),
			),

		unordered_list_marker: _ => token(prec(PREC.list_marker, /-[ \t]+/)),

		ordered_list_marker: _ => token(prec(PREC.list_marker, /[0-9]+\.[ \t]+/)),

		// -------------------------------------------------------------------
		// Verse / Pre / Code blocks
		// -------------------------------------------------------------------

		verse_block: $ =>
			prec.right(seq(
				'#verse',
				optional(field('arguments', $.argument_list)),
				field('body', $.verse_body),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		pre_block: $ =>
			prec.right(seq(
				'#pre',
				optional(field('arguments', $.argument_list)),
				field('body', $.raw_body),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		code_block: $ =>
			prec.right(seq(
				'#code',
				optional(field('arguments', $.argument_list)),
				field('body', $.raw_body),
				optional(field('label', $.block_label)),
				optional($._line_end),
			)),

		raw_body: $ => seq($.raw_body_open, optional($.raw_body_content), $.raw_body_close),

		verse_body: $ =>
			seq(
				'[',
				optional(seq(
					$._verse_line,
					repeat(seq($._line_end, $._verse_line)),
					optional($._line_end),
				)),
				']',
			),

		_verse_line: $ => repeat1(choice($._verse_inline, $.verse_text)),

		_verse_inline: $ =>
			choice(
				$.strong_emphasis,
				$.strong,
				$.emphasis,
				$.code_span,
				$.inline_math,
				$.citation,
				$.reference,
				$.linebreak_call,
				$.inline_call,
				$.hard_break,
				$.soft_hyphen_escape,
				$.escaped_char,
				$.loose_backslash,
			),

		verse_text: _ => token(prec(-1, /[^\n\r\]\\*`$@<#]+/)),

		// -------------------------------------------------------------------
		// Paragraphs
		// -------------------------------------------------------------------

		paragraph: $ =>
			prec.right(seq(
				optional(field('leading_label', $.leading_label)),
				alias($._inline_sequence, $.paragraph_segment),
				repeat(seq(
					$._paragraph_join,
					alias($._inline_sequence, $.paragraph_segment),
				)),
				optional(field('trailing_label', $.trailing_label)),
				optional($._line_end),
			)),

		leading_label: $ => $.block_label,
		trailing_label: $ => $.block_label,
		block_label: $ => $.label,

		// Now that `\\` is a real inline atom (`hard_break`), the only thing
		// joining adjacent paragraph segments is a plain newline. The rule is
		// kept as a hidden alias so a future grammar change (e.g. an explicit
		// `#linebreak` block-level form) can extend it without rewriting every
		// `paragraph` call site.
		_paragraph_join: $ => $.soft_break,

		soft_break: $ => $._line_end,

		// -------------------------------------------------------------------
		// Inline
		// -------------------------------------------------------------------

		_inline_sequence: $ => prec.right(repeat1($._inline_atom)),

		// EBNF lists `label` as an inline atom, but the suggested CST in
		// EBNF.md and every realistic example puts `<name>` in trailing
		// block-label position. Including it inline causes `inline_sequence`
		// to greedily eat the block label. Keep labels as `block_label`
		// only; literal `<` in prose is supported via `\<`.
		_inline_atom: $ =>
			choice(
				$.strong_emphasis,
				$.strong,
				$.emphasis,
				$.code_span,
				$.inline_math,
				$.citation,
				$.reference,
				$.linebreak_call,
				$.inline_call,
				$.hard_break,
				$.soft_hyphen_escape,
				$.escaped_char,
				$.loose_backslash,
				alias($._hash_bang_text, $.text),
				$.text,
			),

		_inline: $ =>
			choice(
				$.strong_emphasis,
				$.strong,
				$.emphasis,
				$.code_span,
				$.inline_math,
				$.citation,
				$.reference,
				$.linebreak_call,
				$.inline_call,
				$.hard_break,
				$.soft_hyphen_escape,
				$.escaped_char,
				$.loose_backslash,
				alias($._hash_bang_text, $.text),
				$.text,
			),

		// EBNF defines `inline_call = hash_call | call_expr`. In practice the
		// `call_expr` alternative (a qualified_name immediately followed by
		// `(...)` without a leading `#`) is unreachable in paragraph
		// position because `text` swallows the leading identifier first.
		// We keep `call_expr` available in expression position only.
		inline_call: $ => $.hash_call,

		emphasis: $ =>
			prec.dynamic(
				PREC.emphasis,
				seq(
					'*',
					repeat1($._emphasis_unit),
					'*',
				),
			),

		strong: $ =>
			prec.dynamic(
				PREC.strong,
				seq(
					'**',
					repeat1($._strong_unit),
					'**',
				),
			),

		strong_emphasis: $ =>
			prec.dynamic(
				PREC.strong_emphasis,
				seq(
					'***',
					repeat1($._strong_emphasis_unit),
					'***',
				),
			),

		_emphasis_unit: $ =>
			choice(
				$.strong_emphasis,
				$.strong,
				$.code_span,
				$.inline_math,
				$.citation,
				$.reference,
				$.linebreak_call,
				$.inline_call,
				$.hard_break,
				$.soft_hyphen_escape,
				$.escaped_char,
				$.loose_backslash,
				$.soft_break,
				$.emph_text,
			),

		_strong_unit: $ =>
			choice(
				$.strong_emphasis,
				$.emphasis,
				$.code_span,
				$.inline_math,
				$.citation,
				$.reference,
				$.linebreak_call,
				$.inline_call,
				$.hard_break,
				$.soft_hyphen_escape,
				$.escaped_char,
				$.loose_backslash,
				$.soft_break,
				$.emph_text,
			),

		_strong_emphasis_unit: $ =>
			choice(
				$.strong,
				$.emphasis,
				$.code_span,
				$.inline_math,
				$.citation,
				$.reference,
				$.linebreak_call,
				$.inline_call,
				$.hard_break,
				$.soft_hyphen_escape,
				$.escaped_char,
				$.loose_backslash,
				$.soft_break,
				$.emph_text,
			),

		emph_text: _ => token(prec(-1, /[^*\\\n\r]+/)),

		code_span: $ =>
			seq(
				'`',
				repeat(choice($.code_text, $.code_escape, $.soft_break)),
				'`',
			),

		code_text: _ => token(prec(-1, /[^`\n\r\\]+/)),

		code_escape: _ => token(seq('\\', /[^\r\n]/)),

		inline_math: $ =>
			seq(
				'$',
				repeat(choice($.math_text, $.math_escape)),
				'$',
			),

		math_text: _ => token(prec(-1, /[^$\\\n\r]+/)),

		math_escape: _ => token(seq('\\', /[^\r\n]/)),

		citation: $ => seq('[', '@', field('target', $.label_name), ']'),

		reference: $ => seq('@', field('target', $.label_name)),

		label: $ => seq('<', field('name', $.label_name), '>'),

		label_name: _ => token(/[A-Za-z_][A-Za-z0-9_-]*(:[A-Za-z_][A-Za-z0-9_-]*)*/),

		// Hard line break: `\\` inside inline text. Compiler lowers this to
		// `InlineKind::HardBreak` (see `mos-parse/src/inline.rs`).
		hard_break: _ => token('\\\\'),

		// Soft hyphen shorthand: `\-` inside inline text. Compiler expands
		// this to the U+00AD soft hyphen codepoint (see `mos-parse/src/inline.rs`).
		soft_hyphen_escape: _ => token('\\-'),

		// Generic inline escape `\X` for any other character (e.g. `\#`,
		// `\*`, `\[`, `\]`, `\<`). `\` and `-` are excluded so the dedicated
		// `hard_break` and `soft_hyphen_escape` tokens win the lexer's
		// longest-match race. The compiler leaves unrecognised backslashes
		// literal (the `\` stays in the text run; no diagnostic) rather
		// than stripping them, so editor and compiler agree on byte content
		// for these forms (see `mos-parse/src/inline.rs`).
		escaped_char: _ => token(seq('\\', /[^\\\-\r\n]/)),

		// A bare `\` that does not form one of the recognised 2-char escape
		// tokens above: typically `\` at end of input or immediately before
		// a newline. The compiler treats this as literal text and emits
		// diagnostic `W025` for the trailing-newline case; surfacing it as
		// a discrete node lets editors distinguish "lone backslash" from a
		// structural parse error. Length-1, so `hard_break` (`\\`),
		// `soft_hyphen_escape` (`\-`), and `escaped_char` (`\X`) always win
		// the longest-match race whenever any of them apply.
		loose_backslash: _ => token('\\'),

		// Tree-sitter pragmatic deviation from EBNF `text_char`: also exclude
		// `[` and `]` so bracket-delimited structures (`content_body`,`array`)
		// parse without ambiguity. Literal brackets in prose can be written via `\[` / `\]`.
		// oxlint-disable-next-line no-useless-escape
		text: _ => token(prec(-2, /[^\n\r#$*`@<\\\[\]]+/)),

		// -------------------------------------------------------------------
		// Calls and content bodies
		// -------------------------------------------------------------------

		hash_call: $ =>
			prec.right(
				PREC.hash_call,
				seq(
					'#',
					field('function', $.qualified_name),
					optional(field('arguments', $.argument_list)),
					optional(field('body', $.content_body)),
				),
			),

		linebreak_call: $ =>
			prec.right(
				PREC.linebreak_call,
				seq(
					'#linebreak',
					optional(field('arguments', $.argument_list)),
				),
			),

		block_call: $ =>
			prec.dynamic(
				1,
				prec.right(seq(
					$.hash_call,
					optional(field('label', $.block_label)),
					optional($._line_end),
				)),
			),

		content_body: $ =>
			seq(
				'[',
				repeat(choice($.blank_line, $.section, $._block, $._line_end)),
				']',
			),

		// -------------------------------------------------------------------
		// Expressions
		// -------------------------------------------------------------------

		_expression: $ =>
			choice(
				$.string,
				$.dimension,
				$.number,
				$.boolean,
				$.null,
				$.array,
				$.object,
				$.call_expr,
				$.qualified_name,
			),

		call_expr: $ =>
			prec.right(seq(
				field('function', $.qualified_name),
				field('arguments', $.argument_list),
			)),

		argument_list: $ =>
			seq(
				'(',
				optional($._nl),
				optional(seq(
					$._argument,
					repeat(seq(
						optional($._nl),
						',',
						optional($._nl),
						$._argument,
					)),
					optional(seq(optional($._nl), ',')),
				)),
				optional($._nl),
				')',
			),

		_argument: $ => choice($.attribute, $._expression),

		attribute: $ =>
			prec(
				PREC.attribute,
				seq(
					field('key', $.identifier),
					':',
					optional($._nl),
					field('value', $._expression),
				),
			),

		array: $ =>
			seq(
				'[',
				optional($._nl),
				optional(seq(
					$._expression,
					repeat(seq(
						optional($._nl),
						',',
						optional($._nl),
						$._expression,
					)),
					optional(seq(optional($._nl), ',')),
				)),
				optional($._nl),
				']',
			),

		object: $ =>
			seq(
				'{',
				optional($._nl),
				optional(seq(
					$.attribute,
					repeat(seq(
						optional($._nl),
						',',
						optional($._nl),
						$.attribute,
					)),
					optional(seq(optional($._nl), ',')),
				)),
				optional($._nl),
				'}',
			),

		qualified_name: $ => prec.right(seq($.identifier, repeat(seq('.', $.identifier)))),

		// -------------------------------------------------------------------
		// Literals
		// -------------------------------------------------------------------

		string: $ =>
			choice(
				seq(
					'"',
					repeat(choice($.string_double_content, $.escape_sequence)),
					token.immediate('"'),
				),
				seq(
					"'",
					repeat(choice($.string_single_content, $.escape_sequence)),
					token.immediate("'"),
				),
			),

		string_double_content: _ => token.immediate(prec(1, /[^"\\\n\r]+/)),

		string_single_content: _ => token.immediate(prec(1, /[^'\\\n\r]+/)),

		escape_sequence: _ => token.immediate(seq('\\', /./)),

		dimension: _ =>
			token(seq(
				/[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)/,
				/pt|mm|cm|in|px|em|rem|ch|fr|%/,
			)),

		number: _ => token(/[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)/),

		boolean: _ => choice('true', 'false'),

		null: _ => 'null',

		identifier: _ => token(/[A-Za-z_][A-Za-z0-9_-]*/),
	},
});
