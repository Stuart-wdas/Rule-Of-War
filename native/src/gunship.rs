// Included in simulation.rs: off-map support; never assigned an airfield stand.
pub const GUNSHIP_COST: f32 = 60.;
pub const GUNSHIP_AREA: f32 = 18.;
const GUNSHIP_RADIUS: f32 = 110.;
const GUNSHIP_SPEED: f32 = 38.;
const GUNSHIP_TURNS: f32 = std::f32::consts::TAU * 5.;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum GunshipPhase {
    Ingress,
    Orbit,
    Egress,
    Departed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GunshipMission {
    target: Point,
    phase: GunshipPhase,
    angle: f32,
    swept: f32,
    height: f32,
    next_light: u64,
    next_heavy: u64,
    departure_ticks: u32,
}

impl GunshipMission {
    fn valid(&self, cols: i32, rows: i32) -> bool {
        self.target.iter().all(|n| n.is_finite() && *n >= 0.)
            && self.target[0] < cols as f32
            && self.target[1] < rows as f32
            && self.angle.is_finite()
            && self.angle.abs() < 100.
            && self.swept.is_finite()
            && (0. ..=GUNSHIP_TURNS).contains(&self.swept)
            && self.height.is_finite()
            && self.height.abs() < 2000.
    }
}

impl NativeGame {
    pub(crate) fn smoke_gunship(&mut self) -> String {
        *self = Self::operation(crate::training::ID, 1024, 1024, default_config());
        self.config["fogEnabled"] = json!(false);
        self.entities.clear();
        self.bases.clear();
        self.props.clear();
        self.supply[0] = 200.;
        let target = [280., 280.];
        self.command(
            json!({"type":"call-in","unitType":"c130","destination":{"x":target[0],"y":target[1]}}),
        );
        while self.entities[0].gunship.as_ref().unwrap().phase == GunshipPhase::Ingress {
            self.tick += 1;
            self.gunship_movement(0);
        }
        let id = self.entities[0].view.id.clone();
        for dx in [-9., 0., 9.] {
            self.spawn("medium", 1, [target[0] + dx, target[1]]);
            let t = self.entities.last_mut().unwrap();
            t.view.hp = 10000.;
            t.view.max_hp = 10000.;
            t.view.training_target = true;
            t.halted = true;
            t.stance = "hold-fire".into();
        }
        id
    }
    fn call_gunship(&mut self, owner: i32, target: Option<Point>) -> String {
        let Some(target) = target.filter(|p| {
            p.iter().all(|n| n.is_finite() && *n >= 0.)
                && p[0] < self.terrain.cols as f32
                && p[1] < self.terrain.rows as f32
        }) else {
            return "Choose a target area inside the battlefield.".into();
        };
        if self.paused {
            return "Resume the operation before calling air support.".into();
        }
        if self
            .entities
            .iter()
            .any(|e| e.view.kind == "c130" && e.view.owner == owner && e.view.hp > 0.)
        {
            return "Your AC-130 is already on station or departing.".into();
        }
        if self.supply[owner as usize] < GUNSHIP_COST {
            return "AC-130 support requires 60 supply.".into();
        }
        let angle = if owner == 0 {
            -std::f32::consts::FRAC_PI_2
        } else {
            std::f32::consts::FRAC_PI_2
        };
        let at = [
            if owner == 0 {
                -180.
            } else {
                self.terrain.cols as f32 + 180.
            },
            target[1] + angle.sin() * GUNSHIP_RADIUS,
        ];
        // Constant orbit altitude clears the complete circle, including hills.
        let height = (0..32)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 32.;
                self.terrain.height(
                    target[0] + a.cos() * GUNSHIP_RADIUS,
                    target[1] + a.sin() * GUNSHIP_RADIUS,
                )
            })
            .fold(self.terrain.height(target[0], target[1]), f32::max)
            + 110.;
        self.supply[owner as usize] -= GUNSHIP_COST;
        self.spawn("c130", owner, at);
        let e = self.entities.last_mut().unwrap();
        e.view.airborne = true;
        e.view.heading = if owner == 0 { 0. } else { std::f32::consts::PI };
        e.aircraft.phase = FlightPhase::Ingress;
        e.aircraft.world_y = height;
        e.view.altitude = Some(height - e.view.elevation);
        e.speed = GUNSHIP_SPEED;
        e.gunship = Some(GunshipMission {
            target,
            phase: GunshipPhase::Ingress,
            angle,
            swept: 0.,
            height,
            next_light: 0,
            next_heavy: 0,
            departure_ticks: 0,
        });
        "AC-130 inbound from your map edge. Five orbits over the marked area.".into()
    }

    fn gunship_movement(&mut self, i: usize) {
        let Some(mut m) = self.entities[i].gunship.take() else {
            return;
        };
        let e = &mut self.entities[i];
        let before = position(e);
        let mut bank = 0.;
        let mut pitch = 0.;
        match m.phase {
            GunshipPhase::Ingress => {
                let remaining = m.target[0] - e.view.x;
                e.view.x += remaining.signum() * (GUNSHIP_SPEED * DT).min(remaining.abs());
                if remaining.abs() <= GUNSHIP_SPEED * DT {
                    e.view.x = m.target[0];
                    m.phase = GunshipPhase::Orbit;
                    m.next_light = self.tick + 6;
                    m.next_heavy = self.tick + 22;
                }
            }
            GunshipPhase::Orbit => {
                let delta = (GUNSHIP_SPEED * DT / GUNSHIP_RADIUS).min(GUNSHIP_TURNS - m.swept);
                m.swept = (m.swept + delta).min(GUNSHIP_TURNS);
                m.angle += delta;
                e.view.x = m.target[0] + GUNSHIP_RADIUS * m.angle.cos();
                e.view.z = m.target[1] + GUNSHIP_RADIUS * m.angle.sin();
                e.view.heading = m.angle + std::f32::consts::FRAC_PI_2;
                bank = 0.52;
                if m.swept >= GUNSHIP_TURNS {
                    m.phase = GunshipPhase::Egress;
                    e.aircraft.phase = FlightPhase::Egress;
                }
            }
            GunshipPhase::Egress => {
                // Roll out of the final tangent and climb away across the far edge.
                m.departure_ticks += 1;
                let peel = (m.departure_ticks as f32 * DT / 3.).min(1.);
                bank = 0.52 * (1. - peel);
                pitch = 0.06 * (1. - peel * 0.6);
                m.height += DT * 2.5;
                e.view.x += e.view.heading.cos() * GUNSHIP_SPEED * DT;
                e.view.z += e.view.heading.sin() * GUNSHIP_SPEED * DT;
                if e.view.x < -200. || e.view.x > self.terrain.cols as f32 + 200. {
                    m.phase = GunshipPhase::Departed;
                }
            }
            GunshipPhase::Departed => {}
        }
        let ground = self.terrain.height(e.view.x, e.view.z);
        e.aircraft.world_y = m.height.max(ground + 75.);
        e.view.elevation = ground;
        e.view.altitude = Some(e.aircraft.world_y - ground);
        e.view.flight_bank += (bank - e.view.flight_bank) * 0.12;
        e.view.flight_pitch = pitch;
        e.view.velocity = [(e.view.x - before[0]) / DT, (e.view.z - before[1]) / DT];
        e.view.fuel_fraction = 1.;
        let fire_light = m.phase == GunshipPhase::Orbit && self.tick >= m.next_light;
        let fire_heavy = m.phase == GunshipPhase::Orbit && self.tick >= m.next_heavy;
        if fire_light {
            m.next_light = self.tick + 24;
        }
        if fire_heavy {
            m.next_heavy = self.tick + 76;
        }
        e.gunship = Some(m);
        if fire_light {
            self.fire_gunship(i, "c130-30");
        }
        if fire_heavy {
            self.fire_gunship(i, "c130-105");
        }
    }

    fn fire_gunship(&mut self, i: usize, weapon: &str) {
        let e = &self.entities[i];
        let target = e.gunship.as_ref().unwrap().target;
        let at = position(e);
        let owner = e.view.owner;
        let source = e.view.id.clone();
        let height = e.aircraft.world_y;
        let s = spec(weapon);
        let angle = self.random() * std::f32::consts::TAU;
        let spread = self.random().sqrt() * number(s, "spread", 12.);
        let to = [
            (target[0] + angle.cos() * spread).clamp(0., self.terrain.cols as f32 - 0.01),
            (target[1] + angle.sin() * spread).clamp(0., self.terrain.rows as f32 - 0.01),
        ];
        let travel = ((distance(at, to).hypot(height - self.terrain.height(to[0], to[1])))
            / number(s, "projectileSpeed", 240.)
            / DT)
            .ceil()
            .max(1.) as u64;
        let p = Projectile {
            owner,
            from: at,
            to,
            born: self.tick,
            impact: self.tick + travel,
            damage: number(s, "damage", 1.),
            radius: number(s, "damageRadius", 4.),
            hit: true,
            weapon: weapon.into(),
            ground_hit: true,
        };
        self.events.push(BattleEvent {
            tick: self.tick,
            kind: "shot".into(),
            at,
            from: Some(to),
            owner,
            radius: p.radius,
            weapon: weapon.into(),
            source,
            duration: travel as f32 * DT,
            hit: None,
            target: None,
            release_height: None,
        });
        self.warn_incoming(&p);
        self.projectiles.push(p);
        self.entities[i].last_shot = self.tick;
        if let Some(t) = &mut self.training {
            t.shots += 1;
        }
    }
}

#[cfg(test)]
mod gunship_tests {
    use super::*;

    fn range() -> NativeGame {
        let mut g = NativeGame::operation(crate::training::ID, 1024, 1024, default_config());
        g.entities.clear();
        g.bases.clear();
        g.props.clear();
        g.config["fogEnabled"] = json!(false);
        g.supply = [100., 100.];
        g
    }
    fn call(g: &mut NativeGame, owner: i32, at: Point) {
        g.command_as(
            owner,
            json!({"type":"call-in","unitType":"c130","destination":{"x":at[0],"y":at[1]}}),
        );
    }

    #[test]
    fn gunship_five_full_orbits_both_player_edges_then_departure_without_a_wreck() {
        for owner in [0, 1] {
            for target in [[500., 500.], [2., 2.], [1022., 1022.]] {
                let mut g = range();
                call(&mut g, owner, target);
                assert_eq!(g.supply[owner as usize], 40.);
                assert_eq!(g.entities.len(), 1);
                let e = &g.entities[0];
                assert!(e.view.airborne && e.home.is_none() && e.aircraft.stand.is_none());
                assert!(if owner == 0 {
                    e.view.x < 0.
                } else {
                    e.view.x > 1024.
                });
                g.validate().unwrap();
                let mut shots = [vec![], vec![]];
                let mut last_swept = 0.;
                let mut departed = false;
                for _ in 0..5000 {
                    if g.entities.is_empty() {
                        departed = true;
                        break;
                    }
                    g.step();
                    if let Some(e) = g.entities.first() {
                        let m = e.gunship.as_ref().unwrap();
                        assert!(m.swept >= last_swept);
                        last_swept = m.swept;
                        if m.phase == GunshipPhase::Orbit {
                            assert!((distance(position(e), target) - GUNSHIP_RADIUS).abs() < 0.01);
                        }
                        for event in g
                            .events
                            .iter()
                            .filter(|s| s.tick == g.tick && s.kind == "shot")
                        {
                            assert_eq!(m.phase, GunshipPhase::Orbit);
                            assert_eq!(event.owner, owner);
                            assert!(event.duration > 0.);
                            let end = event.from.unwrap();
                            assert!(
                                (0. ..1024.).contains(&end[0]) && (0. ..1024.).contains(&end[1])
                            );
                            shots[usize::from(event.weapon == "c130-105")].push(event.tick);
                        }
                    }
                }
                assert!(departed);
                assert_eq!(last_swept, GUNSHIP_TURNS);
                assert!(shots[0].len() > 60 && shots[1].len() > 20);
                assert!(shots[0].windows(2).all(|t| t[1] - t[0] == 24));
                assert!(shots[1].windows(2).all(|t| t[1] - t[0] == 76));
                assert!(!g.props.iter().any(|p| p.wreck.is_some()));
            }
        }
    }

    #[test]
    fn gunship_rejects_invalid_calls_and_regular_production_without_spending() {
        let mut g = range();
        for at in [[-1., 50.], [1024., 50.], [50., 1024.]] {
            call(&mut g, 0, at);
            assert!(g.entities.is_empty());
            assert_eq!(g.supply[0], 100.);
        }
        g.command(json!({"type":"call-in","unitType":"c130"}));
        assert!(g.entities.is_empty());
        g.supply[0] = 59.;
        call(&mut g, 0, [300., 300.]);
        assert!(g.entities.is_empty());
        g.supply[0] = 100.;
        g.paused = true;
        call(&mut g, 0, [300., 300.]);
        assert!(g.entities.is_empty());
        g.paused = false;
        call(&mut g, 0, [300., 300.]);
        let balance = g.supply[0];
        call(&mut g, 0, [400., 400.]);
        assert_eq!(g.supply[0], balance);
        assert_eq!(g.entities.len(), 1);
        let id = g.entities[0].view.id.clone();
        assert_eq!(
            g.issue(
                0,
                &[id],
                Order {
                    kind: "move".into(),
                    target: None,
                    destination: Some([600., 600.])
                },
                false
            ),
            0
        );
        assert_eq!(g.entities[0].gunship.as_ref().unwrap().target, [300., 300.]);
        assert!(training_unit_price("c130").is_none());
    }

    #[test]
    fn gunship_pause_and_save_restore_preserve_the_flight_and_firing_schedule() {
        let mut g = range();
        call(&mut g, 0, [320., 320.]);
        for _ in 0..500 {
            g.step();
        }
        assert_eq!(
            g.entities[0].gunship.as_ref().unwrap().phase,
            GunshipPhase::Orbit
        );
        g.paused = true;
        let before = serde_json::to_value(&g).unwrap();
        g.step();
        assert_eq!(before, serde_json::to_value(&g).unwrap());
        g.paused = false;
        let path =
            std::env::temp_dir().join(format!("rule-of-war-gunship-{}.json", std::process::id()));
        g.save_to(&path).unwrap();
        let mut saved = NativeGame::load_from(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        saved.validate().unwrap();
        for _ in 0..2400 {
            g.step();
            saved.step();
        }
        assert_eq!(
            serde_json::to_value(&g).unwrap(),
            serde_json::to_value(&saved).unwrap()
        );
        assert!(g.entities.is_empty());
    }

    #[test]
    fn gunship_heavy_shell_has_area_damage_spares_friendlies_and_leaves_a_ground_mark() {
        let mut g = range();
        for owner in [0, 1] {
            g.spawn("medium", owner, [500., 500.]);
        }
        let light = spec("c130-30");
        let heavy = spec("c130-105");
        assert!(number(heavy, "damageRadius", 0.) > number(light, "damageRadius", 0.));
        assert!(number(heavy, "damage", 0.) > number(light, "damage", 0.));
        let p = Projectile {
            owner: 0,
            from: [390., 500.],
            to: [500., 500.],
            born: 0,
            impact: 1,
            damage: number(heavy, "damage", 0.),
            radius: number(heavy, "damageRadius", 0.),
            hit: true,
            weapon: "c130-105".into(),
            ground_hit: true,
        };
        g.impact(&p);
        assert_eq!(g.entities[0].view.hp, g.entities[0].view.max_hp);
        assert!(g.entities[1].view.hp < g.entities[1].view.max_hp);
        assert!(g.events.iter().any(|e| e.kind == "blast" && e.radius == 6.));
        // Bare-ground hits use the heavy shell's persistent disturbed-earth mark.
        g.impact(&Projectile {
            to: [700., 700.],
            ..p
        });
        assert!(
            g.props
                .iter()
                .any(|p| p.scar.is_some_and(|m| m.radius == 4.8))
        );
    }
}
