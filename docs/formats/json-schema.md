<!--
  Generated from spec/v1 by scripts/gen-docs.mjs. Do not edit.
  The assets are the specification; this page is a rendering of them.
-->
# JSON instance schema v1

The accepted JSON document: its members, its restricted JSON grammar, and the rule that no model value is a number.

## `[grammar]`

- **subset** — A restricted JSON: objects, arrays, strings, integers, booleans, and null. There are no floating-point literals anywhere in the accepted grammar, so a reader of this schema never needs a decimal-to-binary conversion and can never silently perform one.
- **encoding** — ASCII with line-feed line endings, as every accepted input is.
- **integers** — An integer literal is an optional minus sign and digits with no leading zero, and it is accepted only where the schema calls for a count or an index. It is never a model value.
- **strings** — A string is double quoted. The accepted escapes are the backslash, the double quote, the forward slash, the backspace, the form feed, the line feed, the carriage return, and the tab. A four-digit unicode escape is refused: every accepted input is ASCII, so an escape is either redundant or names something outside the accepted set.
- **whitespace** — Blanks, tabs, line feeds, and carriage returns separate tokens and mean nothing else. There are no comments.
- **trailing** — A trailing comma is refused. Content after the closing brace, other than whitespace, is refused.
- **duplicates** — A repeated member name in one object is refused rather than resolved by position.
- **unknown** — A member the schema does not name is refused. Ignoring what is not understood is how a model acquires a meaning nobody wrote.
- **optional** — There are no optional members. Where absence is meaningful it is spelled null, and a member that is simply missing is refused, because a default is a second reading of the same document.
- **depth** — The nesting the schema describes is the nesting accepted; the reader has a fixed depth and does not recurse on input.

## `[document]`

- **members** —
  - version
  - sense
  - objective_constant
  - vars
  - rows
  - nonzeros
- **version** — The integer 1. A document naming another version is refused rather than read hopefully.
- **sense** — The string min or the string max. A maximizing document has every objective coefficient and the objective constant negated at the parse boundary, exactly as the other two formats do.
- **objective_constant** — A decimal string. The appendix sketch of this schema had no member for it, and a serialization that cannot carry the objective constant is not a serialization of the canonical model; the addition is recorded in a decision record.
- **vars** — An array of variable objects, in the order the model declares them. Empty is refused: a model with no columns has nothing to solve and nothing to prove.
- **rows** — An array of row objects, in the order the model declares them. Empty is accepted; a model constrained only by its bounds is an ordinary box.
- **nonzeros** — An array of coefficient entries.

## `[member.var]`

- **members** —
  - name
  - lb
  - ub
  - integer
  - obj
- **name** — A string label. It is a label and never an identifier; positions in the array are the identifiers.
- **lb** — A decimal string, or null for unbounded below.
- **ub** — A decimal string, or null for unbounded above.
- **integer** — A boolean. There is no integrality section and no marker record; a column says whether it is integer.
- **obj** — A decimal string, the objective coefficient in the direction the document states.
- **duplicate_name** — Two variables sharing a name are refused.

## `[member.row]`

- **members** —
  - name
  - sense
  - rhs
  - range
- **name** — A string label, and a duplicate is refused.
- **sense** —
  - L
  - G
  - E
- **sense_note** — There is no free row and no objective row. The objective lives on the columns, so an N row would be a row with no meaning, and accepting one would make the position of the objective depend on which row happened to come first.
- **rhs** — A decimal string. It is required even where it is zero, because an omitted right-hand side is a default and this schema has none.
- **range** — A decimal string, or null. When present it widens the row exactly as an MPS RANGES entry does, by the same table, so that the two formats cannot disagree about a model one of them round-trips into the other.

## `[member.nonzero]`

- **form** — An array of exactly three elements: the row index, the column index, and the coefficient as a decimal string.
- **indices** — Integers into the rows and vars arrays. A name here would need a lookup and would give a document two ways to say the same thing; an index out of range is refused.
- **order** — Any order is accepted. Entries naming one position sum exactly, as duplicate MPS entries do, and a sum to zero is recorded in the canonical dump as a cancellation rather than as an absence.
- **value** — A decimal string, read by the same grammar and held to the same fences as every other accepted format.

## `[refusals]`

- **schema** — HC-INSTANCE-28 covers a document that is not the accepted JSON, a member that is missing, unknown, repeated, or of the wrong kind, a version that is not one, a sense or a row sense outside its list, an index out of range, and a model value written as a JSON number instead of a string.
- **shared** — A byte that is not ASCII text is HC-INSTANCE-29. A duplicate row label is HC-INSTANCE-14 and a duplicate variable label is HC-INSTANCE-15. An overlong name is HC-INSTANCE-03, and the numeric fences on every decimal string are HC-INSTANCE-04 through HC-INSTANCE-07.
- **traps** —
  - trap_json_value_is_never_a_number
  - trap_json_unknown_member
  - trap_json_range_matches_mps
