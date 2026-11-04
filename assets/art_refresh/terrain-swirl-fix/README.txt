TERRAIN SWIRL DIAGNOSIS AND CORRECTION

The large pale wave bands remained in colour-only rendering, and still remained
with every ground texture sampled down to a constant colour. The raw CPU terrain
coverage and isolated grass texture did not contain the bands. This isolated
the cause to the shader's procedural colour/blend modulation, not the retained
meadow normal/height maps.

The photographic terrain now uses broad soil coverage without whole-field
warped high-frequency jitter or global fine colour modulation. Road-edge
perturbation is weaker and restricted to the actual verge. Grass/stony textures,
meadow normal and height data, terrain geometry, sun, collisions and navigation
are unchanged. The correction applies to matches, training and the demo.

Save your match and restart the game to load the updated embedded shader.

comparison.html: matched lit, colour-only, and constant-texture comparisons.
validation-summary.json: test/render results and preserved asset checks.
band-regression.json: diagnostic spatial-frequency regression measurement.

Developer checks:
  tools/check_terrain_swirl.py (original component isolation)
  tools/probe_terrain_sampling.py (private catalogs with filtered-mean materials)
  tools/verify_terrain_swirl_fix.py (before/after, Low, DLSS and isolation proof)
  tools/finalize_terrain_swirl.py (summary and source asset verification)

--legacy-terrain-noise reproduces the previous modulation for render checks.
--terrain-debug 7 displays the processed soil/road coverage in smoke captures.
The private diagnostic runtimes under .data/render-checks/terrain-swirl/runtimes
contain hardlinked read-only material images and an assets junction. Only their
private catalog files are modified; the project material catalog stays intact.
