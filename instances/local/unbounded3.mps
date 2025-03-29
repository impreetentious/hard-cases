NAME          UNBOUNDED3
* The objective falls without limit along a direction the constraints permit.
* One column is bounded and pulls the search into a feasible region first, so
* the ray is found from a feasible point rather than from the origin.
ROWS
 N  COST
 G  D0
 L  D1
COLUMNS
    F         COST      -2.0           D0        1.0
    G         COST      1.0            D0        1.0
    G         D1        1.0
RHS
    RHS       D0        3.0            D1        10.0
BOUNDS
 UP BND       G         8.0
ENDATA
