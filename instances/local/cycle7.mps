NAME          CYCLE7
* Beale's example, the standard demonstration that a simplex can return to a
* basis it has already held and repeat the sequence forever. Every pivot from
* the starting basis is degenerate. It is here so that a change to the pricing
* rule or the ratio test has to face it before it is committed.
ROWS
 N  COST
 E  R1
 E  R2
 E  R3
COLUMNS
    X4        COST      -0.75          R1        0.25
    X4        R2        0.5
    X5        COST      150.0          R1        -60.0
    X5        R2        -90.0
    X6        COST      -0.02          R1        -0.04
    X6        R2        -0.02          R3        1.0
    X7        COST      6.0            R1        9.0
    X7        R2        3.0
    S1        R1        1.0
    S2        R2        1.0
    S3        R3        1.0
RHS
    RHS       R3        1.0
ENDATA
