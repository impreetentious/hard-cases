NAME KNAP9
* A nine-item binary knapsack whose relaxation is fractional at the root, so
* the search has to branch and the cover separator has something to find. The
* box is five hundred and twelve assignments, which the exhaustive oracle
* walks in full.
ROWS
 N COST
 L CAP
COLUMNS
 MARKER 'MARKER' 'INTORG'
 X1 COST -10 CAP 6
 X2 COST -12 CAP 7
 X3 COST -7 CAP 4
 X4 COST -9 CAP 5
 X5 COST -5 CAP 3
 X6 COST -8 CAP 5
 X7 COST -11 CAP 6
 X8 COST -6 CAP 4
 X9 COST -4 CAP 3
 MARKER 'MARKER' 'INTEND'
RHS
 RHS CAP 21
BOUNDS
 BV BND X1
 BV BND X2
 BV BND X3
 BV BND X4
 BV BND X5
 BV BND X6
 BV BND X7
 BV BND X8
 BV BND X9
ENDATA
