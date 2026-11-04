// Included in simulation.rs. Boarding uses saved orders and follows a moving carrier.
fn transport_clearance(e: &Entity) -> f32 {
    if e.view.kind == "huey" {
        2.8 * e.view.model_scale
    } else {
        body_clearance(&e.view.kind, e.view.model_scale)
    }
}
impl NativeGame {
    pub fn smoke_huey(&mut self, unload: bool, flight: bool) -> String {
        *self = Self::operation(crate::training::ID, 384, 384, default_config());
        self.entities
            .retain(|e| e.view.kind == "airfield" && e.view.owner == 0);
        self.props.clear();
        self.bases.clear();
        self.config["fogEnabled"] = json!(false);
        self.human_opponent = true;
        let base = self.entities[0].view.id.clone();
        let id = self.spawn("huey", 0, [185., 180.]);
        let i = self.entities.len() - 1;
        self.park_aircraft(i, &base, 0);
        let ground = self.terrain.height(185., 180.);
        let e = &mut self.entities[i];
        e.view.x = 185.;
        e.view.z = 180.;
        e.view.heading = 0.3;
        e.view.elevation = ground;
        e.aircraft.world_y = ground;
        e.aircraft.phase = FlightPhase::TransportGrounded;
        for n in 0..12 {
            self.spawn(
                if n == 10 {
                    "medic"
                } else if n == 11 {
                    "engineer"
                } else {
                    "short"
                },
                0,
                [166., 172. + n as f32 * 1.4],
            );
        }
        self.transport_order(0, std::slice::from_ref(&id), None, true);
        for _ in 0..500 {
            self.tick += 1;
            self.boarding_step();
            self.movement();
        }
        assert_eq!(
            self.entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&id))
                .count(),
            12
        );
        if flight {
            self.issue(
                0,
                std::slice::from_ref(&id),
                Order {
                    kind: "move".into(),
                    destination: Some([205., 190.]),
                    target: None,
                },
                false,
            );
            for _ in 0..220 {
                self.tick += 1;
                self.movement();
            }
        } else if unload {
            self.transport_order(0, std::slice::from_ref(&id), None, false);
        }
        self.paused = true;
        id
    }
    fn transport_landing_clear(&self, i: usize) -> bool {
        let e = &self.entities[i];
        let at = position(e);
        let radius = 4.2 * e.view.model_scale;
        let ground = self.terrain.height(at[0], at[1]);
        self.passable_clearance(at, 0, radius)
            && !self.terrain.cell(at[0], at[1]).water()
            && (0..8).all(|n| {
                let angle = n as f32 * std::f32::consts::TAU / 8.;
                let p = [at[0] + angle.cos() * radius, at[1] + angle.sin() * radius];
                self.passable(p, false)
                    && !self.terrain.cell(p[0], p[1]).water()
                    && (self.terrain.height(p[0], p[1]) - ground).abs() < 1.5
            })
            && !self.entities.iter().enumerate().any(|(j, u)| {
                j != i
                    && u.view.hp > 0.
                    && !u.view.building
                    && !u.view.airborne
                    && u.view.category != "troop"
                    && u.transport.is_none()
                    && distance(at, position(u))
                        < radius + body_clearance(&u.view.kind, u.view.model_scale)
            })
    }
    pub fn smoke_carrier(&mut self, unload: bool) -> String {
        *self = Self::operation(crate::training::ID, 384, 384, default_config());
        self.entities.clear();
        self.props.clear();
        self.bases.clear();
        self.config["fogEnabled"] = json!(false);
        let id = self.spawn("troopCarrier", 0, [185., 180.]);
        for n in 0..6 {
            self.spawn(
                if n == 4 {
                    "medic"
                } else if n == 5 {
                    "engineer"
                } else {
                    "short"
                },
                0,
                [166., 172. + n as f32 * 2.],
            );
        }
        self.command(json!({"type":"command","commandType":"load","unitIds":[id]}));
        for _ in 0..400 {
            self.step();
        }
        assert_eq!(
            self.entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&id))
                .count(),
            6
        );
        if unload {
            self.command(json!({"type":"command","commandType":"unload","unitIds":[id]}));
        }
        self.paused = true;
        id
    }
    fn transport_order(
        &mut self,
        owner: i32,
        ids: &[String],
        target: Option<&str>,
        load: bool,
    ) -> String {
        let carriers: Vec<_> = self
            .entities
            .iter()
            .filter(|e| {
                e.view.owner == owner
                    && transport_capacity(&e.view.kind) > 0
                    && !e.view.training_target
                    && e.view.hp > 0.
                    && (target == Some(e.view.id.as_str())
                        || target.is_none() && ids.contains(&e.view.id))
            })
            .map(|e| e.view.id.clone())
            .collect();
        let mut count = 0;
        let mut status = None;
        for carrier in carriers {
            let i = self
                .entities
                .iter()
                .position(|e| e.view.id == carrier)
                .unwrap();
            let at = position(&self.entities[i]);
            let air = flag(spec(&self.entities[i].view.kind), "airUnit");
            if air {
                if self.entities[i].aircraft.phase.returning() {
                    status = Some("Transport is returning or refuelling; wait until parked.");
                    continue;
                }
                let needs_landing = self.entities[i].view.airborne
                    || self.entities[i].aircraft.phase == FlightPhase::HeliTakeoff;
                if needs_landing {
                    if !self.transport_landing_clear(i) {
                        status =
                            Some("Landing zone blocked. Move the Huey to clear, level ground.");
                        continue;
                    }
                    let e = &mut self.entities[i];
                    e.aircraft.phase = FlightPhase::TransportLanding;
                    e.aircraft.transport_unload = !load;
                    e.orders.clear();
                    e.path.clear();
                    e.speed = 0.;
                    e.view.velocity = [0., 0.];
                    if !load {
                        status = Some("Huey landing to unload troops.");
                        continue;
                    }
                }
            }
            if load {
                // Stop at the pickup point; a later move command can still relocate the carrier.
                self.entities[i].orders.clear();
                self.entities[i].path.clear();
                self.entities[i].speed = 0.;
                let used = self
                    .entities
                    .iter()
                    .filter(|e| {
                        e.transport.as_ref() == Some(&carrier)
                            || e.orders.front().is_some_and(|o| {
                                o.kind == "board" && o.target.as_ref() == Some(&carrier)
                            })
                    })
                    .count();
                let available =
                    transport_capacity(&self.entities[i].view.kind).saturating_sub(used);
                let candidates: Vec<_> = self
                    .entities
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        e.view.owner == owner
                            && e.view.category == "troop"
                            && e.view.hp > 0.
                            && !e.view.downed
                            && !e.view.retreating
                            && e.transport.is_none()
                            && e.view.garrison.is_none()
                            && e.orders.front().is_none_or(|o| o.kind != "board")
                            && if target.is_some() {
                                ids.contains(&e.view.id)
                            } else {
                                distance(position(e), at) <= 28.
                            }
                    })
                    .map(|(j, e)| (j, distance(position(e), at)))
                    .collect();
                let mut candidates = candidates;
                candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
                for (j, _) in candidates.into_iter().take(available) {
                    let e = &mut self.entities[j];
                    e.cancel_local_actions();
                    e.release_stop();
                    e.orders = VecDeque::from([Order {
                        kind: "board".into(),
                        target: Some(carrier.clone()),
                        destination: None,
                    }]);
                    e.path.clear();
                    e.warfare = InfantryWarfare::default();
                    e.cover_goal = None;
                    e.cover_ordered = false;
                    e.tactical_until = 0;
                    e.aa_target = None;
                    e.target = None;
                    count += 1;
                }
            } else {
                // Unloading cancels the current journey and all queued waypoints.
                let e = &mut self.entities[i];
                e.orders.clear();
                e.path.clear();
                e.patrol_origin = None;
                e.target = None;
                e.speed = 0.;
                e.view.velocity = [0., 0.];
                e.stationary = 0;
                for e in &mut self.entities {
                    if e.orders
                        .front()
                        .is_some_and(|o| o.kind == "board" && o.target.as_ref() == Some(&carrier))
                    {
                        e.orders.clear();
                        e.path.clear();
                    }
                }
                let passengers: Vec<_> = self
                    .entities
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.transport.as_ref() == Some(&carrier))
                    .map(|(j, _)| j)
                    .collect();
                let mut placed = vec![];
                for j in passengers {
                    let mut chosen = None;
                    'rings: for ring in 0..4 {
                        for slot in 0..24 {
                            let angle = self.entities[i].view.heading
                                + std::f32::consts::PI
                                + slot as f32 * std::f32::consts::TAU / 24.;
                            let radius =
                                transport_clearance(&self.entities[i]) + 1.2 + ring as f32 * 1.5;
                            let p = [at[0] + angle.cos() * radius, at[1] + angle.sin() * radius];
                            if self.passable(p, false)
                                && placed.iter().all(|q| distance(*q, p) > 1.3)
                                && !self.entities.iter().any(|e| {
                                    e.transport.is_none()
                                        && !e.view.building
                                        && e.view.hp > 0.
                                        && distance(position(e), p) < 1.2
                                })
                            {
                                chosen = Some(p);
                                break 'rings;
                            }
                        }
                    }
                    if let Some(p) = chosen {
                        let e = &mut self.entities[j];
                        e.view.x = p[0];
                        e.view.z = p[1];
                        e.view.elevation = self.terrain.height(p[0], p[1]);
                        e.transport = None;
                        e.view.velocity = [0., 0.];
                        e.speed = 0.;
                        e.orders.clear();
                        e.path.clear();
                        placed.push(p);
                        count += 1;
                    }
                }
            }
        }
        self.boarding_step();
        if count == 0
            && let Some(status) = status
        {
            return status.into();
        }
        if load {
            format!("{count} soldier(s) boarding. Select the carrier and U to unload.")
        } else {
            format!("{count} soldier(s) unloaded. Blocked exits keep passengers safely aboard.")
        }
    }
    fn boarding_step(&mut self) {
        for i in 0..self.entities.len() {
            let Some(id) = self.entities[i]
                .orders
                .front()
                .filter(|o| o.kind == "board")
                .and_then(|o| o.target.clone())
            else {
                continue;
            };
            let Some(j) = self.entities.iter().position(|e| {
                e.view.id == id
                    && e.view.owner == self.entities[i].view.owner
                    && transport_capacity(&e.view.kind) > 0
                    && e.view.hp > 0.
            }) else {
                self.entities[i].orders.clear();
                self.entities[i].path.clear();
                continue;
            };
            let occupied = self
                .entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&id))
                .count();
            if self.entities[i].view.hp <= 0.
                || self.entities[i].view.downed
                || self.entities[i].view.retreating
                || occupied >= transport_capacity(&self.entities[j].view.kind)
                || self.entities[j].aircraft.phase.returning()
            {
                self.entities[i].orders.clear();
                self.entities[i].path.clear();
                continue;
            }
            let at = position(&self.entities[j]);
            let radius = transport_clearance(&self.entities[j]) + 1.;
            if distance(position(&self.entities[i]), at) <= radius + 0.8
                && self.entities[j].speed < 1.
                && !self.entities[j].view.airborne
                && !matches!(
                    self.entities[j].aircraft.phase,
                    FlightPhase::TransportLanding | FlightPhase::HeliTakeoff
                )
            {
                let e = &mut self.entities[i];
                e.transport = Some(id);
                e.orders.clear();
                e.path.clear();
                e.speed = 0.;
                e.view.velocity = [0., 0.];
                e.view.x = at[0];
                e.view.z = at[1];
            } else if self.tick.is_multiple_of(10)
                || self.entities[i]
                    .orders
                    .front()
                    .is_some_and(|o| o.destination.is_none())
            {
                let heading = self.entities[j].view.heading;
                let mut chosen = None;
                for turn in [0., 0.8, -0.8, 1.6, -1.6] {
                    let angle = heading + std::f32::consts::PI + turn;
                    let goal = [at[0] + angle.cos() * radius, at[1] + angle.sin() * radius];
                    if !self.passable(goal, false) {
                        continue;
                    }
                    let path = self.route(i, position(&self.entities[i]), goal);
                    if !path.is_empty() {
                        chosen = Some((goal, path));
                        break;
                    }
                }
                if let Some((goal, path)) = chosen {
                    self.entities[i].path = path;
                    self.entities[i].orders.front_mut().unwrap().destination = Some(goal);
                } else {
                    self.entities[i].orders.clear();
                    self.entities[i].path.clear();
                    self.notice =
                        "No safe boarding route. Move the carrier to clear ground.".into();
                }
            }
        }
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    fn huey_range() -> (NativeGame, String) {
        let mut g = NativeGame::operation(crate::training::ID, 384, 384, default_config());
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.config["fogEnabled"] = json!(false);
        g.terrain = crate::terrain::Terrain::new("huey-range", 384, 384);
        g.terrain.authored = vec![0.52; 384 * 384];
        g.terrain.hydrate();
        let base = g.spawn("airfield", 0, [230., 190.]);
        g.entities[0].view.model_scale = 0.5;
        let id = g.spawn("huey", 0, [222., 130.]);
        g.park_aircraft(1, &base, 0);
        (g, id)
    }
    fn advance_transport(g: &mut NativeGame, ticks: usize) {
        for _ in 0..ticks {
            g.tick += 1;
            g.boarding_step();
            g.movement();
        }
    }
    fn fly_to(g: &mut NativeGame, id: &str, at: Point) {
        assert_eq!(
            g.issue(
                0,
                &[id.into()],
                Order {
                    kind: "move".into(),
                    target: None,
                    destination: Some(at),
                },
                false
            ),
            1
        );
        advance_transport(g, 1000);
        let e = g.entities.iter().find(|e| e.view.id == id).unwrap();
        assert_eq!(e.aircraft.phase, FlightPhase::Hover);
        assert!(e.view.airborne);
        assert!(distance(position(e), at) < 0.5);
    }
    #[test]
    fn huey_carries_exactly_twelve_flies_saves_and_lands_before_unloading() {
        let (mut g, id) = huey_range();
        for n in 0..14 {
            g.spawn(
                if n == 4 {
                    "medic"
                } else if n == 5 {
                    "engineer"
                } else {
                    "short"
                },
                0,
                [202., 120. + n as f32 * 1.4],
            );
        }
        g.spawn("short", 1, [221., 129.]);
        g.transport_order(0, std::slice::from_ref(&id), None, true);
        // Repeating L must respect reserved seats, including soldiers still walking to the cabin.
        g.transport_order(0, std::slice::from_ref(&id), None, true);
        advance_transport(&mut g, 450);
        assert_eq!(
            g.entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&id))
                .count(),
            12
        );
        assert!(
            g.entities
                .iter()
                .filter(|e| e.view.owner == 1)
                .all(|e| e.transport.is_none())
        );
        let s = g.snapshot();
        let p = s.entities.iter().find(|p| p.id == id).unwrap();
        assert_eq!((p.occupants, p.capacity), (12, 12));
        assert_eq!(
            s.entities
                .iter()
                .filter(|p| p.category == "troop" && p.owner == 0)
                .count(),
            2
        );
        fly_to(&mut g, &id, [150., 220.]);
        assert!(
            g.entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&id))
                .all(|e| distance(position(e), [150., 220.]) < 0.5)
        );
        let result = g.transport_order(0, std::slice::from_ref(&id), None, false);
        assert!(result.contains("landing"), "{result}");
        advance_transport(&mut g, 20);
        assert_eq!(
            g.entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&id))
                .count(),
            12
        );
        let mut copy: NativeGame =
            serde_json::from_slice(&serde_json::to_vec(&g).unwrap()).unwrap();
        copy.terrain.hydrate();
        copy.validate().unwrap();
        advance_transport(&mut copy, 160);
        let h = copy.entities.iter().find(|e| e.view.id == id).unwrap();
        assert_eq!(h.aircraft.phase, FlightPhase::TransportGrounded);
        assert!(!h.view.airborne);
        assert!(h.view.flight_height() < 0.1);
        assert_eq!(h.view.elevation, copy.terrain.height(h.view.x, h.view.z));
        assert!(copy.entities.iter().all(|e| e.transport.is_none()));
        let dropped: Vec<_> = copy
            .entities
            .iter()
            .filter(|e| {
                e.view.category == "troop"
                    && e.view.owner == 0
                    && distance(position(e), [150., 220.]) < 20.
            })
            .collect();
        assert_eq!(dropped.len(), 12);
        for (i, a) in dropped.iter().enumerate() {
            assert!(copy.passable(position(a), false));
            assert!(distance(position(a), position(h)) > transport_clearance(h));
            assert!(
                dropped
                    .iter()
                    .skip(i + 1)
                    .all(|b| distance(position(a), position(b)) > 1.3)
            );
        }
        // A field landing is reusable: load the same force and take off again.
        copy.transport_order(0, std::slice::from_ref(&id), None, true);
        advance_transport(&mut copy, 400);
        assert_eq!(
            copy.entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&id))
                .count(),
            12
        );
        fly_to(&mut copy, &id, [130., 180.]);
    }
    #[test]
    fn huey_lands_to_pick_up_selected_troops_and_move_cancels_pending_dropoff() {
        let (mut g, id) = huey_range();
        fly_to(&mut g, &id, [150., 220.]);
        let soldier = g.spawn("short", 0, [146., 220.]);
        g.spawn("short", 0, [147., 221.]);
        g.transport_order(0, std::slice::from_ref(&soldier), Some(&id), true);
        advance_transport(&mut g, 20);
        assert!(g.entities.iter().all(|e| e.transport.is_none()));
        advance_transport(&mut g, 180);
        assert_eq!(
            g.entities.iter().filter(|e| e.transport.is_some()).count(),
            1
        );
        assert_eq!(
            g.entities
                .iter()
                .find(|e| e.view.id == soldier)
                .unwrap()
                .transport
                .as_ref(),
            Some(&id)
        );
        fly_to(&mut g, &id, [130., 180.]);
        g.transport_order(0, std::slice::from_ref(&id), None, false);
        advance_transport(&mut g, 10);
        fly_to(&mut g, &id, [150., 200.]);
        assert_eq!(
            g.entities.iter().filter(|e| e.transport.is_some()).count(),
            1
        );
        assert!(!g.entities[1].aircraft.transport_unload);
    }
    #[test]
    fn huey_refuses_blocked_landing_and_keeps_passengers_on_fuel_return() {
        let (mut g, id) = huey_range();
        let soldier = g.spawn("short", 0, [220., 130.]);
        g.transport_order(0, &[soldier], Some(&id), true);
        advance_transport(&mut g, 80);
        fly_to(&mut g, &id, [150., 220.]);
        let obstacle = g.spawn("medium", 0, [150., 220.]);
        let result = g.transport_order(0, std::slice::from_ref(&id), None, false);
        assert!(result.contains("blocked"), "{result}");
        assert_eq!(g.entities[1].aircraft.phase, FlightPhase::Hover);
        assert_eq!(
            g.entities.iter().filter(|e| e.transport.is_some()).count(),
            1
        );
        g.entities.retain(|e| e.view.id != obstacle);
        g.entities[1].fuel = 190.;
        advance_transport(&mut g, 1400);
        assert_eq!(g.entities[1].aircraft.phase, FlightPhase::Parked);
        assert_eq!(
            g.entities.iter().filter(|e| e.transport.is_some()).count(),
            1
        );
        assert_eq!(g.entities[1].view.fuel_fraction, 1.);
    }
    #[test]
    fn huey_and_apache_share_queued_and_occupied_helipad_capacity() {
        let (mut g, id) = huey_range();
        let base = g.entities[0].view.id.clone();
        assert_eq!(g.free_aircraft_stand(&base, "helicopter", None), Some(1));
        assert!(g.recruit("helicopter", &base, 0).starts_with("Recruiting"));
        let supply = g.supply[0];
        assert!(g.recruit("huey", &base, 0).contains("capacity"));
        assert_eq!(g.supply[0], supply);
        assert_eq!(g.free_aircraft_stand(&base, "plane", None), Some(0));
        g.jobs.clear();
        let apache = g.spawn("helicopter", 0, [0., 0.]);
        let i = g.entities.iter().position(|e| e.view.id == apache).unwrap();
        g.park_aircraft(i, &base, 1);
        assert!(g.free_aircraft_stand(&base, "huey", None).is_none());
        g.validate().unwrap();
        // Save validation rejects mixed helicopter types assigned to the same pad.
        g.entities[i].aircraft.stand = Some(0);
        assert!(g.validate().is_err());
        assert_eq!(g.entities[1].view.id, id);
    }
    #[test]
    fn unloading_a_moving_carrier_cancels_queued_travel_and_stops_it() {
        let mut g = NativeGame::operation(crate::training::ID, 384, 384, default_config());
        g.entities.clear();
        g.bases.clear();
        g.props.clear();
        let carrier = g.spawn("troopCarrier", 0, [150., 180.]);
        g.spawn("short", 0, [150., 180.]);
        g.entities[1].transport = Some(carrier.clone());
        for goal in [[190., 180.], [220., 195.]] {
            assert_eq!(
                g.issue(
                    0,
                    std::slice::from_ref(&carrier),
                    Order {
                        kind: "move".into(),
                        target: None,
                        destination: Some(goal)
                    },
                    true
                ),
                1
            );
        }
        for _ in 0..20 {
            g.tick += 1;
            g.movement();
        }
        assert!(g.entities[0].speed > 0.);
        let stopped = position(&g.entities[0]);
        g.command(json!({"type":"command","commandType":"unload","unitIds":[carrier]}));
        let e = &g.entities[0];
        assert!(e.orders.is_empty() && e.path.is_empty());
        assert_eq!(e.speed, 0.);
        assert_eq!(e.view.velocity, [0., 0.]);
        assert!(g.entities[1].transport.is_none());
        for _ in 0..40 {
            g.tick += 1;
            g.movement();
        }
        assert_eq!(position(&g.entities[0]), stopped);
    }
    #[test]
    fn boarding_walks_to_real_sized_carrier_carries_six_and_unloads_clear_of_hull() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("boarding", 200, 200);
        g.terrain.authored = vec![0.52; 40000];
        g.terrain.hydrate();
        let carrier = g.spawn("troopCarrier", 0, [70., 70.]);
        for n in 0..8 {
            g.spawn("short", 0, [52., 58. + n as f32 * 2.]);
        }
        g.entities[8].view.downed = true;
        g.transport_order(0, std::slice::from_ref(&carrier), None, true);
        for _ in 0..400 {
            g.tick += 1;
            g.boarding_step();
            g.movement();
        }
        assert_eq!(
            g.entities
                .iter()
                .filter(|e| e.transport.as_ref() == Some(&carrier))
                .count(),
            6
        );
        let view = g.snapshot();
        let p = view.entities.iter().find(|e| e.id == carrier).unwrap();
        assert_eq!((p.occupants, p.capacity), (6, 6));
        let mut loaded: NativeGame =
            serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        loaded.transport_order(0, std::slice::from_ref(&carrier), None, false);
        assert!(loaded.entities.iter().all(|e| e.transport.is_none()));
        for e in loaded
            .entities
            .iter()
            .filter(|e| e.view.category == "troop" && !e.view.downed)
            .take(6)
        {
            assert!(
                distance(position(e), [70., 70.])
                    > body_clearance("troopCarrier", loaded.entities[0].view.model_scale)
            );
            assert!(loaded.passable(position(e), false));
        }
    }
    #[test]
    fn targeted_boarding_only_loads_selected_friendly_infantry_and_move_cancels_it() {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        let c = g.spawn("troopCarrier", 0, [55., 55.]);
        let a = g.spawn("short", 0, [45., 50.]);
        g.spawn("short", 0, [54., 54.]);
        g.spawn("short", 1, [55., 55.]);
        g.command(json!({"type":"command","commandType":"load","unitIds":[a],"targetId":c}));
        assert_eq!(
            g.entities
                .iter()
                .filter(|e| e.orders.front().is_some_and(|o| o.kind == "board"))
                .count(),
            1
        );
        g.issue(
            0,
            &[a],
            Order {
                kind: "move".into(),
                destination: Some([30., 30.]),
                target: None,
            },
            false,
        );
        assert_eq!(g.entities[1].orders[0].kind, "move");
        assert!(g.entities.iter().all(|e| e.transport.is_none()));
    }
}
