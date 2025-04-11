\ Two-sided rows written in both directions, an equality, and the two words
\ that remove a bound. This is the range table the MPS reader applies,
\ reached through a different grammar.
Maximize
 profit: 2 a + 3 b - c
Subject To
 r1: 1 <= a + b <= 5
 r2: 8 >= 2 a - c >= -2
 r3: a + b + c = 3
 r4: c >= -4
Bounds
 a <= 6
 b free
 -1 <= c <= 9
End
