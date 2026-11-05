# AC-130 gunship call-in

Click **AC-130** on the command dock, or press **J**, then click the target area.
It costs **60 supply**; one gunship may be active for each side. Right-click or
Escape cancels targeting. The same call-in works on the training ground.

The aircraft enters already airborne from the owning player's map edge. It
flies at least 220 metres above the terrain and joins a 200 metre radius
port-side orbit over the selected point, completes exactly
three full circles while firing, rolls out, climbs and leaves across the far
edge. It never uses a runway, parking stand or base production queue. The
selected point stays fixed even if units in the area move or die. Inbound and
outbound legs do not fire. Normal anti-aircraft weapons can engage it.

The 30 mm fires once per 1.2 seconds; the 105 mm fires once per 3.8 seconds.
Both fire from their respective left-side model sockets, with visible shell
travel and lingering pale smoke trails, separate cannon sounds, splash damage,
earth/debris blasts and ground
marks. The heavy gun has a six metre damage radius and the larger blast.
Firing rates, flight speed and altitude are gameplay choices.

Flight phase, orbit angle, completed arc, firing timers and shells in flight
are saved, so loading continues the same mission. The roster label displays
completed circles out of three.

The native geometry is in `native/src/c130_models.rs`: rounded cargo fuselage,
high wing, single fin, four six-blade animated propellers, closed gear doors,
cockpit panes, sensors, and forward 30 mm/aft 105 mm gun positions. Runtime
materials reuse the game's grey aircraft finish. The mesh is also included
in `--export-art` for Blender inspection.

## Photographic references

Downloaded references and source/credit URLs are in `references/` and
`references/sources.json`. They are modelling references, separate from runtime
art. Consult the linked source pages for the photographer and reuse notice.

- [USAF AC-130J fact sheet and aircraft photograph](https://www.af.mil/About-Us/Fact-Sheets/Display/Article/467756/ac-130j-ghostrider/)
- [Flying into the future: four-engine silhouette](https://www.hurlburt.af.mil/News/Article-Display/Article/810950/flying-into-the-future/)
- [Live-fire training: gun position and target view](https://www.dvidshub.net/image/6623269/afsoc-ac-130j-conducts-first-live-fire-training-japan)
- [105 mm loading and breech](https://www.dvidshub.net/image/6427971/ac-130j-crew-maintain-proficiency-combat-readiness)
- [Front-quarter airframe and propellers](https://www.dvidshub.net/image/9103340/afsoc-celebrates-35th-anniversary)
- [Port-side guns and underside, USAF photo by Senior Airman Jose Miguel T. Tamondong](https://www.c-130.net/g3/c-130-photos/North-American-C-130-Photos/USAF-C-130/240529-F-XX992-1681)

## Verification

`cargo test --locked gunship_` covers both player edges, corner targets, three
complete circles, cadence, departure, invalid purchases, pause/save restoration,
damage, ground marks, model sockets and propellers. A native capture can be made
with `--offscreen --compact --c130-check --smoke-test <absolute.png>`.
