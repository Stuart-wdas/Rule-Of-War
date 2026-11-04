# Rolling mushroom explosion

This earlier study is retained as an editable source. The current game uses
the two reference-directed animations in `../reference_blasts/`. This folder's
preview page also shows that updated pair, preserving existing preview links.

Double-click **View Explosion Preview.cmd** in the repository root, or open
`preview.html` directly in a browser. The offline player includes play/pause,
frame scrubbing, half/double speed, three impact sizes and a night backdrop.
`preview.gif` is a quick looping review; `preview.png` is a still.

`mushroom_explosion.blend` is the editable source. The 64-frame timeline lasts
6.4 seconds. Frame 5 is the hot burst, 22 is the rising crown, and 40 is mature
smoke. The crown lobes roll outward while a narrow column feeds the centre;
animated 4D noise drives density and temperature. The ground plane is excluded
from rendering to keep the exported effect transparent. Lights and the elevated
RTS camera are included. Use Rendered viewport shading or F12 to inspect a frame.

This is an authored volume animation, not a baked fluid simulation. The game
uses its prelit RGBA render, so each heavy explosion needs one animated cloud
plus a brief flash core, shared textures and the existing sparks/debris/dust.
Rocket-artillery strikes, helicopter rocket impacts and vehicle destruction use
the effect; smaller impacts retain their existing compact bursts. The animation
fades fully to transparent, and each frame gets isolated alpha-aware mipmaps.

Rebuild from the repository root:

```powershell
& 'C:/Program Files/Blender Foundation/Blender 5.2/blender.exe' --background --python assets/Realistic_Explosion_VFX/tools/author_mushroom.py
# Use a Python environment with Pillow:
python assets/Realistic_Explosion_VFX/tools/pack_mushroom.py
cargo build --locked
```

The author's camera also exports the projected ground pivot in `mushroom.json`;
the game and player use that pivot to anchor the cloud to the impact. Keep it
with `mushroom_8x8.png`. The packed sheet is 2560×3072, with 320×384 frame cells.
The original supplied VFX remain intact for light impacts and persistent fires.
