NAME          TRANSPORT33
* A three-plant, three-market transportation problem. Supplies total
* seventy-five and demands total seventy-five, so every supply row and every
* demand row binds exactly. The structure is the one every network-flow
* implementation is first tested on: a totally unimodular matrix whose vertices
* are integral even though nothing here declares an integer variable.
ROWS
 N  COST
 E  SUP0
 E  SUP1
 E  SUP2
 E  DEM0
 E  DEM1
 E  DEM2
COLUMNS
    X00       COST      8.0            SUP0      1.0
    X00       DEM0      1.0
    X01       COST      6.0            SUP0      1.0
    X01       DEM1      1.0
    X02       COST      10.0           SUP0      1.0
    X02       DEM2      1.0
    X10       COST      9.0            SUP1      1.0
    X10       DEM0      1.0
    X11       COST      12.0           SUP1      1.0
    X11       DEM1      1.0
    X12       COST      13.0           SUP1      1.0
    X12       DEM2      1.0
    X20       COST      14.0           SUP2      1.0
    X20       DEM0      1.0
    X21       COST      9.0            SUP2      1.0
    X21       DEM1      1.0
    X22       COST      16.0           SUP2      1.0
    X22       DEM2      1.0
RHS
    RHS       SUP0      20.0           SUP1      30.0
    RHS       SUP2      25.0           DEM0      10.0
    RHS       DEM1      25.0           DEM2      40.0
ENDATA
