NAME          INFEASIBLE4
* No feasible point, and not obviously so: the contradiction needs two rows and
* a bound together rather than a single crossed pair. The first phase has to
* run to completion and report what it could not repair.
ROWS
 N  COST
 G  A
 G  B
 L  C
COLUMNS
    P         COST      1.0            A         2.0
    P         B         1.0            C         1.0
    Q         COST      1.0            A         1.0
    Q         B         3.0            C         1.0
RHS
    RHS       A         12.0           B         15.0
    RHS       C         6.0
BOUNDS
 UP BND       P         4.0
 UP BND       Q         4.0
ENDATA
