NAME          COVER6
* A set-covering relaxation: six shifts covering seven demand periods, each
* period needing at least its stated coverage. Every row is a greater-than row
* with a positive right-hand side, so the origin is infeasible and the first
* phase has real work to do before the objective is looked at once.
ROWS
 N  COST
 G  P0
 G  P1
 G  P2
 G  P3
 G  P4
 G  P5
 G  P6
COLUMNS
    S0        COST      170.0          P0        1.0
    S0        P1        1.0            P2        1.0
    S1        COST      160.0          P1        1.0
    S1        P2        1.0            P3        1.0
    S2        COST      175.0          P2        1.0
    S2        P3        1.0            P4        1.0
    S3        COST      180.0          P3        1.0
    S3        P4        1.0            P5        1.0
    S4        COST      195.0          P4        1.0
    S4        P5        1.0            P6        1.0
    S5        COST      205.0          P5        1.0
    S5        P6        1.0            P0        1.0
RHS
    RHS       P0        4.0            P1        6.0
    RHS       P2        9.0            P3        7.0
    RHS       P4        5.0            P5        8.0
    RHS       P6        3.0
ENDATA
