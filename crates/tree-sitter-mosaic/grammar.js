/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/**
 * Tree-sitter 0.26-style ESM grammar for Mosaic `.mos` documents.
 *
 * @file Mosaic grammar for Tree-sitter
 * @author Kaj Kowalski <info@kajkowalski.nl>
 * @license MIT
 */

const PREC = {
	call: 10,
	attribute: 9,
	inline: 5,
	emphasis: 4,
	text: 1,
};

const WS = /[\t\f\r ]+/;
const NEWLINE = /\n/;
const BLANK_LINE = /\n[\t\f\r ]*\n+/;
const LINE_COMMENT = /[^\n]*/;

const PARAGRAPH_TEXT = /[^\n#$*`@<\\\u005B]+/;
const INLINE_TEXT = /[^\n#$*`@<\\\u005D)]+/;
const EMPHASIS_TEXT = /[^*\n\\]+|\\./;
const CONTENT_TEXT = /[^\]\n#$*`@<\\]+|\\./;

export default grammar({
	name: 'mosaic',

	extras: $ => [
		WS,
		$.comment,
	],

	word: $ => $.identifier,

	supertypes: $ => [
		$.block,
		$.inline,
		$.expression,
		$.value,
	],

	conflicts: $ => [
		[$.block_call, $.inline],
		[$.heading, $.inline],
		[$.emphasis],
		[$.strong],
		[$.strong_emphasis],
		[$.expression, $.value],
		[$.paragraph, $.content_block],
	],

	rules: {
		source_file: $ => repeat($._document_item),

		_document_item: $ =>
			choice(
				$.blank_line,
				$.block,
				$.newline,
			),

		// ---------------------------------------------------------------------
		// Lines and comments
		// ---------------------------------------------------------------------

		newline: _ => NEWLINE,

		blank_line: _ => token(BLANK_LINE),

		comment: _ =>
			token(choice(
				seq('//', LINE_COMMENT),
				seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
			)),

		// ---------------------------------------------------------------------
		// Blocks
		// ---------------------------------------------------------------------

		block: $ =>
			choice(
				$.set_directive,
				$.import_directive,
				$.include_directive,
				$.heading,
				$.block_call,
				$.display_math,
				$.paragraph,
			),

		set_directive: $ =>
			seq(
				'#set',
				field('target', $.identifier),
				field('arguments', $.argument_list),
			),

		import_directive: $ =>
			seq(
				'#import',
				field('path', $.string),
				optional(seq(':', commaSep1($.identifier))),
			),

		include_directive: $ =>
			seq(
				'#include',
				field('path', $.string),
			),

		heading: $ =>
			prec.right(seq(
				field('marker', $.heading_marker),
				field('content', optional($.inline_sequence)),
				field('label', optional($.label)),
			)),

		heading_marker: _ => token(seq(/={1,6}/, /[\t ]+/)),

		paragraph: $ =>
			prec.right(repeat1(
				field(
					'content',
					choice(
						$.inline,
						alias($._paragraph_text, $.text),
					),
				),
			)),

		_paragraph_text: _ => token(prec(PREC.text, PARAGRAPH_TEXT)),

		block_call: $ =>
			prec.dynamic(
				1,
				seq(
					$.call,
					optional($.label),
				),
			),

		display_math: $ =>
			prec.right(seq(
				'$$',
				field('content', repeat(choice($.math_text, $.math_escape))),
				'$$',
				optional($.label),
			)),

		// ---------------------------------------------------------------------
		// Inline content
		// ---------------------------------------------------------------------

		inline_sequence: $ =>
			prec.right(repeat1(choice(
				$.inline,
				alias($._inline_text, $.text),
			))),

		inline: $ =>
			choice(
				$.strong_emphasis,
				$.strong,
				$.emphasis,
				$.code_span,
				$.inline_math,
				$.reference,
				$.label,
				$.escape,
				$.linebreak_escape,
				$.call,
			),

		_inline_text: _ => token(prec(PREC.text, INLINE_TEXT)),

		emphasis: $ =>
			prec.dynamic(
				1,
				seq(
					'*',
					field(
						'content',
						repeat1(choice(
							$.inline,
							alias($._emphasis_text, $.text),
						)),
					),
					'*',
				),
			),

		strong: $ =>
			prec.dynamic(
				2,
				seq(
					'**',
					field(
						'content',
						repeat1(choice(
							$.inline,
							alias($._strong_text, $.text),
						)),
					),
					'**',
				),
			),

		strong_emphasis: $ =>
			prec.dynamic(
				3,
				seq(
					'***',
					field(
						'content',
						repeat1(choice(
							$.inline,
							alias($._strong_emphasis_text, $.text),
						)),
					),
					'***',
				),
			),

		_emphasis_text: _ => token(prec(PREC.text, EMPHASIS_TEXT)),
		_strong_text: _ => token(prec(PREC.text, EMPHASIS_TEXT)),
		_strong_emphasis_text: _ => token(prec(PREC.text, EMPHASIS_TEXT)),

		code_span: _ =>
			token(seq(
				'`',
				repeat(choice(/[^`\\\n]+/, /\\./)),
				'`',
			)),

		inline_math: $ =>
			seq(
				'$',
				field('content', repeat(choice($.math_text, $.math_escape))),
				'$',
			),

		math_text: _ => token(prec(PREC.text, /[^$\\]+/)),

		math_escape: _ => token(seq('\\', /./)),

		reference: $ =>
			seq(
				'@',
				field('target', $.label_name),
			),

		label: $ =>
			seq(
				'<',
				field('name', $.label_name),
				'>',
			),

		label_name: _ => token(/[A-Za-z][A-Za-z0-9_-]*(?::[A-Za-z0-9_-]+)*/),

		escape: _ => token(seq('\\', /./)),

		linebreak_escape: _ => token(seq('\\', /[\t ]*\n/)),

		// ---------------------------------------------------------------------
		// Calls and expressions
		// ---------------------------------------------------------------------

		call: $ =>
			prec(
				PREC.call,
				seq(
					'#',
					field('function', $.identifier),
					optional(field('arguments', $.argument_list)),
					optional(field('body', $.content_block)),
				),
			),

		argument_list: $ =>
			seq(
				'(',
				optional(commaSep1(choice($.attribute, $.expression))),
				optional(','),
				')',
			),

		content_block: $ =>
			seq(
				'[',
				repeat(choice(
					$.block,
					$.inline,
					alias($._content_text, $.text),
					$.newline,
				)),
				']',
			),

		_content_text: _ => token(prec(PREC.text, CONTENT_TEXT)),

		attribute: $ =>
			prec(
				PREC.attribute,
				seq(
					field('key', $.identifier),
					':',
					field('value', $.value),
				),
			),

		expression: $ =>
			choice(
				$.value,
				$.array,
				$.object,
				$.call,
				$.identifier,
			),

		value: $ =>
			choice(
				$.string,
				$.dimension,
				$.number,
				$.boolean,
				$.null,
				$.identifier,
			),

		array: $ =>
			seq(
				'[',
				optional(commaSep1($.expression)),
				optional(','),
				']',
			),

		object: $ =>
			seq(
				'{',
				optional(commaSep1($.attribute)),
				optional(','),
				'}',
			),

		string: _ =>
			token(choice(
				seq('"', repeat(choice(/[^"\\]+/, /\\./)), '"'),
				seq("'", repeat(choice(/[^'\\]+/, /\\./)), "'"),
			)),

		dimension: _ =>
			token(seq(
				/[+-]?([0-9]+(\.[0-9]+)?|\.[0-9]+)/,
				/pt|mm|cm|in|px|em|rem|ch|fr|%/,
			)),

		number: _ => token(/[+-]?([0-9]+(\.[0-9]+)?|\.[0-9]+)/),

		boolean: _ => choice('true', 'false'),

		null: _ => 'null',

		identifier: _ => token(/[A-Za-z_][A-Za-z0-9_-]*/),
	},
});

/**
 * @param {RuleOrLiteral} rule
 * @returns {SeqRule}
 */
function commaSep1(rule) {
	return seq(rule, repeat(seq(',', rule)));
}
