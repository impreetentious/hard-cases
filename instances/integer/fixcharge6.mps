NAME FIXCHARGE6
* A fixed-charge facility model: three sites, each with a capacity that only
* exists once its opening cost is paid. The linking rows are what make the
* relaxation fractional -- a site half open serves half its capacity for half
* the cost, which is exactly the answer integrality forbids. Three continuous
* columns beside three binary ones, so the exhaustive oracle cannot reach it
* and the standalone verifier is the only check there is.
ROWS
 N COST
 G DEMAND
 L LINK1
 L LINK2
 L LINK3
COLUMNS
 X1 COST 2 DEMAND 1
 X1 LINK1 1
 X2 COST 3 DEMAND 1
 X2 LINK2 1
 X3 COST 4 DEMAND 1
 X3 LINK3 1
 MARKER 'MARKER' 'INTORG'
 Y1 COST 20 LINK1 -10
 Y2 COST 16 LINK2 -10
 Y3 COST 14 LINK3 -10
 MARKER 'MARKER' 'INTEND'
RHS
 RHS DEMAND 14
BOUNDS
 UP BND X1 10
 UP BND X2 10
 UP BND X3 10
 BV BND Y1
 BV BND Y2
 BV BND Y3
ENDATA
