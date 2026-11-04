# Aircraft photographic references

These photographs were inspected for silhouette, component proportions, cockpit
construction and surface finish. They are references only: no photograph has
been projected onto the model or bundled as a game texture.

- [A-10 Thunderbolt II 80-0252, Wikimedia Commons](https://commons.wikimedia.org/wiki/File:A-10_Thunderbolt_II_80-0252.jpg): nose and canopy proportions, recessed engine intakes, wing thickness, wheel fairings and grey paint.
- [US Air Force A-10C fact sheet](https://www.af.mil/About-Us/Fact-Sheets/Fact-Sheet-Display/Article/123518/a-10-thunderbolt-ii/): aircraft identity and general configuration.
- [Boeing Apache photograph](https://www.boeing.com/content/dam/boeing/v2/products/ah-64-apache/ah-64-roadblock-1.jpg): angular tandem windows, painted cockpit roof, engine shoulders, slim tail boom, swept rotor tips and muted olive finish.
- [Boeing AH-64 Apache](https://www.boeing.com/content/theboeingcompany/us/en/defense/military-rotorcraft/ah-64-apache.html): additional manufacturer imagery and overall layout.

The game models preserve the established unit scale and airfield clearances;
they are optimized visual approximations rather than dimensionally exact replicas.
The user's supplied A-10 three-view drawing is packed, with its original
attribution intact, in `assets/aircraft/a10/a10.blend`. Its six main surface
meshes are authored in that Blender source and exported to `a10_geometry.json`;
the game embeds those exported meshes. Helicopter geometry remains authored in
`native/src/aircraft_geometry.rs`, with mechanical parts, weapon sockets and rigid
animation pivots for both aircraft in `native/src/aircraft_models.rs`.
The supplied repo paint texture provides subtle wear after a neutral colour
conversion in the renderer. Editable Blender review exports use the same meshes
and exported neutral paint texture.

## Base and runway scale

The runway prototype uses 20% of a 5,000-foot (1,524-metre) reference
operating length. This is a gameplay scale choice, not a universal A-10 minimum.
The prototype width uses 20% of 150 feet (45.72 metres), or 9.144 metres.
The current compound scale of 80% makes the placed runway about 244 metres
long and 7.3 metres wide. Aircraft
retain their existing gameplay scale so units remain readable.

- [USAF ACC 2018 Demo Team and Heritage Flight Support Manual](https://www.acc.af.mil/Portals/92/Docs/2018%20Demo%20Team%20and%20HF%20Support%20Manual%20%28Chg%204%29.pdf?ver=2018-05-29-070656-207): A-10 demonstration runway reference of 5,000 feet.
- [Air Force runway design criteria, WBDG](https://www.wbdg.org/FFC/AF/AFMAN/111111_Runway.pdf): 150-foot runway width reference for fighter/trainer operations.

The whole compound has been relaid around that runway, with larger facilities,
service roads, separated aircraft parking and a continuous perimeter. Both new
operation maps are 1024 x 1024 metres to accommodate the expanded compounds.
