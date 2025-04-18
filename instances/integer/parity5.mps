NAME PARITY5
* An infeasibility the relaxation cannot see. Every coefficient in the
* equality is even and the right-hand side is odd, so no integer assignment
* satisfies it while the continuous relaxation satisfies it comfortably.
*
* With separation off the certificate is a tree whose every leaf is a
* contradiction, which is worth reading once. With it on, one Gomory cut sees
* the parity and the root closes by a single contradiction. Both are in the
* scoreboard, side by side.
ROWS
 N COST
 E BALANCE
 L ROOM
COLUMNS
 MARKER 'MARKER' 'INTORG'
 A COST 3 BALANCE 4
 A ROOM 1
 B COST 4 BALANCE 6
 B ROOM 1
 C COST 5 BALANCE 10
 C ROOM 1
 MARKER 'MARKER' 'INTEND'
RHS
 RHS BALANCE 15 ROOM 5
BOUNDS
 UI BND A 5
 UI BND B 5
 UI BND C 5
ENDATA
