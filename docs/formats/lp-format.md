<!--
  Generated from spec/v1 by scripts/gen-docs.mjs. Do not edit.
  The assets are the specification; this page is a rendering of them.
-->
# LP-format acceptance v1

The accepted LP-format subset: sections, statements, terms, bounds, and the reserved words.

## `[shape]`

- **encoding** — ASCII with line-feed line endings, as every accepted input is. Anything else is refused by the byte-level shape check before a line is examined.
- **comment** — A backslash begins a comment that runs to the end of the line. A comment may follow content on the same line, and the characters it removes are removed before the line is read as anything else.
- **blank** — A line that is empty after comment removal and trimming is ignored and does not terminate a statement.
- **case** — Section keywords and reserved words are matched without regard to case. Names are matched exactly. A file that spells a keyword one way and a name another is not relying on a reader preference; a name that differs from another only in case is two names.

## `[statements]`

- **rule** — Every objective and every constraint carries a label. A line whose first token is a name followed immediately by a colon begins a new statement; any other line inside those two sections continues the statement before it.
- **excluded** — The common alternative is to guess: start a new constraint at a leading sign, or at a line that contains a relational operator. Both readings split a wrapped row differently, and both do it silently.
- **required_label** — A constraint without a label is refused. Requiring the label is what makes the continuation rule total, and it costs only the files that were relying on a generated name nobody wrote down.
- **colon** — The colon follows the name with no space between them. A space there would make a label indistinguishable from a first term under the continuation rule.
- **sections** — A line whose first token is a section keyword ends the statement before it and opens that section. Reserved words are not names, so this can never be mistaken for a continuation.
- **elsewhere** — In the bound and integrality sections there is no continuation. A bound statement occupies one line; an integrality section lists names separated by whitespace across as many lines as it likes, because a list of names has nothing to continue.

## `[sections]`

- **order** — The objective section, then the constraint section, then the bound section, then the two integrality sections, then the terminator. Each may appear at most once and the relative order is fixed.
- **required** — The objective section, the constraint section, and the terminator are required. A file that stops before the terminator is refused as truncated rather than read as complete.
- **unknown** — Any other section keyword is refused, the semicontinuous and special-ordered-set sections included. Their absence is a stated non-goal.
- **minimize** —
  - MINIMIZE
  - MINIMISE
  - MIN
  - MINIMUM
- **maximize** —
  - MAXIMIZE
  - MAXIMISE
  - MAX
  - MAXIMUM
- **subject_to** —
  - SUBJECT TO
  - SUCH THAT
  - ST
  - S.T.
  - ST.
- **bounds** —
  - BOUNDS
  - BOUND
- **general** —
  - GENERAL
  - GENERALS
  - GEN
- **binary** —
  - BINARY
  - BINARIES
  - BIN
- **end** —
  - END
- **spacing** — A two-word keyword may carry any run of blanks between its words. It is matched after that run is collapsed to a single blank, so the two spellings are one keyword and not two.

## `[section.objective]`

- **sense** — The keyword chooses the direction. A maximizing objective has every coefficient and its constant negated at the parse boundary, exactly as the MPS objective sense does.
- **label** — The objective carries a label like any other statement. The canonical model has no objective row, so the label is read to satisfy the statement rule and then discarded.
- **empty** — An objective with a label and no terms is accepted and means the zero objective. The MPS reader refuses a file with no objective row because nothing in that file states a direction; here the section and its keyword are present, so the emptiness is written down rather than inferred.
- **constant** — A bare number among the terms is the objective constant. Several of them sum. This is the one place a term may carry no name.

## `[section.constraints]`

- **form** — A label, then either a linear form with one relational operator and one number, or a number, an operator, a linear form, the same operator, and a number.
- **operators** —
  - <=
  - >=
  - =
  - <
  - >
- **loose** — The one-character spellings are read as their two-character forms. LP files use them interchangeably and no reader has ever meant a strict inequality by them; a strict inequality over the rationals has no closed feasible set to prove anything about.
- **double_sided** — In the two-sided form both operators must point the same way and neither may be the equality. The bounds are assigned from the direction, so a file mixing directions is refused rather than reordered.
- **mapping** — expr <= u gives the row an upper bound only. expr >= l gives it a lower bound only. expr = v gives both, equal. l <= expr <= u gives both, and u >= expr >= l gives the same row.
- **crossing** — A row whose two sides cross is a contradictory model, not a malformed file. It is built and handed on, and the presolve chain is what produces the contradiction.
- **constant_left** — A bare number inside the linear form of a constraint is refused. Moving it to the other side is arithmetic the reader would be doing on behalf of an author who did not write it, and the objective section is the only place a constant belongs.
- **empty_form** — A constraint whose linear form has no terms is refused. It states a fact about no variables, and the two readings, a trivially true row and a mistake, are not distinguishable from the file.

## `[section.bounds]`

- **forms** —
  - name >= value
  - name <= value
  - name = value
  - lower <= name <= upper
  - upper >= name >= lower
  - name FREE
- **one_per_line** — One bound statement per line. There is no continuation here and no separator, so two on a line would need one.
- **default_lower** — Every variable starts with a lower bound of zero and no upper bound. A bound statement replaces the side it names and leaves the other alone.
- **free** — The word FREE removes both sides. It is the only way to say that in this subset.
- **infinite** —
  - -INF
  - -INFINITY
  - +INF
  - +INFINITY
- **infinite_rule** — An infinity spelling is accepted only as a bound endpoint, where it means that side is absent. It is a reserved word and not a number: everywhere a number is required, including a coefficient and a right-hand side, it is refused as a malformed numeric token. The sign is required, because a bound endpoint that does not say which side it is on has to be worked out from the operator.
- **negative_upper** — An upper bound below the default lower bound of zero, on a variable whose lower bound has not been given, is refused. Readers disagree about whether it also opens the lower side, and the two readings differ on which points are feasible. Give the lower bound first and the file is accepted.
- **repeats** — Two statements setting the same side of one variable are refused. Setting the two sides separately is ordinary.
- **undeclared** — A bound naming a variable that no objective or constraint term declared is refused. Variables are declared by use here; a file whose only mention of a variable is a bound is stating something about a column that is not in the model.

## `[section.integrality]`

- **general** — Names listed under the general section are integer, and their bounds are whatever the bound section said, or the defaults.
- **binary** — Names listed under the binary section are integer with bounds of zero and one. The bounds are set by the section and a bound statement for the same variable is refused as a repeat, because the two are the same claim written twice and a reader would have to decide which wins.
- **both** — A name in both integrality sections is refused as a duplicate.
- **undeclared** — A name that no term declared is refused, for the reason the bound section gives.
- **separators** — Names are separated by whitespace and may run across lines. A comma between them is refused: it appears in no LP file this subset accepts and admitting it would make an empty element possible.

## `[terms]`

- **form** — An optional sign, then an optional number, then a name. A term with no number has a coefficient of one. A term with no sign after the first is refused: the terms of a linear form are joined by their signs, and a missing one is a missing operator.
- **first_sign** — The sign on the first term of a form is optional and defaults to positive.
- **juxtaposition** — The number and the name may be adjacent or separated by blanks. A number cannot end with a character a name can start with, so the split is never in doubt.
- **double_sign** — Two signs in a row are refused. A minus followed by a negative coefficient is arithmetic the file is asking the reader to perform.
- **duplicates** — A variable appearing more than once in one linear form has its coefficients summed exactly, as duplicate MPS entries do. A sum to zero is recorded in the canonical dump as a cancellation rather than as an absence.
- **declaration** — A variable is declared by its first appearance in a term, and variables are ordered by first appearance: the objective first, then the constraints in file order.

## `[names]`

- **pattern** — A letter or an underscore, then any run of letters, digits, underscores, and full stops.
- **length** — Names are capped at the same length every accepted format caps them at.
- **reserved** — A reserved word is never a name. The section keywords, the word FREE, and the four infinity spellings are reserved, matched without regard to case, and a file using one as a variable label is refused.
- **duplicate_row** — Two constraints sharing a label are refused, which is the same rule the MPS reader applies to two rows sharing a name.

## `[numbers]`

- **grammar** — The same numeric grammar every accepted format uses: an optional sign, digits with an optional fractional part, and an optional decimal exponent. Not-a-number and infinity spellings are refused here, whatever they mean in a bound position.
- **exactness** — A number becomes a rational directly from its characters, with no intermediate binary floating-point value, and is subject to the same token length, projected bit width, and round-trip fences as every other accepted format.

## `[refusals]`

- **grammar** — HC-INSTANCE-27 covers a token that does not fit the grammar, a statement whose shape is not one of the accepted forms, a missing label, a reserved word used as a name, and a term or bound that is written in a way this subset excludes.
- **shared** — A byte that is not ASCII text is HC-INSTANCE-29. An unknown section keyword is HC-INSTANCE-08 and a repeated or misordered one is HC-INSTANCE-09. A duplicate constraint label is HC-INSTANCE-14 and a duplicate integrality listing is HC-INSTANCE-15. A bound or integrality entry naming an undeclared variable is HC-INSTANCE-13. A repeated bound on one side is HC-INSTANCE-24. A negative upper bound against the default lower bound is HC-INSTANCE-19. Input that ends before the terminator is HC-INSTANCE-26. An overlong name is HC-INSTANCE-03, and the numeric fences are HC-INSTANCE-04 through HC-INSTANCE-07.
- **traps** —
  - trap_lp_statement_continuation
  - trap_lp_default_bounds
  - trap_lp_double_sided_row
  - trap_lp_reserved_word_as_name
