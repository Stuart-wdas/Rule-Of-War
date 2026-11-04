// Included in simulation.rs: authoritative, deterministic tactical behaviour.
fn weapon_can_damage(weapon: &str, target: &Piece) -> bool {
    let small_arms = spec(weapon)["category"] == "troop" || weapon == "troopCarrier";
    !(small_arms && matches!(target.category.as_str(), "vehicle" | "aircraft"))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct BattlefieldProp {
    pub id: String,
    pub kind: String,
    pub at: Point,
    pub heading: f32,
    pub hp: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scar: Option<crate::ground_impacts::Mark>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wreck: Option<crate::destruction::Wreck>,
}
impl BattlefieldProp {
    pub fn grounded(&self) -> bool {
        self.wreck.as_ref().is_none_or(|w| w.crash.is_none())
    }
    pub fn local(&self, at: Point) -> Point {
        let (s, c) = self.heading.sin_cos();
        let (dx, dz) = (at[0] - self.at[0], at[1] - self.at[1]);
        [dx * c - dz * s, dx * s + dz * c]
    }
    pub fn nearest(&self, at: Point) -> Point {
        let Some(wreck) = &self.wreck else {
            return self.at;
        };
        let local = self.local(at);
        let x = local[0].clamp(-wreck.half_extents[0], wreck.half_extents[0]);
        let z = local[1].clamp(-wreck.half_extents[1], wreck.half_extents[1]);
        let (s, c) = self.heading.sin_cos();
        [self.at[0] + c * x + s * z, self.at[1] - s * x + c * z]
    }
    pub fn blocks(&self, at: Point) -> bool {
        self.wreck.as_ref().is_some_and(|w| {
            w.crash.is_none() && !w.piece.building && {
                let local = self.local(at);
                local[0].abs() < w.half_extents[0] + 0.2 && local[1].abs() < w.half_extents[1] + 0.2
            }
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct BattleEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub tick: u64,
    pub kind: String,
    pub at: Point,
    pub from: Option<Point>,
    pub owner: i32,
    pub radius: f32,
    #[serde(default)]
    pub weapon: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub release_height: Option<f32>,
}
impl NativeGame {
    fn infantry_manoeuvres(&mut self) {
        if !self.tick.is_multiple_of(20) {
            return;
        }
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            if e.view.category != "troop"
                || e.view.hp <= 0.
                || e.view.downed
                || e.view.retreating
                || e.view.training_target
                || e.transport.is_some()
                || e.view.garrison.is_some()
                || e.medical_until > self.tick
                || e.stance != "attack"
                || e.player_ordered
                || e.halted
                || e.cover_retry_after > self.tick
                || e.warfare.active(self.tick)
                || e.tactical_until > self.tick
                || e.orders.front().is_some_and(|o| {
                    !["attack", "attack-move", "patrol"].contains(&o.kind.as_str())
                })
            {
                continue;
            }
            let Some(threat) = e
                .threat
                .filter(|_| e.threat_until > self.tick)
                .or_else(|| self.entity_threat(e))
            else {
                continue;
            };
            let pressure = e.threat_until > self.tick
                && (e.view.suppression >= 0.65 || e.view.hp / e.view.max_hp < 0.35);
            let rotate = e.view.suppression >= 0.4 && e.speed < 0.01 && e.threat_until > self.tick;
            let blocked = e.speed < 0.01
                && distance(position(e), threat) <= self.weapon_range(&e.view.kind)
                && !self.building_line_clear(position(e), threat, None, None);
            if !pressure
                && e.orders.is_empty()
                && !e.cover_ordered
                && distance(position(e), threat) > self.weapon_range(&e.view.kind)
            {
                let building = self
                    .entities
                    .iter()
                    .filter(|b| {
                        b.view.building
                            && b.view.hp > 0.
                            && (b.view.owner == e.view.owner || b.view.owner < 0)
                    })
                    .flat_map(|b| {
                        let slots = if b.view.lots.is_empty() {
                            self.window_slots(b, None)
                        } else {
                            (0..b.view.lots.len())
                                .flat_map(|l| self.window_slots(b, Some(l)))
                                .collect()
                        };
                        slots
                            .into_iter()
                            .map(move |s| (b.view.id.clone(), distance(position(e), s.0)))
                    })
                    .filter(|(_, d)| *d <= 10.)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(id, _)| id);
                if let Some(building) = building {
                    let owner = e.view.owner;
                    let id = e.view.id.clone();
                    if self.enter_building(owner, &[id], &building, Some(threat)) > 0 {
                        continue;
                    }
                }
            }
            if !pressure && !rotate && !blocked {
                continue;
            }
            let e = &self.entities[i];
            let at = position(e);
            let d = distance(at, threat).max(0.01);
            let away = [(at[0] - threat[0]) / d, (at[1] - threat[1]) / d];
            let occupied: Vec<_> = self
                .entities
                .iter()
                .enumerate()
                .filter(|(j, e)| *j != i && !e.view.downed)
                .map(|(_, e)| e.cover_goal.unwrap_or(position(e)))
                .collect();
            let anchor = if pressure {
                [at[0] + away[0] * 10., at[1] + away[1] * 10.]
            } else {
                [at[0] - away[1] * 5., at[1] + away[0] * 5.]
            };
            let cover = self
                .nearby_cover_slot(i, anchor, 8., &occupied)
                .filter(|p| distance(at, *p) > 2.);
            let candidates = [
                cover,
                Some(anchor),
                Some([
                    at[0] + away[1] * 5. + away[0] * 5.,
                    at[1] - away[0] * 5. + away[1] * 5.,
                ]),
            ];
            for goal in candidates.into_iter().flatten() {
                if !self.passable(goal, false) || (pressure && distance(goal, threat) <= d + 2.) {
                    continue;
                }
                let path = self.route(i, at, goal);
                if path.is_empty() {
                    continue;
                }
                let e = &mut self.entities[i];
                e.path = path;
                e.cover_goal = cover.filter(|p| distance(*p, goal) < 0.1);
                e.cover_ordered = false;
                e.tactical_until = self.tick + 120;
                break;
            }
        }
    }
    #[cfg(test)]
    fn point_visible(&self, owner: i32, at: Point) -> bool {
        self.observation().sees(owner,at)
    }
    fn town_street_at(&self, at: Point) -> bool {
        self.entities
            .iter()
            .filter(|e| !e.view.lots.is_empty() && e.view.label != "PRACTICE HOUSES")
            .any(|e| {
                crate::settlement_layout::roads_for(&e.view.kind, e.view.town_plan)
                    .iter()
                    .any(|r| r.distance([at[0] - e.view.x, at[1] - e.view.z]) < 2.3)
            })
    }
    fn seed_cover(&mut self) {
        let anchors: Vec<_> = self
            .entities
            .iter()
            .filter(|e| e.view.kind == "objective" || e.view.kind == "barracks")
            .map(position)
            .collect();
        for (i, at) in anchors.iter().enumerate() {
            for (j, offset) in [[-6., -5.], [6., 5.], [-9., 4.], [9., -4.]]
                .iter()
                .enumerate()
            {
                let p = [at[0] + offset[0], at[1] + offset[1]];
                if self.passable(p, false)
                    && self.clear_site(p)
                    && (j < 2 || !self.town_street_at(p))
                {
                    self.props.push(BattlefieldProp {
                        scar: None,
                        id: format!("cover-{i}-{j}"),
                        kind: if j < 2 { "sandbags" } else { "wall" }.into(),
                        at: p,
                        heading: if j % 2 == 0 {
                            0.
                        } else {
                            std::f32::consts::FRAC_PI_2
                        },
                        wreck: None,
                        hp: 4.,
                    });
                }
            }
        }
        if self.terrain.id == crate::battlefield::ID {
            for (row, z) in [80., 112., 176., 208., 272., 304.].into_iter().enumerate() {
                for (col, x) in [132., 164., 220., 252.].into_iter().enumerate() {
                    let at = [
                        x / 384. * self.terrain.cols as f32,
                        z / 384. * self.terrain.rows as f32,
                    ];
                    if self.passable(at, false)
                        && self.clear_site(at)
                        && (row % 2 != 0 || !self.town_street_at(at))
                    {
                        self.props.push(BattlefieldProp {
                            scar: None,
                            id: format!("line-{row}-{col}"),
                            kind: if row % 2 == 0 { "wall" } else { "sandbags" }.into(),
                            at,
                            heading: std::f32::consts::FRAC_PI_2,
                            wreck: None,
                            hp: 5.,
                        });
                    }
                }
            }
            return;
        }
        // Deterministic roadside positions, bounded independently of map area.
        for i in 0..64 {
            let x = (8. + self.random() * (self.terrain.cols as f32 - 16.).max(1.))
                .min(self.terrain.cols as f32 - 1.);
            let z = (8. + self.random() * (self.terrain.rows as f32 - 16.).max(1.))
                .min(self.terrain.rows as f32 - 1.);
            if self.passable([x, z], false) && self.clear_site([x, z]) {
                self.props.push(BattlefieldProp {
                    scar: None,
                    id: format!("field-{i}"),
                    kind: if i % 3 == 0 { "crates" } else { "sandbags" }.into(),
                    at: [x, z],
                    heading: i as f32 * 1.7,
                    wreck: None,
                    hp: 3.,
                });
            }
        }
    }
    fn formation_destination(
        &self,
        index: usize,
        goal: Point,
        slot: usize,
        count: usize,
        occupied: &[Point],
    ) -> Point {
        let at = position(&self.entities[index]);
        let d = distance(at, goal).max(0.01);
        let forward = [(goal[0] - at[0]) / d, (goal[1] - at[1]) / d];
        let columns = (count as f32).sqrt().ceil().max(1.) as usize;
        let row = slot / columns;
        let row_count = (count - row * columns).min(columns);
        let side = (slot % columns) as f32 - (row_count as f32 - 1.) * 0.5;
        let spacing = if self.entities[index].view.category == "vehicle" {
            8.5
        } else {
            1.8
        };
        let candidate = [
            goal[0] - forward[1] * side * spacing - forward[0] * row as f32 * spacing,
            goal[1] + forward[0] * side * spacing - forward[1] * row as f32 * spacing,
        ];
        let s = spec(&self.entities[index].view.kind);
        let valid = if flag(s, "airUnit") {
            self.passable(candidate, true)
        } else if flag(s, "waterUnit") {
            self.sea_passable(candidate)
        } else {
            self.passable_clearance(
                candidate,
                0,
                body_clearance(
                    &self.entities[index].view.kind,
                    self.entities[index].view.model_scale,
                ),
            )
        };
        let candidate = if valid {
            candidate
        } else if self.entities[index].view.category == "vehicle" {
            self.clearance_destination(
                candidate,
                body_clearance(
                    &self.entities[index].view.kind,
                    self.entities[index].view.model_scale,
                ),
            )
            .unwrap_or(goal)
        } else {
            goal
        };
        if self.entities[index].view.category == "troop" {
            self.nearby_cover_slot(index, candidate, 8., occupied)
                .unwrap_or(candidate)
        } else {
            candidate
        }
    }
    fn cover_level(&self, at: Point, shooter: Option<Point>) -> u8 {
        let protects = |center: Point, width: f32| {
            distance(center, at) <= 3.2
                && shooter.is_none_or(|from| {
                    // The obstacle must be between shooter and target, not behind the target.
                    (center[0] - at[0]) * (from[0] - at[0])
                        + (center[1] - at[1]) * (from[1] - at[1])
                        > 0.
                        && segment_distance(center, at, from) < width
                })
        };
        let props = self
            .props
            .iter()
            .filter(|p| {
                if !p.grounded() {
                    return false;
                }
                if p.wreck.is_some() {
                    return protects(p.nearest(at), 1.);
                }
                let (sn, c) = p.heading.sin_cos();
                let half = if p.kind == "crates" { 1.2 } else { 2.1 };
                let along = ((at[0] - p.at[0]) * c - (at[1] - p.at[1]) * sn).clamp(-half, half);
                let edge = [p.at[0] + c * along, p.at[1] - sn * along];
                protects(edge, if p.kind == "crates" { 0.8 } else { 1. })
            })
            .map(|p| {
                if p.wreck.as_ref().is_some_and(|w| !w.piece.building) {
                    2
                } else if p.hp <= 0. || p.kind == "crates" || p.kind == "crater" {
                    1
                } else {
                    2
                }
            })
            .max()
            .unwrap_or(0);
        let buildings = self.entities.iter().any(|e| {
            e.view.building
                && e.view.kind != "objective"
                && e.view.hp > 0.
                && if e.view.lots.is_empty() {
                    protects(position(e), 2.)
                } else {
                    e.view.lots.iter().filter(|l| l.integrity > 0.).any(|lot| {
                        let dx = at[0] - e.view.x - lot.at[0];
                        let dz = at[1] - e.view.z - lot.at[1];
                        if dx * dx + dz * dz > (lot.width.hypot(lot.depth) * 0.42 + 3.2).powi(2) {
                            return false;
                        }
                        let local = lot.nearest([at[0] - e.view.x, at[1] - e.view.z]);
                        protects([e.view.x + local[0], e.view.z + local[1]], 3.)
                    })
                }
        });
        props.max(if buildings { 2 } else { 0 })
    }
    fn nearby_cover_slot(
        &self,
        index: usize,
        goal: Point,
        radius: f32,
        occupied: &[Point],
    ) -> Option<Point> {
        let _timing = crate::performance::span("infantry.cover_search");
        let e = &self.entities[index];
        let enemy = e
            .threat
            .filter(|_| e.threat_until > self.tick)
            .or_else(|| self.entity_threat(e))
            .or_else(|| {
                self.entities
                    .iter()
                    .filter(|t| {
                        t.view.owner >= 0
                            && t.view.owner != e.view.owner
                            && !t.view.downed
                            && t.view.hp > 0.
                            && number(spec(&t.view.kind), "damage", 0.) > 0.
                            && self.visible(e.view.owner, t)
                    })
                    .min_by(|a, b| {
                        distance(goal, position(a)).total_cmp(&distance(goal, position(b)))
                    })
                    .map(position)
            });
        let candidates = self
            .props
            .iter()
            .filter(|p| {
                p.grounded()
                    && (p.hp > 0. || p.wreck.is_some())
                    && distance(goal, p.nearest(goal)) <= radius
            })
            .flat_map(|p| {
                let from = enemy.unwrap_or(position(e));
                if let Some(wreck) = &p.wreck {
                    let from_local = p.local(from);
                    let (s, c) = p.heading.sin_cos();
                    let h = wreck.half_extents;
                    let sign = if enemy.is_some() { -1. } else { 1. };
                    let side = if from_local[0] >= 0. { sign } else { -sign };
                    let end = if from_local[1] >= 0. { sign } else { -sign };
                    return [-1., 0., 1.]
                        .into_iter()
                        .flat_map(|slot| {
                            [
                                [side * (h[0] + 0.7), slot * (h[1] - 0.2).max(0.)],
                                [slot * (h[0] - 0.2).max(0.), end * (h[1] + 0.7)],
                            ]
                        })
                        .map(|local| {
                            [
                                p.at[0] + c * local[0] + s * local[1],
                                p.at[1] - s * local[0] + c * local[1],
                            ]
                        })
                        .collect::<Vec<_>>();
                }
                let (sn, c) = p.heading.sin_cos();
                let normal = [sn, c];
                let side =
                    if (from[0] - p.at[0]) * normal[0] + (from[1] - p.at[1]) * normal[1] >= 0. {
                        -1.
                    } else {
                        1.
                    };
                let side = if enemy.is_some() { side } else { -side };
                let d = distance(from, p.at).max(0.01);
                let cap = [
                    p.at[0] + (p.at[0] - from[0]) / d * 3.4,
                    p.at[1] + (p.at[1] - from[1]) / d * 3.4,
                ];
                [-1.5, 0., 1.5]
                    .into_iter()
                    .map(move |along| {
                        [
                            p.at[0] + normal[0] * side * 0.95 + c * along,
                            p.at[1] + normal[1] * side * 0.95 - sn * along,
                        ]
                    })
                    .chain(std::iter::once(cap))
                    .collect::<Vec<_>>()
            })
            .chain(
                self.entities
                    .iter()
                    .filter(|b| b.view.building && b.view.hp > 0.)
                    .flat_map(|b| {
                        b.view
                            .lots
                            .iter()
                            .filter(|l| l.integrity > 0.)
                            .map(move |lot| {
                                let center = [b.view.x + lot.at[0], b.view.z + lot.at[1]];
                                let from = enemy.unwrap_or(position(e));
                                let direction = if enemy.is_some() {
                                    [center[0] - from[0], center[1] - from[1]]
                                } else {
                                    [from[0] - center[0], from[1] - center[1]]
                                };
                                let d = distance(direction, [0., 0.]).max(0.01);
                                let (dx, dz) = (direction[0] / d, direction[1] / d);
                                let (s, c) = lot.yaw.sin_cos();
                                let reach = (lot.width * 0.5 / (c * dx - s * dz).abs().max(0.001))
                                    .min(lot.depth * 0.5 / (s * dx + c * dz).abs().max(0.001))
                                    + 1.4;
                                [center[0] + dx * reach, center[1] + dz * reach]
                            })
                    }),
            );
        let attack_limit = e
            .orders
            .front()
            .filter(|o| o.kind == "attack")
            .and_then(|o| o.target.as_ref())
            .and_then(|id| {
                self.entities
                    .iter()
                    .position(|t| &t.view.id == id && self.visible(e.view.owner, t))
            })
            .map(|j| {
                (
                    j,
                    distance(position(e), attack_point(&self.entities[j], position(e)))
                        .min(self.infantry_attack_range(index, j) * 0.94),
                )
            });
        let mut ranked: Vec<_> = candidates
            .filter(|at| {
                distance(goal, *at) <= radius + 1.6
                    && self.passable(*at, false)
                    && occupied.iter().all(|p| distance(*at, *p) >= 1.35)
                    && self.cover_level(*at, enemy) > 0
                    && attack_limit.is_none_or(|(j, minimum)| {
                        distance(*at, attack_point(&self.entities[j], *at)) >= minimum - 0.25
                    })
            })
            .map(|at| {
                let score = distance(goal, at) * 3. + distance(position(e), at) * 0.1
                    - self.cover_level(at, enemy) as f32 * 1.5
                    + enemy
                        .map(|from| (12. - distance(from, at)).max(0.) * 2.)
                        .unwrap_or(0.);
                (score, at)
            })
            .collect();
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
        ranked.into_iter().map(|(_, at)| at).find(|at| {
            distance(position(e), *at) < 0.2 || !self.route(index, position(e), *at).is_empty()
        })
    }
    fn cover_threat(&self, index: usize) -> Option<Point> {
        self.entity_threat(&self.entities[index])
    }
    fn entity_threat(&self, e: &Entity) -> Option<Point> {
        self.entity_threat_if(e, |_, t| self.visible(e.view.owner, t))
    }
    fn entity_threat_observed(&self, e: &Entity, observed: &[[bool; 2]]) -> Option<Point> {
        self.entity_threat_if(e, |j, t| {
            observed[j]
                .get(e.view.owner as usize)
                .copied()
                .unwrap_or_else(|| self.visible(e.view.owner, t))
        })
    }
    fn entity_threat_if(&self, e: &Entity, seen: impl Fn(usize, &Entity) -> bool) -> Option<Point> {
        let own_range = self.weapon_range(&e.view.kind);
        self.entities
            .iter()
            .enumerate()
            .filter(|(j, t)| {
                t.view.owner >= 0
                    && t.view.owner != e.view.owner
                    && t.view.hp > 0.
                    && !t.view.downed
                    && t.transport.is_none()
                    && number(spec(&t.view.kind), "damage", 0.) > 0.
                    && distance(position(e), position(t))
                        <= self.weapon_range(&t.view.kind).max(own_range) + 8.
                    && seen(*j, t)
            })
            .min_by(|(_, a), (_, b)| {
                distance(position(e), position(a)).total_cmp(&distance(position(e), position(b)))
            })
            .map(|(_, t)| position(t))
    }
    fn retreat(&mut self, index: usize) -> bool {
        if !self.leave_building(index, false) {
            return false;
        }
        let e = &self.entities[index];
        let at = e
            .view
            .garrison
            .as_ref()
            .filter(|g| g.exiting)
            .and_then(|g| self.garrison_door(g))
            .map(|d| d.0)
            .unwrap_or(position(e));
        let base = self
            .entities
            .iter()
            .filter(|b| {
                b.view.owner == e.view.owner
                    && b.view.hp > 0.
                    && ["barracks", "fieldHospital"].contains(&b.view.kind.as_str())
            })
            .min_by(|a, b| {
                distance(position(e), position(a)).total_cmp(&distance(position(e), position(b)))
            })
            .map(|b| {
                let d = distance(at, position(b)).max(0.01);
                let away = [(at[0] - b.view.x) / d, (at[1] - b.view.z) / d];
                let (s, c) = b.view.heading.sin_cos();
                let local = [c * away[0] - s * away[1], s * away[0] + c * away[1]];
                let edge = (number(spec(&b.view.kind), "footprintWidth", 3.6)
                    * 0.5
                    * b.view.model_scale
                    / local[0].abs().max(0.001))
                .min(
                    number(spec(&b.view.kind), "footprintDepth", 3.6) * 0.5 * b.view.model_scale
                        / local[1].abs().max(0.001),
                );
                (
                    position(b),
                    edge + body_clearance(&e.view.kind, e.view.model_scale) + 1.4,
                )
            });
        let Some((base, clearance)) = base else {
            return false;
        };
        let d = distance(at, base).max(0.01);
        let goal = [
            base[0] + (at[0] - base[0]) / d * clearance,
            base[1] + (at[1] - base[1]) / d * clearance,
        ];
        let path = self.route(index, at, goal);
        if path.is_empty() {
            return false;
        }
        let e = &mut self.entities[index];
        e.cancel_local_actions();
        e.release_stop();
        e.cover_goal = None;
        e.cover_ordered = false;
        e.medical_until = 0;
        e.treatment_target = None;
        e.orders.clear();
        e.path = path;
        e.target = None;
        e.windup = 0;
        e.patrol_origin = None;
        e.stance = "retreat".into();
        e.view.retreating = true;
        e.orders.push_back(Order {
            kind: "retreat".into(),
            destination: Some(goal),
            target: None,
        });
        true
    }
    fn movement_factor(&self, index: usize) -> f32 {
        let e = &self.entities[index];
        let pace = if e.view.category == "troop" {
            3.6
        } else if e.view.category == "vehicle" {
            3.0
        } else {
            2.0
        };
        pace * if e.view.retreating {
            1.7
        } else if e.view.suppression >= 0.8 {
            0.22
        } else if e.view.suppression >= 0.45 {
            0.55
        } else {
            1.
        }
    }
    fn tactical_step(&mut self) {
        self.infantry_warfare();
        self.infantry_manoeuvres();
        self.events.retain(|e| {
            self.tick.saturating_sub(e.tick)
                <= if e.kind == "shot" {
                    ((e.duration.min(60.) / DT).ceil() as u64 + 5).max(24)
                } else {
                    24
                }
        });
        for e in &mut self.entities {
            if self.tick.saturating_sub(e.last_hit) > 20 {
                e.view.suppression = (e.view.suppression - 0.012).max(0.);
            }
            if e.view.downed && self.tick >= e.downed_until {
                e.view.hp = 0.;
            }
            if e.view.retreating && e.path.is_empty() {
                e.view.retreating = false;
                e.view.suppression = 0.;
                e.stance = "attack".into();
            }
        }
        let cover_visibility = std::cell::OnceCell::new();
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            if e.warfare.active(self.tick) {
                continue;
            }
            let Some(goal) = e.cover_goal else {
                continue;
            };
            let enemy = self.cover_threat(i);
            let invalid = !self.passable(goal, false)
                || self.cover_level(
                    goal,
                    enemy.or(e.threat.filter(|_| e.threat_until > self.tick)),
                ) == 0;
            let finished = !e.cover_ordered
                && !e.orders.is_empty()
                && enemy.is_none()
                && e.threat_until <= self.tick;
            let rescue_needed = flag(spec(&e.view.kind), "medical")
                && self.entities.iter().any(|a| {
                    a.view.owner == e.view.owner
                        && a.view.downed
                        && distance(position(e), position(a)) < 18.
                });
            let stalled = !e.cover_ordered
                && !e.orders.is_empty()
                && e.tactical_until > 0
                && e.tactical_until <= self.tick
                && self
                    .infantry_engagement_target(
                        i,
                        cover_visibility.get_or_init(|| self.visibility_mask()),
                    )
                    .is_none_or(|j| !self.infantry_firing_position(i, position(e), j));
            if invalid || finished || rescue_needed || stalled {
                if stalled {
                    self.entities[i].cover_retry_after = self.tick + 100;
                    self.entities[i].tactical_until = 0;
                }
                self.entities[i].cover_goal = None;
                self.entities[i].cover_ordered = false;
                if let Some(goal) = self.entities[i].orders.front().and_then(|o| o.destination) {
                    self.entities[i].path = self.route(i, position(&self.entities[i]), goal);
                } else {
                    self.entities[i].path.clear();
                }
            } else if enemy.is_some() {
                self.entities[i].threat = enemy;
            }
        }
        if self.tick.is_multiple_of(4) {
            for i in 0..self.entities.len() {
                let e = &self.entities[i];
                if e.view.hp <= 0.
                    || e.view.downed
                    || e.transport.is_some()
                    || e.stance != "attack"
                    || e.player_ordered
                    || e.halted
                    || e.cover_retry_after > self.tick
                    || e.medical_until > self.tick
                    || e.cover_goal.is_some()
                    || e.view.training_target
                    || e.view.garrison.is_some()
                    || e.warfare.active(self.tick)
                    || e.tactical_until > self.tick
                {
                    continue;
                }
                let medical = flag(spec(&e.view.kind), "medical");
                let automatic = e
                    .orders
                    .front()
                    .is_none_or(|o| ["attack", "attack-move", "patrol"].contains(&o.kind.as_str()));
                let medical_goal = if medical && e.orders.is_empty() && e.path.is_empty() {
                    self.entities
                        .iter()
                        .filter(|a| {
                            a.view.owner == e.view.owner
                                && a.view.downed
                                && distance(position(e), position(a)) < 18.
                        })
                        .min_by_key(|a| a.downed_until)
                        .map(|a| {
                            let p = position(a);
                            let d = distance(position(e), p).max(0.01);
                            [
                                p[0] + (e.view.x - p[0]) / d * 1.4,
                                p[1] + (e.view.z - p[1]) / d * 1.4,
                            ]
                        })
                } else {
                    None
                };
                let goal = if medical_goal.is_some() {
                    medical_goal
                } else if e.view.category == "troop"
                    && automatic
                    && (e.threat_until > self.tick || self.cover_threat(i).is_some())
                    && self.cover_level(position(e), self.cover_threat(i).or(e.threat)) < 2
                {
                    let occupied: Vec<_> = self
                        .entities
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .filter(|(_, other)| {
                            other.view.category == "troop"
                                && !other.view.downed
                                && other.transport.is_none()
                        })
                        .map(|(_, other)| other.cover_goal.unwrap_or(position(other)))
                        .collect();
                    let anchor = e.orders.front().and_then(|o| o.destination).filter(|goal| {
                        distance(position(e), *goal) <= 24. && e.view.suppression < 0.8
                    });
                    anchor
                        .and_then(|goal| self.nearby_cover_slot(i, goal, 8., &occupied))
                        .or_else(|| self.nearby_cover_slot(i, position(e), 12., &occupied))
                } else {
                    None
                };
                if let Some(goal) = goal {
                    let path = self.route(i, position(&self.entities[i]), goal);
                    if !path.is_empty() || distance(position(&self.entities[i]), goal) < 0.2 {
                        self.entities[i].path = path;
                        if medical_goal.is_none() {
                            self.entities[i].cover_goal = Some(goal);
                            let seconds = distance(position(&self.entities[i]), goal)
                                / (number(spec(&self.entities[i].view.kind), "speed", 0.8) * 3.6)
                                    .max(0.1);
                            self.entities[i].tactical_until =
                                self.tick + (seconds / DT) as u64 + 60;
                        }
                    }
                }
            }
        }
    }
    fn warn_incoming(&mut self, p: &Projectile) {
        for e in &mut self.entities {
            if e.view.owner >= 0
                && e.view.owner != p.owner
                && e.view.category == "troop"
                && !e.view.downed
                && e.view.hp > 0.
                && segment_distance(position(e), p.from, p.to) < p.radius.max(3.)
            {
                e.threat = Some(p.from);
                e.threat_until = self.tick + 40;
            }
        }
    }
    fn separate_units(&mut self) {
        // Soft steering: only moving land units yield, never push idle defenders into water.
        let points: Vec<_> = self.entities.iter().map(position).collect();
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            if e.view.building
                || e.view.downed
                || e.transport.is_some()
                || e.view.airborne
                || flag(spec(&e.view.kind), "airUnit")
                || flag(spec(&e.view.kind), "waterUnit")
                || e.speed <= 0.01
                || e.view.garrison.is_some()
            {
                continue;
            }
            let at = points[i];
            let mut shift = [0., 0.];
            for (j, p) in points.iter().enumerate() {
                let other = &self.entities[j];
                if i == j
                    || other.view.building
                    || other.view.airborne
                    || other.transport.is_some()
                    || other.view.downed
                    || other.view.garrison.is_some()
                {
                    continue;
                }
                let d = distance(at, *p);
                let clearance = if e.view.category == "vehicle" {
                    2.5
                } else {
                    0.4
                } + if other.view.category == "vehicle" {
                    2.5
                } else {
                    0.4
                };
                if d < clearance {
                    let direction = if d > 0.001 {
                        [(at[0] - p[0]) / d, (at[1] - p[1]) / d]
                    } else {
                        [if i < j { -1. } else { 1. }, 0.]
                    };
                    let amount = ((clearance - d) * 0.06).min(0.18);
                    shift[0] += direction[0] * amount;
                    shift[1] += direction[1] * amount;
                }
            }
            let next = [at[0] + shift[0], at[1] + shift[1]];
            if self.clear_segment_radius(
                at,
                next,
                0,
                body_clearance(&e.view.kind, e.view.model_scale),
            ) {
                self.entities[i].view.x = next[0];
                self.entities[i].view.z = next[1];
            }
        }
    }
    pub(crate) fn smoke_blast(&mut self, at: Point) {
        self.impact(&Projectile {
            owner: 0,
            from: [at[0] - 12., at[1]],
            to: at,
            born: self.tick,
            impact: self.tick,
            damage: 4.,
            radius: 2.5,
            hit: true,
            weapon: "long".into(),
            ground_hit: true,
        });
    }
    pub(crate) fn smoke_ground_impact(&mut self, at: Point, weapon: &str) {
        self.impact(&Projectile {
            owner: 0,
            from: [at[0] - 18., at[1] - 7.],
            to: at,
            born: self.tick,
            impact: self.tick,
            damage: 3.,
            radius: number(spec(weapon), "damageRadius", 2.5),
            hit: true,
            weapon: weapon.into(),
            ground_hit: true,
        });
        self.events.clear();
    }
    pub(crate) fn smoke_tank_hit(&mut self, at: Point, kill: bool, owner: i32) {
        self.impact(&Projectile {
            owner,
            from: [at[0] - 12., at[1]],
            to: at,
            born: self.tick,
            impact: self.tick,
            damage: if kill { 200. } else { 1. },
            radius: number(spec("medium"), "damageRadius", 4.),
            hit: true,
            weapon: "medium".into(),
            ground_hit: false,
        });
        self.record_wrecks();
        self.entities.retain(|e| e.view.hp > 0.);
    }
    pub(crate) fn smoke_destroy_models(&mut self) {
        let targets: Vec<_> = self
            .entities
            .iter()
            .filter(|e| e.view.id.starts_with("destruction-check-"))
            .map(position)
            .collect();
        for at in targets {
            self.impact(&Projectile {
                owner: 0,
                from: [at[0] - 12., at[1]],
                to: at,
                born: self.tick,
                impact: self.tick,
                damage: 100.,
                radius: 0.35,
                hit: true,
                weapon: String::new(),
                ground_hit: false,
            });
        }
        self.record_wrecks();
        self.entities.retain(|e| e.view.hp > 0.);
    }
    fn impact(&mut self, p: &Projectile) {
        // Small arms can suppress occupants, but barely damage masonry.
        let structure_damage = p.damage
            * if spec(&p.weapon)["category"] == "troop" {
                0.1
            } else {
                1.
            };
        let terrain_radius = number(spec(&p.weapon), "terrainDamageRadius", 0.).clamp(0., 3.);
        let mark_seed = (self.tick as u32).wrapping_mul(2654435761)
            ^ p.to[0].to_bits().rotate_left(7)
            ^ p.to[1].to_bits().rotate_left(17)
            ^ self.props.len() as u32;
        let mark = crate::ground_impacts::Mark::for_weapon(&p.weapon, terrain_radius, mark_seed);
        let contact_radius = terrain_radius.max(0.35);
        let scar_allowed = p.ground_hit
            && mark.is_some()
            && !self.terrain.cell(p.to[0], p.to[1]).water()
            && !self.entities.iter().any(|e| {
                if e.view.airborne || e.transport.is_some() {
                    return false;
                }
                if !e.view.lots.is_empty() {
                    let local = [p.to[0] - e.view.x, p.to[1] - e.view.z];
                    return e.view.lots.iter().any(|l| {
                        l.integrity > 0. && distance(local, l.nearest(local)) <= contact_radius
                    });
                }
                if e.view.category == "vehicle" {
                    return distance(position(e), p.to)
                        <= contact_radius + e.view.model_scale * 3.5;
                }
                e.view.building
                    && !["objective", "bridge"].contains(&e.view.kind.as_str())
                    && distance(position(e), p.to)
                        <= contact_radius
                            + number(spec(&e.view.kind), "footprintWidth", 4.).max(number(
                                spec(&e.view.kind),
                                "footprintDepth",
                                4.,
                            )) * e.view.model_scale
                                * 0.6
            })
            && !self.props.iter().any(|w| {
                w.grounded()
                    && w.wreck.is_some()
                    && distance(w.nearest(p.to), p.to) <= contact_radius
            });
        let target_health: f32 = self
            .entities
            .iter()
            .filter(|e| e.view.training_target)
            .map(|e| e.view.hp.max(0.))
            .sum();
        self.warn_incoming(p);
        let explosive = p.radius >= 1.;
        let contact_height = if p.weapon == "plane" || p.weapon == "medium" || p.weapon.starts_with("c130-") {
            Some(crate::runtime::cannon_contact_height(
                self.entities.iter().map(|e| &e.view),
                p.to,
                p.owner,
                self.terrain.height(p.to[0], p.to[1]),
            ))
        } else if explosive {
            let ground = self.terrain.height(p.to[0], p.to[1]);
            let surface = crate::runtime::structure_contact_height(
                self.entities.iter().map(|e| &e.view),
                p.to,
                ground,
            );
            (surface > ground + 0.2).then_some(surface)
        } else {
            None
        };
        self.events.push(BattleEvent {
            hit: None,
            target: None,
            tick: self.tick,
            kind: if explosive && !["plane", "medium"].contains(&p.weapon.as_str()) {
                "blast"
            } else {
                "impact"
            }
            .into(),
            at: p.to,
            from: Some(p.from),
            owner: p.owner,
            radius: p.radius.max(0.4),
            weapon: p.weapon.clone(),
            source: String::new(),
            duration: 0.,
            release_height: contact_height,
        });
        if p.hit {
            for e in &mut self.entities {
                if e.view.lots.is_empty() || e.view.owner == p.owner {
                    continue;
                }
                let local = [p.to[0] - e.view.x, p.to[1] - e.view.z];
                for (index, lot) in e.view.lots.iter_mut().enumerate() {
                    if lot.integrity > 0. && distance(local, lot.nearest(local)) <= p.radius + 0.1 {
                        let d = distance(local, lot.nearest(local));
                        lot.integrity = (lot.integrity
                            - structure_damage / lot.max_hp()
                                * crate::destruction::blast_falloff(d, p.radius))
                        .max(0.);
                        if lot.integrity <= 0. {
                            self.events.push(BattleEvent {
                                hit: None,
                                target: None,
                                tick: self.tick,
                                kind: "collapse".into(),
                                at: [e.view.x + lot.at[0], e.view.z + lot.at[1]],
                                from: Some(p.from),
                                owner: e.view.owner,
                                radius: lot.width.min(lot.depth) * 0.45,
                                weapon: format!("house:{index}"),
                                source: e.view.id.clone(),
                                duration: 0.,
                                release_height: None,
                            });
                        }
                    }
                }
                e.view.hp = e
                    .view
                    .lots
                    .iter()
                    .map(|l| l.max_hp() * l.integrity)
                    .sum::<f32>()
                    .max(0.01);
            }
        }
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            if !e.view.lots.is_empty()
                || e.view.owner < 0
                || e.view.owner == p.owner
                || e.view.kind == "objective"
                || e.transport.is_some()
                || e.view.hp <= 0.
                || !weapon_can_damage(&p.weapon, &e.view)
            {
                continue;
            }
            let d = if explosive && e.view.building {
                let (s, c) = e.view.heading.sin_cos();
                let (dx, dz) = (p.to[0] - e.view.x, p.to[1] - e.view.z);
                let half_x =
                    number(spec(&e.view.kind), "footprintWidth", 3.6) * e.view.model_scale * 0.5;
                let half_z =
                    number(spec(&e.view.kind), "footprintDepth", 3.6) * e.view.model_scale * 0.5;
                ((c * dx - s * dz).abs() - half_x)
                    .max(0.)
                    .hypot(((s * dx + c * dz).abs() - half_z).max(0.))
            } else {
                distance(position(e), p.to)
            };
            let d = if e.view.airborne && !flag(spec(&p.weapon), "antiAir") {
                d.hypot(e.view.flight_height())
            } else {
                d
            };
            let cover = if e
                .view
                .garrison
                .as_ref()
                .is_some_and(|g| g.entered && self.garrison_slot(g).is_some())
            {
                2
            } else {
                self.cover_level(position(e), Some(p.from))
            };
            let protection = if explosive {
                if cover == 2 { 0.75 } else { 1. }
            } else {
                match cover {
                    2 => 0.38,
                    1 => 0.7,
                    _ => 1.,
                }
            };
            // Roll only for an actual support-gun vehicle hit, never a miss or
            // an infantry/building hit. A regular hit cannot destroy a fresh vehicle.
            let vehicle_damage = if p.weapon == "long"
                && p.hit
                && d <= p.radius
                && e.view.category == "vehicle"
                && !e.view.airborne
            {
                let max_hp = e.view.max_hp;
                let critical = self.random() < number(spec("long"), "vehicleCriticalChance", 0.2);
                Some(if critical {
                    max_hp
                } else {
                    number(spec("long"), "vehicleDamage", 0.75)
                })
            } else {
                None
            };
            let e = &mut self.entities[i];
            if e.view.category == "troop" && !e.view.downed && d <= p.radius.max(2.8) {
                e.view.suppression = (e.view.suppression
                    + (if explosive { 0.5 } else { 0.24 }) * protection)
                    .min(1.);
                e.last_hit = self.tick;
            }
            if p.hit && d <= p.radius {
                let amount = vehicle_damage.unwrap_or(if e.view.building {
                    structure_damage
                } else {
                    p.damage
                }) * protection
                    * crate::destruction::blast_falloff(d, p.radius)
                    * if e.view.retreating { 0.65 } else { 1. };
                damage(&mut e.view, amount);
                if e.view.hp <= 0.
                    && !e.view.downed
                    && !e.view.training_target
                    && ["short", "scout", "engineer"].contains(&e.view.kind.as_str())
                {
                    e.view.downed = true;
                    e.view.hp = 0.01;
                    e.view.strength = 0;
                    e.downed_until = self.tick + 400;
                    e.orders.clear();
                    e.path.clear();
                    e.speed = 0.;
                    e.view.velocity = [0., 0.];
                }
            }
        }
        if p.owner == 0
            && let Some(training) = &mut self.training
        {
            let remaining: f32 = self
                .entities
                .iter()
                .filter(|e| e.view.training_target)
                .map(|e| e.view.hp.max(0.))
                .sum();
            let dealt = (target_health - remaining).max(0.);
            if dealt > 0. {
                training.hits += 1;
                training.damage += dealt;
            }
        }
        if explosive {
            for object in &mut self.props {
                let d = distance(object.nearest(p.to), p.to);
                if object.hp > 0. && d <= p.radius + 1.5 {
                    object.hp = (object.hp
                        - p.damage * crate::destruction::blast_falloff(d, p.radius + 1.5))
                    .max(0.);
                    if object.hp <= 0. {
                        self.events.push(BattleEvent {
                            hit: None,
                            target: None,
                            tick: self.tick,
                            kind: "collapse".into(),
                            at: object.at,
                            from: Some(p.from),
                            owner: p.owner,
                            radius: 1.2,
                            weapon: object.kind.clone(),
                            source: object.id.clone(),
                            duration: 0.,
                            release_height: None,
                        });
                    }
                }
            }
            if scar_allowed
                && self.props.len() < 2000
                && let Some(mark) = mark
                && !self.props.iter().any(|o| {
                    o.kind == "crater" && distance(o.at, p.to) < mark.radius * 0.30
                        && (crate::ground_impacts::Mark::from_prop(o).excavated || !mark.excavated)
                })
            {
                // A heavy hit replaces a shallow shell scar at the same point.
                self.props.retain(|o| !(o.kind == "crater"
                    && !crate::ground_impacts::Mark::from_prop(o).excavated
                    && distance(o.at, p.to) < mark.radius * 0.30));
                // Keep long battles bounded; saved terrain depressions remain.
                if self.props.iter().filter(|p| p.kind == "crater").count() >= 512
                    && let Some(index) = self.props.iter().position(|p| p.kind == "crater") {
                    self.props.remove(index);
                }
                self.props.push(BattlefieldProp {
                    scar: Some(mark),
                    id: format!("crater-{}-{:08x}-{:08x}-{mark_seed}", self.tick,
                        p.to[0].to_bits(), p.to[1].to_bits()),
                    kind: "crater".into(),
                    at: p.to,
                    heading: (p.to[0] - p.from[0]).atan2(p.to[1] - p.from[1])
                        + (mark.seed % 4096) as f32 * 0.00153398,
                    wreck: None,
                    hp: 0.,
                });
                if terrain_radius > 0. {
                    self.terrain.crater(p.to[0], p.to[1], terrain_radius.ceil() as i32);
                }
            }
        }
    }
    fn aircraft_crashes(&mut self) {
        let mut impacts = vec![];
        for prop in &mut self.props {
            let Some(wreck) = &mut prop.wreck else {
                continue;
            };
            let Some(crash) = &mut wreck.crash else {
                continue;
            };
            if crash.advance(&mut prop.at, &mut prop.heading, &self.terrain, DT) {
                wreck.crash = None;
                wreck.born = self.tick;
                wreck.piece.airborne = false;
                wreck.piece.altitude = None;
                wreck.piece.x = prop.at[0];
                wreck.piece.z = prop.at[1];
                wreck.piece.heading = std::f32::consts::FRAC_PI_2 - prop.heading;
                wreck.piece.elevation = self.terrain.height(prop.at[0], prop.at[1]);
                impacts.push((prop.at, wreck.piece.owner, wreck.piece.id.clone()));
            }
        }
        for (at, owner, source) in impacts {
            let event = self.events.len();
            self.impact(&Projectile {
                owner,
                from: at,
                to: at,
                born: self.tick,
                impact: self.tick,
                damage: 2.5,
                radius: 5.5,
                hit: true,
                weapon: "helicopter".into(),
                ground_hit: false,
            });
            self.events[event].kind = "crash-impact".into();
            self.events[event].source = source;
        }
    }
    fn record_wrecks(&mut self) {
        for e in &self.entities {
            if e.view.hp <= 0.
                && (e.view.building
                    || e.view.category == "vehicle"
                    || e.view.category == "aircraft")
                && e.view.kind != "objective"
                && !self
                    .props
                    .iter()
                    .any(|p| p.id == format!("wreck-{}", e.view.id))
            {
                if self.props.len() >= 2048 {
                    if let Some(index) = self.props.iter().position(|p| p.kind == "crater") {
                        self.props.remove(index);
                    } else {
                        continue;
                    }
                }
                let mut piece = e.view.clone();
                piece.bridge_end = e.bridge_end;
                piece.aim = e
                    .target
                    .as_ref()
                    .and_then(|id| {
                        self.entities
                            .iter()
                            .find(|t| &t.view.id == id)
                            .map(position)
                    })
                    .or(piece.aim);
                let center = piece.position();
                let at = [center.x, center.z];
                let heading = crate::destruction::heading(&piece);
                let radius = if piece.building {
                    3.5
                } else {
                    (piece.model_scale * 1.1).clamp(1., 4.)
                };
                self.events.push(BattleEvent {
                    hit: None,
                    target: None,
                    tick: self.tick,
                    kind: if piece.category == "aircraft"
                        && piece.airborne
                        && piece.flight_height() > 0.5
                    {
                        "aircraft-hit"
                    } else {
                        "destruction"
                    }
                    .into(),
                    at,
                    from: None,
                    owner: piece.owner,
                    radius,
                    weapon: piece.kind.clone(),
                    source: piece.id.clone(),
                    duration: 0.,
                    release_height: (piece.category == "aircraft" && piece.airborne)
                        .then_some(center.y),
                });
                self.props.push(BattlefieldProp {
                    scar: None,
                    id: format!("wreck-{}", e.view.id),
                    kind: "wreck".into(),
                    at,
                    heading,
                    wreck: Some(crate::destruction::Wreck::new(piece, self.tick)),
                    hp: 0.,
                });
            }
        }
    }
}

#[cfg(test)]
mod tactical_tests {
    use super::*;
    #[test]
    fn ground_scars_preserve_weapon_size_save_data_and_never_mark_water_or_armour() {
        let mut g = NativeGame::operation(crate::training::ID, 384, 384, default_config());
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        let original_revision = g.terrain.revision;
        g.smoke_ground_impact([190., 190.], "medium");
        assert_eq!(g.props.len(), 1);
        assert_eq!(g.terrain.revision, original_revision);
        assert!(!g.props[0].scar.unwrap().excavated);
        g.smoke_ground_impact([190., 190.], "long");
        assert_eq!(g.props.len(), 1, "heavy soil replaces the smaller shell scar");
        assert!(g.props[0].scar.unwrap().excavated);
        let revision = g.terrain.revision;
        g.smoke_ground_impact([190., 190.], "long");
        assert_eq!(g.props.len(), 1);
        assert_eq!(g.terrain.revision, revision);
        g.smoke_ground_impact([202., 190.], "xtralong");
        assert!(g.props[1].scar.unwrap().radius > g.props[0].scar.unwrap().radius);
        let saved: NativeGame = serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&saved.props).unwrap(), serde_json::to_value(&g.props).unwrap());
        g.smoke_ground_impact([316., 314.], "long");
        assert_eq!(g.props.len(), 2, "water must not grow a soil scar");
        g.spawn("medium", 1, [230., 190.]);
        g.smoke_ground_impact([230., 190.], "long");
        assert_eq!(g.props.len(), 2, "armour contact must not grow a ground scar");
    }
    #[test]
    fn persistent_ground_scar_budget_keeps_unique_ids_and_retires_the_oldest() {
        let mut g = NativeGame::operation(crate::training::ID, 384, 384, default_config());
        g.entities.clear();
        g.props.clear();
        for i in 0..520 {
            g.smoke_ground_impact([120. + (i % 26) as f32 * 4., 30. + (i / 26) as f32 * 4.], "medium");
        }
        assert_eq!(g.props.len(), 512);
        let ids: std::collections::HashSet<_> = g.props.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.len(), g.props.len());
        assert_eq!(g.props[0].at, [152., 30.]);
    }
    #[test]
    fn tank_rounds_emit_small_impacts_and_vehicle_kills_emit_one_destruction() {
        let mut g = game();
        g.entities.clear();
        g.props.clear();
        g.events.clear();
        g.spawn("medium", 1, [40., 40.]);
        g.entities[0].view.hp = 100.;
        g.smoke_tank_hit([40., 40.], false, 0);
        assert_eq!(g.entities[0].view.hp, 99.);
        assert_eq!(
            g.events
                .iter()
                .filter(|e| e.kind == "impact" && e.weapon == "medium")
                .count(),
            1
        );
        assert!(
            !g.events
                .iter()
                .any(|e| e.kind == "blast" || e.kind == "destruction")
        );
        assert!(g.events[0].release_height.unwrap() > g.terrain.height(40., 40.) + 0.5);
        g.smoke_tank_hit([40., 40.], true, 0);
        assert_eq!(
            g.events.iter().filter(|e| e.kind == "destruction").count(),
            1
        );
        assert!(!g.events.iter().any(|e| e.kind == "blast"));
    }
    #[test]
    fn a_building_blast_keeps_its_roof_contact_when_the_same_hit_collapses_it() {
        let mut g = game();
        g.entities.clear();
        g.props.clear();
        g.events.clear();
        g.spawn("village", -1, [30., 30.]);
        g.entities[0].view.lots = vec![crate::settlement_layout::Lot {
            at: [0., 0.],
            width: 10.,
            depth: 10.,
            height: 3.2,
            yaw: 0.,
            style: 0,
            ground: 0.,
            integrity: 1.,
        }];
        let roof = crate::runtime::structure_contact_height(
            g.entities.iter().map(|e| &e.view),
            [30., 30.],
            g.terrain.height(30., 30.),
        );
        assert!(roof > g.terrain.height(30., 30.) + 3.2);
        g.impact(&Projectile {
            owner: 0,
            from: [30., 10.],
            to: [30., 30.],
            born: g.tick,
            impact: g.tick,
            damage: 100.,
            radius: 2.5,
            hit: true,
            weapon: "long".into(),
            ground_hit: false,
        });
        assert_eq!(g.entities[0].view.lots[0].integrity, 0.);
        assert_eq!(
            g.events
                .iter()
                .find(|e| e.kind == "blast")
                .unwrap()
                .release_height,
            Some(roof)
        );
    }
    #[test]
    fn helicopter_crash_retains_height_and_momentum_then_explodes_once_on_contact() {
        let mut g = game();
        g.entities.clear();
        g.props.clear();
        let id = g.spawn("helicopter", 1, [40., 40.]);
        let ground = g.terrain.height(40., 40.);
        g.entities[0].view.airborne = true;
        g.entities[0].view.altitude = Some(24.);
        g.entities[0].view.velocity = [5., 1.];
        g.entities[0].view.hp = 0.;
        g.record_wrecks();
        g.record_wrecks();
        assert_eq!(g.props.len(), 1);
        let initial = g.props[0]
            .wreck
            .as_ref()
            .unwrap()
            .crash
            .as_ref()
            .unwrap()
            .world_y;
        assert!((initial - ground - 24.).abs() < 0.001);
        assert!(!g.props[0].blocks([40., 40.]));
        assert_eq!(g.cover_level([40., 40.], None), 0);
        assert_eq!(
            g.events.iter().filter(|e| e.kind == "aircraft-hit").count(),
            1
        );
        assert!(!g.events.iter().any(|e| e.kind == "crash-impact"));
        for _ in 0..10 {
            g.tick += 1;
            g.aircraft_crashes();
        }
        assert!(g.props[0].at[0] > 42.);
        let falling = g.props[0].wreck.as_ref().unwrap().crash.as_ref().unwrap();
        assert!(
            falling.world_y < initial && falling.vertical_speed < -5. && falling.roll.abs() > 0.1
        );
        let mut saved: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        saved.terrain.hydrate();
        saved.validate().unwrap();
        let mut victory = saved.clone();
        victory.winner = Some(0);
        victory.paused = false;
        for _ in 0..180 {
            victory.step();
        }
        assert!(victory.props[0].wreck.as_ref().unwrap().crash.is_none());
        assert_eq!(victory.winner, Some(0));
        assert_eq!(
            victory
                .events
                .iter()
                .filter(|e| e.kind == "crash-impact")
                .count(),
            1
        );
        for _ in 0..180 {
            g.tick += 1;
            saved.tick += 1;
            g.aircraft_crashes();
            saved.aircraft_crashes();
        }
        assert!(g.props[0].wreck.as_ref().unwrap().crash.is_none());
        assert!(g.props[0].blocks(g.props[0].at));
        let explosion = g
            .events
            .iter()
            .filter(|e| e.kind == "crash-impact")
            .collect::<Vec<_>>();
        assert_eq!(explosion.len(), 1);
        assert_eq!(explosion[0].source, id);
        assert_eq!(explosion[0].at, g.props[0].at);
        assert!(explosion[0].radius >= 5.);
        assert_eq!(
            serde_json::to_value(&g.props).unwrap(),
            serde_json::to_value(&saved.props).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&g.events).unwrap(),
            serde_json::to_value(&saved.events).unwrap()
        );
        assert!(g.terrain.edits.is_empty());
    }
    #[test]
    fn only_heavy_ground_impacts_deform_terrain_and_never_dig_under_tanks() {
        let mut g = game();
        g.props.clear();
        g.terrain.edits.insert(
            "30,30".into(),
            crate::terrain::Cell {
                elevation: 0.5,
                surface: 0.5,
                moisture: 0.5,
                kind: 4,
            },
        );
        let original = g.terrain.cell(30., 30.).elevation;
        let mut shell = Projectile {
            owner: 0,
            from: [20., 30.],
            to: [30., 30.],
            born: 0,
            impact: 0,
            damage: 2.,
            radius: 3.5,
            hit: true,
            weapon: "medium".into(),
            ground_hit: true,
        };
        g.impact(&shell);
        assert_eq!(g.terrain.cell(30., 30.).elevation, original);
        shell.weapon = "long".into();
        shell.ground_hit = false;
        g.impact(&shell);
        assert_eq!(g.terrain.cell(30., 30.).elevation, original);
        shell.ground_hit = true;
        g.spawn("medium", 1, [30., 30.]);
        g.impact(&shell);
        assert_eq!(g.terrain.cell(30., 30.).elevation, original);
        assert!(g.entities[0].view.hp < g.entities[0].view.max_hp);
        g.entities.clear();
        g.impact(&shell);
        let crater = g.terrain.cell(30., 30.).elevation;
        assert!(crater < original);
        g.impact(&shell);
        assert_eq!(g.terrain.cell(30., 30.).elevation, crater);
        let restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        assert_eq!(restored.terrain.modifications(), g.terrain.modifications());
    }
    #[test]
    fn cover_and_individual_houses_emit_one_source_specific_collapse() {
        let mut g = game();
        g.props.clear();
        bag(&mut g, [30., 30.]);
        let blast = Projectile {
            owner: 0,
            from: [20., 30.],
            to: [30., 30.],
            born: 0,
            impact: 0,
            damage: 5.,
            radius: 2.,
            hit: true,
            weapon: "long".into(),
            ground_hit: true,
        };
        g.impact(&blast);
        g.impact(&blast);
        let collapses: Vec<_> = g.events.iter().filter(|e| e.kind == "collapse").collect();
        assert_eq!(collapses.len(), 1);
        assert_eq!(collapses[0].weapon, "sandbags");
        assert_eq!(collapses[0].source, "test-cover");
        assert_eq!(g.props.iter().filter(|p| p.kind == "crater").count(), 1);
        g.events.clear();
        let town = g.spawn("town", -1, [60., 60.]);
        let lot = &g.entities[0].view.lots[0];
        let to = [60. + lot.at[0], 60. + lot.at[1]];
        let shell = Projectile {
            owner: 0,
            from: [40., 60.],
            to,
            born: 0,
            impact: 0,
            damage: lot.max_hp(),
            radius: 0.35,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        };
        g.impact(&shell);
        g.impact(&shell);
        let collapses: Vec<_> = g.events.iter().filter(|e| e.kind == "collapse").collect();
        assert_eq!(collapses.len(), 1);
        assert_eq!(collapses[0].weapon, "house:0");
        assert_eq!(collapses[0].source, town);
    }
    #[test]
    fn vehicle_death_keeps_model_aim_scale_and_hull_cover_through_a_save() {
        let mut g = game();
        g.props.clear();
        let id = g.spawn("medium", 1, [30., 30.]);
        g.entities[0].view.hp = 0.;
        g.entities[0].view.heading = 0.7;
        g.entities[0].view.aim = Some([45., 35.]);
        g.record_wrecks();
        g.record_wrecks();
        assert_eq!(g.props.len(), 1);
        assert_eq!(
            g.events.iter().filter(|e| e.kind == "destruction").count(),
            1
        );
        let prop = &g.props[0];
        assert_eq!(prop.id, format!("wreck-{id}"));
        let w = prop.wreck.as_ref().unwrap();
        assert_eq!(w.piece.kind, "medium");
        assert_eq!(w.piece.aim, Some([45., 35.]));
        assert_eq!(w.piece.model_scale, 2.25);
        assert_eq!(prop.heading, std::f32::consts::FRAC_PI_2 - 0.7);
        assert!(!g.passable([30., 30.], false));
        assert!(g.passable([30., 30.], true));
        let (s, c) = prop.heading.sin_cos();
        let target = [
            30. + s * (w.half_extents[1] + 0.8),
            30. + c * (w.half_extents[1] + 0.8),
        ];
        let shooter = [30. - s * 20., 30. - c * 20.];
        assert_eq!(g.cover_level(target, Some(shooter)), 2);
        assert_eq!(
            g.cover_level(target, Some([target[0] + s * 10., target[1] + c * 10.])),
            0
        );
        let restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&g.props).unwrap(),
            serde_json::to_value(&restored.props).unwrap()
        );
        assert_eq!(restored.cover_level(target, Some(shooter)), 2);
        let old: BattlefieldProp = serde_json::from_value(
            json!({"id":"old-wreck","kind":"wreck","at":[40.,40.],"heading":0.,"hp":0.}),
        )
        .unwrap();
        assert!(old.wreck.is_none());
    }
    #[test]
    fn collapsed_facility_and_aircraft_keep_their_own_models_and_rubble_is_passable() {
        let mut g = game();
        g.props.clear();
        g.spawn("vehicleDepot", 1, [30., 30.]);
        g.spawn("helicopter", 1, [60., 30.]);
        for e in &mut g.entities {
            e.view.hp = 0.;
        }
        g.record_wrecks();
        assert_eq!(g.props.len(), 2);
        assert!(g.passable([30., 30.], false));
        assert!(g.props[0].wreck.as_ref().unwrap().piece.building);
        assert_eq!(g.props[1].wreck.as_ref().unwrap().piece.kind, "helicopter");
        assert!(!g.passable([60., 30.], false));
        g.validate().unwrap();
    }
    #[test]
    fn explosive_damage_falls_off_and_hits_the_edge_of_large_structures() {
        let mut g = game();
        g.props.clear();
        g.spawn("medium", 1, [30., 30.]);
        g.spawn("medium", 1, [32.8, 30.]);
        for e in &mut g.entities {
            e.view.hp = 10.;
            e.view.max_hp = 10.;
        }
        g.impact(&Projectile {
            owner: 0,
            from: [20., 30.],
            to: [30., 30.],
            born: 0,
            impact: 0,
            damage: 2.,
            radius: 3.,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        assert_eq!(g.entities[0].view.hp, 8.);
        assert!(g.entities[1].view.hp > 9.);
        g.entities.clear();
        g.spawn("vehicleDepot", 1, [60., 60.]);
        g.impact(&Projectile {
            owner: 0,
            from: [40., 60.],
            to: [54., 60.],
            born: 0,
            impact: 0,
            damage: 3.,
            radius: 2.,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        assert!(g.entities[0].view.hp > 0. && g.entities[0].view.hp < g.entities[0].view.max_hp);
    }
    #[test]
    fn incoming_fire_seeks_cover_before_impact_and_preserves_queued_attack_orders() {
        let mut g = game();
        let id = g.spawn("short", 1, [30., 30.]);
        bag(&mut g, [33., 30.]);
        g.issue(
            1,
            std::slice::from_ref(&id),
            Order {
                kind: "attack-move".into(),
                destination: Some([50., 30.]),
                target: None,
            },
            false,
        );
        g.warn_incoming(&Projectile {
            owner: 0,
            from: [20., 30.],
            to: [30., 30.],
            born: 0,
            impact: 20,
            damage: 1.,
            radius: 0.2,
            hit: false,
            weapon: String::new(),
            ground_hit: false,
        });
        g.tick = 4;
        g.tactical_step();
        assert!(g.entities[0].cover_goal.is_some());
        assert_eq!(g.entities[0].view.hp, 3.);
        assert!(g.snapshot_for(1).entities[0].under_fire);
        assert_eq!(g.snapshot_for(1).entities[0].activity, "seeking-cover");
        g.issue(
            1,
            &[id],
            Order {
                kind: "attack-move".into(),
                destination: Some([60., 30.]),
                target: None,
            },
            true,
        );
        for tick in 5..=39 {
            g.tick = tick;
            g.tactical_step();
            g.movement();
        }
        assert_eq!(g.entities[0].orders.len(), 2);
        g.tick = 41;
        g.tactical_step();
        assert!(g.entities[0].cover_goal.is_none());
        assert_eq!(g.entities[0].orders[0].destination, Some([50., 30.]));
        assert_eq!(g.entities[0].orders[1].destination, Some([60., 30.]));
        assert!(!g.entities[0].path.is_empty());
    }
    #[test]
    fn explicit_move_keeps_control_while_idle_medics_seek_cover() {
        let mut g = game();
        let id = g.spawn("short", 1, [30., 30.]);
        bag(&mut g, [33., 30.]);
        g.issue(
            1,
            &[id],
            Order {
                kind: "move".into(),
                destination: Some([50., 30.]),
                target: None,
            },
            false,
        );
        g.warn_incoming(&Projectile {
            owner: 0,
            from: [20., 30.],
            to: [30., 30.],
            born: 0,
            impact: 20,
            damage: 1.,
            radius: 0.2,
            hit: false,
            weapon: String::new(),
            ground_hit: false,
        });
        g.tick = 4;
        g.tactical_step();
        assert!(g.entities[0].cover_goal.is_none());
        assert_eq!(g.entities[0].orders[0].kind, "move");
        g.spawn("medic", 1, [30., 34.]);
        bag(&mut g, [33., 34.]);
        g.warn_incoming(&Projectile {
            owner: 0,
            from: [20., 34.],
            to: [30., 34.],
            born: 4,
            impact: 24,
            damage: 1.,
            radius: 0.2,
            hit: false,
            weapon: String::new(),
            ground_hit: false,
        });
        g.tick = 8;
        g.tactical_step();
        assert!(g.entities[1].cover_goal.is_some());
    }
    #[test]
    fn forward_base_restores_authored_terrain_and_keeps_enemy_orders_private() {
        let operation = NativeGame::default();
        assert_eq!(operation.terrain.id, "forward-base");
        assert!(operation.terrain.cell(64.5, 64.5).elevation > 0.4);
        let mut restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&operation).unwrap()).unwrap();
        restored.validate().unwrap();
        restored.terrain.hydrate();
        assert_eq!(
            operation.terrain.cell(63.2, 51.7),
            restored.terrain.cell(63.2, 51.7)
        );
        let mut g = game();
        let own = g.spawn("short", 0, [30., 30.]);
        let enemy = g.spawn("short", 1, [35., 30.]);
        g.issue(
            1,
            std::slice::from_ref(&enemy),
            Order {
                kind: "move".into(),
                destination: Some([42., 30.]),
                target: None,
            },
            false,
        );
        g.issue(
            0,
            &[own],
            Order {
                kind: "move".into(),
                destination: Some([32., 30.]),
                target: None,
            },
            false,
        );
        let snapshot = g.snapshot_for(0);
        assert!(
            snapshot
                .entities
                .iter()
                .find(|p| p.id == enemy)
                .unwrap()
                .orders
                .is_empty()
        );
        assert!(
            !snapshot
                .entities
                .iter()
                .find(|p| p.owner == 0)
                .unwrap()
                .orders
                .is_empty()
        );
    }
    fn game() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.config["fogEnabled"] = json!(false);
        g.human_opponent = true;
        g
    }
    fn bag(g: &mut NativeGame, at: Point) {
        g.props.push(BattlefieldProp {
            scar: None,
            id: "test-cover".into(),
            kind: "sandbags".into(),
            at,
            heading: std::f32::consts::FRAC_PI_2,
            wreck: None,
            hp: 4.,
        });
    }
    #[test]
    fn group_destinations_are_spaced_and_enemy_commands_are_rejected() {
        let mut g = game();
        let a = g.spawn("short", 0, [20., 30.]);
        let b = g.spawn("short", 0, [20., 34.]);
        let enemy = g.spawn("short", 1, [21., 32.]);
        let n = g.issue(
            0,
            &[a, b, enemy],
            Order {
                kind: "move".into(),
                destination: Some([40., 30.]),
                target: None,
            },
            false,
        );
        assert_eq!(n, 2);
        assert!(
            distance(
                g.entities[0].orders[0].destination.unwrap(),
                g.entities[1].orders[0].destination.unwrap()
            ) >= 1.35
        );
        assert!(g.entities[2].orders.is_empty());
    }
    #[test]
    fn group_cover_orders_use_separate_slots() {
        let mut g = game();
        let a = g.spawn("short", 0, [20., 30.]);
        let b = g.spawn("short", 0, [20., 34.]);
        bag(&mut g, [40., 30.]);
        assert_eq!(
            g.issue(
                0,
                &[a, b],
                Order {
                    kind: "move".into(),
                    destination: Some([40., 30.]),
                    target: None
                },
                false
            ),
            2
        );
        assert!(
            distance(
                g.entities[0].orders[0].destination.unwrap(),
                g.entities[1].orders[0].destination.unwrap()
            ) >= 1.35
        );
    }
    #[test]
    fn directional_cover_reduces_damage_but_flanking_bypasses_it() {
        let mut g = game();
        g.spawn("short", 1, [31., 30.]);
        bag(&mut g, [30., 30.]);
        assert_eq!(g.cover_level([31., 30.], Some([20., 30.])), 2);
        assert_eq!(g.cover_level([31., 30.], Some([40., 30.])), 0);
        g.impact(&Projectile {
            owner: 0,
            from: [20., 30.],
            to: [31., 30.],
            born: 0,
            impact: 0,
            damage: 1.,
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        assert!((g.entities[0].view.hp - 2.62).abs() < 0.0001);
    }
    #[test]
    fn misses_suppress_without_damage_then_recover_and_retreat_to_base() {
        let mut g = game();
        let id = g.spawn("short", 1, [31., 30.]);
        g.spawn("barracks", 1, [45., 30.]);
        for _ in 0..4 {
            g.impact(&Projectile {
                owner: 0,
                from: [20., 30.],
                to: [31., 30.],
                born: 0,
                impact: 0,
                damage: 1.,
                radius: 0.2,
                hit: false,
                weapon: String::new(),
                ground_hit: false,
            });
        }
        assert_eq!(g.entities[0].view.hp, 3.);
        assert!(g.entities[0].view.suppression > 0.8);
        assert_eq!(
            g.issue(
                1,
                &[id],
                Order {
                    kind: "retreat".into(),
                    destination: None,
                    target: None
                },
                false
            ),
            1
        );
        for _ in 0..180 {
            g.tick += 1;
            g.tactical_step();
            g.movement();
        }
        assert!(!g.entities[0].view.retreating);
        assert!(distance(position(&g.entities[0]), [45., 30.]) < 10.);
        assert!(!g.building_contains(&g.entities[1], position(&g.entities[0]), 0.25));
    }
    #[test]
    fn medic_rescues_downed_squad_and_bleedout_is_finite() {
        let mut g = game();
        g.spawn("short", 1, [31., 30.]);
        g.spawn("medic", 1, [34., 30.]);
        g.impact(&Projectile {
            owner: 0,
            from: [20., 30.],
            to: [31., 30.],
            born: 0,
            impact: 0,
            damage: 4.,
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        assert!(g.entities[0].view.downed);
        g.economy_support();
        assert!(g.entities[0].view.downed);
        g.tick += 40;
        g.economy_support();
        assert!(!g.entities[0].view.downed);
        assert!(g.entities[0].view.hp > 0.);
        g.impact(&Projectile {
            owner: 0,
            from: [20., 30.],
            to: [31., 30.],
            born: 0,
            impact: 0,
            damage: 4.,
            radius: 0.2,
            hit: true,
            weapon: String::new(),
            ground_hit: false,
        });
        g.tick += 401;
        g.tactical_step();
        assert_eq!(g.entities[0].view.hp, 0.);
    }
    #[test]
    fn explosives_destroy_cover_deform_dry_ground_and_save_tactical_state() {
        let mut g = game();
        bag(&mut g, [30., 30.]);
        g.terrain.edits.insert(
            "30,30".into(),
            crate::terrain::Cell {
                elevation: 0.5,
                surface: 0.5,
                moisture: 0.6,
                kind: 4,
            },
        );
        let height = g.terrain.cell(30., 30.).elevation;
        g.impact(&Projectile {
            owner: 0,
            from: [20., 30.],
            to: [30., 30.],
            born: 0,
            impact: 0,
            damage: 5.,
            radius: 2.,
            hit: true,
            weapon: "long".into(),
            ground_hit: true,
        });
        assert_eq!(g.props[0].hp, 0.);
        assert!(g.terrain.cell(30., 30.).elevation < height);
        assert!(!g.terrain.cell(30., 30.).water());
        let restored: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&g.props).unwrap(),
            serde_json::to_value(restored.props).unwrap()
        );
        assert_eq!(g.terrain.modifications(), restored.terrain.modifications());
    }
}
