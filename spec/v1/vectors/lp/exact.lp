\ The same model the MPS vector of this name carries, written in LP format.
\ Everything below the digest line of the two canonical dumps is identical,
\ which is what makes this pair a round-trip vector rather than two files.
Minimize
 OBJ: 0.1 X - 2.5 Y - 5
Subject To
 R1: 3 X - 3 X
     + 1.25 Y + 0.75 Y >= 1
 R2: 1.5e2 X = 300
Bounds
 X <= 2.5
Binary
 Y
End
