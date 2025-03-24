<!--
  Generated from spec/v1 by scripts/gen-docs.mjs. Do not edit.
  The assets are the specification; this page is a rendering of them.
-->
# Rejection identifiers v1

Every rejection identifier, the group it belongs to, and the earliest check that emits it.

## `[policy]`

- **earliest** — Each identifier names exactly one check, and that check is the earliest point at which the defect is detectable. A tamper case that could plausibly be caught by two checks must be caught by the earlier one, so the corpus pins the order of the checks as well as their existence.
- **exit_code** — hc verify exits 0 on acceptance and 1 on rejection, printing the identifier, the offending record, and the deterministic work units consumed.
- **grouping** — The HCC groups cover the certificate. The instance group covers the original file, and exists because the two independent parsers must be able to disagree comparably: a differential test that sees one parser accept and the other reject needs to know whether they rejected for the same reason.

## HC-INSTANCE

| Identifier | Message | Earliest check |
|---|---|---|
| `HC-INSTANCE-01` | instance file exceeds the maximum accepted size | File length against the parser byte cap, before reading. |
| `HC-INSTANCE-02` | instance exceeds the maximum accepted nonzero count | Running nonzero count during COLUMNS, before each insertion. |
| `HC-INSTANCE-03` | name exceeds the maximum length or contains a line feed | Name length and content at the point the name is read. |
| `HC-INSTANCE-04` | numeric token exceeds the maximum token length | Numeric token length, before the token is interpreted. |
| `HC-INSTANCE-05` | numeric token projects a rational exceeding the bit cap | Projected bit length computed from the token, before any limb is allocated. |
| `HC-INSTANCE-06` | exact nonzero value does not round to a nonzero finite binary64 | Binary64 round-trip of each exact nonzero model value, after construction. |
| `HC-INSTANCE-07` | numeric token is malformed or spells a not-a-number or infinity | Numeric token syntax. |
| `HC-INSTANCE-08` | unknown section header | Section header against the accepted list. |
| `HC-INSTANCE-09` | section appears out of order or more than once | Section order and repetition, at the header. |
| `HC-INSTANCE-10` | a required section is missing | Presence of required sections, at end of parse. |
| `HC-INSTANCE-11` | no objective row was declared | Objective selection, at the end of ROWS. |
| `HC-INSTANCE-12` | record references an undeclared row | Row name lookup, at the referencing record. |
| `HC-INSTANCE-13` | record references an undeclared column | Column name lookup, at the referencing record. |
| `HC-INSTANCE-14` | duplicate row name | Row name insertion in ROWS. |
| `HC-INSTANCE-15` | duplicate column name | Column name insertion in COLUMNS. |
| `HC-INSTANCE-16` | multiple named vectors in one section are not supported | Vector name of the second distinct RHS, RANGES, or BOUNDS vector. |
| `HC-INSTANCE-17` | unsupported bound type | Bound type against the accepted list. |
| `HC-INSTANCE-18` | integer bound value is not an exact integer | Integrality of the value on an LI or UI record. |
| `HC-INSTANCE-19` | negative upper bound on a column with the default lower bound is ambiguous | Sign of a UP value against the current lower bound, in strict mode. |
| `HC-INSTANCE-20` | field content overruns its fixed-form column window | Field extent in fixed form, at the point the field is read. |
| `HC-INSTANCE-21` | unbalanced or nested INTORG and INTEND markers | Marker balance, at each marker record and at the end of COLUMNS. |
| `HC-INSTANCE-22` | column records are not contiguous | Column identity against the current column, at each COLUMNS record. |
| `HC-INSTANCE-23` | RANGES entry names the objective row or a free row | Row sense of the target, at each RANGES record. |
| `HC-INSTANCE-24` | duplicate bound of the same type on one column | Bound type and column against previously applied bounds. |
| `HC-INSTANCE-25` | record carries more fields than its type accepts | Field count in free form, at each record. |
| `HC-INSTANCE-26` | unexpected end of input before ENDATA | End of input before the terminating section. |
| `HC-INSTANCE-27` | LP-format input does not match the accepted grammar | LP-format grammar, at the offending token. |
| `HC-INSTANCE-28` | JSON instance does not match the accepted schema | JSON instance schema, at the offending member. |

## HCC-PARSE

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-PARSE-01` | certificate magic is not HCC1 | First four bytes of the stream. |
| `HCC-PARSE-02` | unsupported HCC or canonicalization version | Header version fields. |
| `HCC-PARSE-03` | certificate is not bound to this instance | Header digest against the digest recomputed from the supplied instance. |
| `HCC-PARSE-04` | declared certificate size exceeds a cap | Declared record count and byte length against the caps, before any allocation. |
| `HCC-PARSE-05` | non-minimal LEB128 encoding | LEB128 decoding, at the value. |
| `HCC-PARSE-06` | LEB128 value does not fit in 64 bits | LEB128 decoding, at the value. |
| `HCC-PARSE-07` | certificate stream is truncated | Available bytes against the record length, before reading the payload. |
| `HCC-PARSE-08` | trailing bytes follow the End record | Remaining bytes after the End record. |
| `HCC-PARSE-09` | CRC-32C checksum mismatch | Checksum over the covered range, at the End record. |
| `HCC-PARSE-10` | unknown record type | Record type against the closed list. |
| `HCC-PARSE-11` | record payload length disagrees with its declared length | Bytes consumed by the payload against the declared record length. |
| `HCC-PARSE-12` | record identifiers are not strictly increasing | Record identifier against the previous identifier. |
| `HCC-PARSE-13` | rational text is not in canonical form | Rational text syntax, at the token. |
| `HCC-PARSE-14` | rational exceeds the bit cap | Bit length of a parsed rational, against the cap. |
| `HCC-PARSE-15` | rational token exceeds the maximum token length | Declared token length, before reading the rational text. |
| `HCC-PARSE-16` | set-valued array is unsorted or contains a repeat | Element order within an array whose semantics are a set. |
| `HCC-PARSE-17` | End record counts disagree with the stream | End record counts against the records actually read. |
| `HCC-PARSE-18` | declared tree depth exceeds the cap | Declared tree depth against the hard depth cap, before replay. |
| `HCC-PARSE-19` | record count exceeds the cap | Running record count against the cap, during the stream. |
| `HCC-PARSE-20` | certificate byte length exceeds the cap | Running byte count against the cap, during the stream. |

## HCC-TRANSFORM

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-TRANSFORM-01` | unknown transform rule | Transform rule identifier against the closed list. |
| `HCC-TRANSFORM-02` | transform is not applicable to the recomputed model | Applicability condition, recomputed on the model the verifier holds. |
| `HCC-TRANSFORM-03` | recomputed model does not match the hash the transform states | Canonical hash of the recomputed result against the record. |
| `HCC-TRANSFORM-04` | forward map does not preserve feasibility or objective direction | Forward map on the witness points carried by the certificate. |
| `HCC-TRANSFORM-05` | reverse map does not restore feasibility with equal objective | Reverse map on the witness points carried by the certificate. |
| `HCC-TRANSFORM-06` | transform contradiction does not follow from the recomputed model | Contradiction derivation, recomputed from the cited row and bounds. |
| `HCC-TRANSFORM-07` | transform record appears after the root | Record position relative to the Root record. |
| `HCC-TRANSFORM-08` | integer bound was rounded in the wrong direction | Rounding direction on an integer bound derived by the transform. |
| `HCC-TRANSFORM-09` | transform chain does not produce the reduced model the root names | Final chain model hash against the Root reduced model hash. |

## HCC-SCOPE

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-SCOPE-01` | cited inequality is not visible at this node | Visibility of each cited inequality at the citing node. |
| `HCC-SCOPE-02` | record cites an identifier that does not precede it | Cited identifier against the citing identifier. |
| `HCC-SCOPE-03` | cut is cited outside the subtree it was derived in | Ancestry of the citing node against the cut scope node. |
| `HCC-SCOPE-04` | cut scope node is neither the derivation node nor an ancestor of it | Cut scope node against the derivation node. |
| `HCC-SCOPE-05` | tableau combination uses a row not visible at the derivation node | Visibility of each row entering the tableau combination. |

## HCC-DUAL

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-DUAL-01` | inference uses a negative multiplier | Sign of each multiplier, before summation. |
| `HCC-DUAL-02` | inference carries a zero multiplier, which is not canonical | Each multiplier against zero, before summation. |
| `HCC-DUAL-03` | claimed bound does not equal its nonnegative inequality combination | Recomputed bound against the bound the record states. |
| `HCC-DUAL-04` | multiplied coefficients do not sum to the objective vector | Summed coefficient vector against the reduced objective vector. |
| `HCC-DUAL-05` | contradiction coefficients do not sum to zero | Summed coefficient vector against zero, for a contradiction. |
| `HCC-DUAL-06` | contradiction right-hand side sum is not strictly positive | Summed right-hand side, for a contradiction. |
| `HCC-DUAL-07` | inference cites an inequality identifier that does not exist | Existence of each cited inequality identifier. |

## HCC-CUT

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-CUT-01` | combination does not isolate exactly one integer basic variable | Reconstructed combination, before applying the formula. |
| `HCC-CUT-02` | nonbasic variable sits at no finite bound | Bound state of each nonbasic in the reconstructed equation. |
| `HCC-CUT-03` | integer displacement is measured from a non-integral endpoint | Integrality of the endpoint each integer displacement is measured from. |
| `HCC-CUT-04` | fractional part is not strictly between zero and one | Fractional part of the right-hand side of the reconstructed equation. |
| `HCC-CUT-05` | recomputed Gomory coefficient differs from the coefficient stated | Recomputed cut coefficients against the serialized ones. |
| `HCC-CUT-06` | cover source row is not a positive-integer binary knapsack | Source row weights, capacity, and variable types. |
| `HCC-CUT-07` | cover weight sum does not strictly exceed the capacity | Sum of cover weights against the capacity. |
| `HCC-CUT-08` | cover source row contains a variable that is not binary | Variable types in the source row. |
| `HCC-CUT-09` | lifting has no feasible state, so the variable should have been fixed | Feasible states in the lifting dynamic program. |
| `HCC-CUT-10` | lifting coefficient is negative or not an integer | Sign and integrality of each recomputed lifting coefficient. |
| `HCC-CUT-11` | recomputed lifting coefficient differs from the coefficient stated | Recomputed lifting coefficients against the serialized ones. |
| `HCC-CUT-12` | lifting exceeds the dynamic-program caps, so only an unlifted cover is admissible | Dynamic-program size against the caps, before running it. |
| `HCC-CUT-13` | unknown cut family | Cut family identifier against the closed list. |

## HCC-TREE

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-TREE-01` | branch variable is not declared integer | Integrality declaration of the branching variable. |
| `HCC-TREE-02` | branch split value is not an exact integer | Split value at the Branch record. |
| `HCC-TREE-03` | branch child identifier already exists | Child identifiers against the nodes already created. |
| `HCC-TREE-04` | node has more than one parent | Parent assignment during tree replay. |
| `HCC-TREE-05` | node is not reachable from the root | Reachability from the root, after replay. |
| `HCC-TREE-06` | node is neither branched nor terminated | Terminal record presence, after replay. |
| `HCC-TREE-07` | node carries more than one terminal record | Terminal record count per node, during replay. |
| `HCC-TREE-08` | leaf cites an inference made at a different node | Node of the cited inference against the node of the leaf. |
| `HCC-TREE-09` | integral leaf point is not feasible or not integral | Reduced point of an integral leaf against the node model. |
| `HCC-TREE-10` | integral leaf objective does not equal its cited bound | Objective of the integral leaf point against its cited bound. |
| `HCC-TREE-11` | frontier leaf has no finite bound | Finiteness of the frontier bound. |
| `HCC-TREE-12` | frontier bound is at least the incumbent objective and must be a closed leaf | Frontier bound against the incumbent objective. |
| `HCC-TREE-13` | tree depth exceeds the cap | Replay stack depth against the cap. |
| `HCC-TREE-14` | root names a reduced model the transform chain does not produce | Root reduced model hash against the replayed chain. |

## HCC-POINT

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-POINT-01` | point violates a row side | Exact row activity against each finite row side. |
| `HCC-POINT-02` | point violates a variable bound | Each component against its finite bounds. |
| `HCC-POINT-03` | point is not integral where the model declares integrality | Integrality of each component of a variable declared integer. |
| `HCC-POINT-04` | recomputed objective differs from the objective claimed | Recomputed objective against the claimed value. |
| `HCC-POINT-05` | original and reduced witnesses do not correspond through the transform chain | Reduced point mapped through the replayed chain against the original point. |
| `HCC-POINT-06` | point dimension does not match the model | Vector length against the model dimension. |
| `HCC-POINT-07` | point-only certificate contains a transform, root, or tree record | Record kinds present alongside a point-only claim. |

## HCC-RAY

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-RAY-01` | direction is not a recession direction of a row | Row activity of the direction against each row sense. |
| `HCC-RAY-02` | direction leaves a finite variable bound | Sign of each direction component against the finite bounds. |
| `HCC-RAY-03` | objective does not strictly improve along the direction | Objective inner product with the direction. |
| `HCC-RAY-04` | ray base point is not feasible | Base point against every row and bound of the original model. |
| `HCC-RAY-05` | ray base point is not integral where the model declares integrality | Integrality of the base point where declared. |
| `HCC-RAY-06` | mixed-integer ray has no positive integer step | Presence and sign of the integer step, for a mixed-integer claim. |
| `HCC-RAY-07` | stepped direction is not integral on an integer variable | Integrality of the stepped direction on each integer variable. |
| `HCC-RAY-08` | ray certificate contains a transform, root, or tree record | Record kinds present alongside a ray claim. |

## HCC-CLAIM

| Identifier | Message | Earliest check |
|---|---|---|
| `HCC-CLAIM-01` | header claim kind differs from the claim record | Header claim kind against the Claim record. |
| `HCC-CLAIM-02` | the records present do not close the claimed status | Recomputed closure against the claimed status. |
| `HCC-CLAIM-03` | a witness record the claim requires is missing | Presence of the witness records the claim requires. |
| `HCC-CLAIM-04` | optimal claim has an open frontier leaf | Leaf kinds present, for an optimal claim. |
| `HCC-CLAIM-05` | optimal claim has a leaf bound below the incumbent objective | Each closed leaf bound against the incumbent objective. |
| `HCC-CLAIM-06` | gap claim has no open frontier leaf | Frontier leaf presence, for a gap claim. |
| `HCC-CLAIM-07` | gap claim lower bound is not strictly below the incumbent objective | Recomputed L against U, for a gap claim. |
| `HCC-CLAIM-08` | recomputed gap differs from the gap stated | Recomputed gap against the stated gap. |
| `HCC-CLAIM-09` | infeasible claim has a leaf that is not a contradiction | Leaf kinds present, for an infeasible claim. |
| `HCC-CLAIM-10` | claim kind is not permitted with the records present | Claim kind against the record kinds present. |
