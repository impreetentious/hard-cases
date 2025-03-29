NAME          BLEND4
* A blending problem: four raw materials mixed into one hundred units of
* product, with two quality specifications written as ranged rows and a
* purchase limit on the cheapest input. Ranged rows are the reason this
* instance is here -- a reader that mishandles the sign of a range produces a
* model that is feasible, solvable, and wrong.
ROWS
 N  COST
 E  MASS
 G  PROT
 L  FAT
 G  FIBRE
COLUMNS
    A         COST      12.0           MASS      1.0
    A         PROT      0.20           FAT       0.08
    A         FIBRE     0.02
    B         COST      9.0            MASS      1.0
    B         PROT      0.12           FAT       0.03
    B         FIBRE     0.06
    C         COST      15.0           MASS      1.0
    C         PROT      0.35           FAT       0.12
    C         FIBRE     0.01
    D         COST      7.0            MASS      1.0
    D         PROT      0.05           FAT       0.01
    D         FIBRE     0.11
RHS
    RHS       MASS      100.0          PROT      16.0
    RHS       FAT       7.0            FIBRE     4.0
RANGES
    RNG       PROT      6.0
BOUNDS
 UP BND       D         40.0
ENDATA
