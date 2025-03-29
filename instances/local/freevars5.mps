NAME          FREEVARS5
* Free variables, negative lower bounds, and a fixed column together. Every
* nonbasic resting place the search knows about appears here, which is the
* point: a cold start that placed every column at its lower bound would put the
* free one at negative infinity, and a bounded simplex that treated the fixed
* one as movable would price a column that cannot go anywhere.
ROWS
 N  COST
 E  EQ0
 G  GE0
 L  LE0
COLUMNS
    U         COST      2.0            EQ0       1.0
    U         GE0       1.0
    V         COST      -3.0           EQ0       1.0
    V         LE0       1.0
    W         COST      1.0            GE0       1.0
    W         LE0       -1.0
    Z         COST      5.0            EQ0       1.0
RHS
    RHS       EQ0       6.0            GE0       -4.0
    RHS       LE0       9.0
BOUNDS
 FR BND       U
 LO BND       V         -5.0
 UP BND       V         12.0
 MI BND       W
 UP BND       W         3.0
 FX BND       Z         2.0
ENDATA
