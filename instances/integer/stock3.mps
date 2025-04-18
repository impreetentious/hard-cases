NAME STOCK3
* Three stock lengths bought in whole units against a yield requirement and a
* machine-hours limit. The relaxation buys two and a fraction of the cheapest
* length, which is the answer integrality forbids, and no rounding of it is
* feasible -- so the search splits at thresholds other than zero and one, and
* the branch records in the certificate carry an integer a reader has to look
* at rather than a bit. Three hundred and forty-three assignments, which the
* exhaustive oracle walks in full.
ROWS
 N COST
 G YIELD
 L HOURS
COLUMNS
 MARKER 'MARKER' 'INTORG'
 A COST 5 YIELD 3
 A HOURS 2
 B COST 7 YIELD 4
 B HOURS 3
 C COST 9 YIELD 7
 C HOURS 5
 MARKER 'MARKER' 'INTEND'
RHS
 RHS YIELD 19 HOURS 17
BOUNDS
 UI BND A 6
 UI BND B 6
 UI BND C 6
ENDATA
