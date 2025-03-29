NAME          FLOW8
* A minimum-cost flow on five nodes and eight arcs, with capacities written as
* upper bounds rather than as rows. The node balance rows are equalities whose
* right-hand sides sum to zero, which is the shape a conservation law takes and
* the shape a sign error in the objective constant hides in.
ROWS
 N  COST
 E  N0
 E  N1
 E  N2
 E  N3
 E  N4
COLUMNS
    A01       COST      4.0            N0        1.0
    A01       N1        -1.0
    A02       COST      2.0            N0        1.0
    A02       N2        -1.0
    A12       COST      1.0            N1        1.0
    A12       N2        -1.0
    A13       COST      6.0            N1        1.0
    A13       N3        -1.0
    A23       COST      3.0            N2        1.0
    A23       N3        -1.0
    A24       COST      8.0            N2        1.0
    A24       N4        -1.0
    A34       COST      2.0            N3        1.0
    A34       N4        -1.0
    A14       COST      9.0            N1        1.0
    A14       N4        -1.0
RHS
    RHS       N0        12.0           N4        -12.0
BOUNDS
 UP BND       A01       7.0
 UP BND       A02       8.0
 UP BND       A12       4.0
 UP BND       A13       5.0
 UP BND       A23       6.0
 UP BND       A24       5.0
 UP BND       A34       9.0
 UP BND       A14       3.0
ENDATA
