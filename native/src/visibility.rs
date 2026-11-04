// Included in simulation.rs: observation facts reused within one simulation/display phase.
struct Observation<'a> {
    fog: bool,
    sources: [Vec<SightSource>; 2],
    game: &'a NativeGame,
    world: Option<Arc<SpottingWorld>>,
}

#[cfg(test)]
mod effect_visibility_tests {
    use super::*;
    fn field() -> NativeGame {
        let mut g = NativeGame::new();
        g.entities.clear();
        g.props.clear();
        g.bases.clear();
        g.terrain = crate::terrain::Terrain::new("incoming", 256, 128);
        g.terrain.authored = vec![0.52; 256 * 128];
        g.terrain.hydrate();
        g.config["fogEnabled"] = json!(true);
        g.tick = 10;
        g
    }
    #[test]
    fn incoming_enemy_rounds_enter_vision_without_revealing_the_shooter() {
        let mut g = field();
        g.spawn("short", 0, [120., 60.]);
        let shooter = g.spawn("medium", 1, [20., 60.]);
        g.events.push(BattleEvent {
            tick: 10,
            kind: "shot".into(),
            at: [20., 60.],
            from: Some([120., 60.]),
            owner: 1,
            radius: 0.4,
            weapon: "medium".into(),
            source: shooter.clone(),
            duration: 2.,
            release_height: None,
            hit: None,
            target: None,
        });
        assert!(
            g.snapshot().events.is_empty(),
            "round has not entered sight yet"
        );
        g.tick = 40;
        let s = g.snapshot();
        assert!(!s.entities.iter().any(|p| p.id == shooter));
        let shot = s.events.iter().find(|e| e.kind == "shot").unwrap();
        assert!((shot.at[0] - 88.).abs() < 0.01);
        assert!(shot.duration < 2. && shot.source.starts_with("incoming-"));
        assert!(shot.tick > 10 && shot.tick <= g.tick);
    }
    #[test]
    fn last_friendly_vehicle_death_keeps_the_enemy_impact_and_destruction_visible() {
        let mut g = field();
        let victim = g.spawn("medium", 0, [120., 60.]);
        let enemy = g.spawn("medium", 1, [20., 60.]);
        g.impact(&Projectile {
            owner: 1,
            from: [20., 60.],
            to: [120., 60.],
            born: 9,
            impact: 10,
            damage: 100.,
            radius: 4.,
            hit: true,
            weapon: "medium".into(),
            ground_hit: false,
        });
        g.record_wrecks();
        g.entities.retain(|e| e.view.hp > 0.);
        assert!(!g.entities.iter().any(|e| e.view.id == victim));
        let s = g.snapshot();
        assert!(!s.entities.iter().any(|p| p.id == enemy));
        assert!(s.events.iter().any(|e| e.owner == 1 && e.kind == "impact"));
        assert!(
            s.events
                .iter()
                .any(|e| e.kind == "destruction" && e.source == victim)
        );
    }
    #[test]
    fn unseen_fire_outside_all_sight_sources_stays_hidden() {
        let g = field();
        let observation = Observation {
            fog: true,
            sources: [
                vec![SightSource {
                    at: [120., 60.],
                    height: g.terrain.height(120., 60.) + 1.7,
                    squared: 32_f32.powi(2),
                    infantry: true,
                    facing: None,
                }],
                vec![],
            ],
            game: &g,
            world: None,
        };
        assert!(observation.segment_window(0, [0., 0.], [30., 0.]).is_none());
        assert!(
            observation
                .segment_window(0, [20., 60.], [200., 60.])
                .is_some()
        );
    }
}
impl Observation<'_> {
    fn segment_window(&self, owner: i32, from: Point, to: Point) -> Option<(f32, f32)> {
        if !self.fog {
            return Some((0., 1.));
        }
        let delta = [to[0] - from[0], to[1] - from[1]];
        let a = delta[0] * delta[0] + delta[1] * delta[1];
        if a < 0.0001 {
            return self.sees(owner, from).then_some((0., 1.));
        }
        let mut window: Option<(f32, f32)> = None;
        for source in self.sources.get(owner as usize)? {
            let (center, squared) = (source.at, source.squared);
            let offset = [from[0] - center[0], from[1] - center[1]];
            let b = offset[0] * delta[0] + offset[1] * delta[1];
            let discriminant =
                b * b - a * (offset[0] * offset[0] + offset[1] * offset[1] - squared);
            if discriminant < 0. {
                continue;
            }
            let enter = ((-b - discriminant.sqrt()) / a).max(0.);
            let exit = ((-b + discriminant.sqrt()) / a).min(1.);
            if enter >= exit {
                continue;
            }
            window = Some(window.map_or((enter, exit), |(lo, hi)| (lo.min(enter), hi.max(exit))));
        }
        // Keep a contiguous visible portion: never join two pieces across an unseen house.
        let (enter, exit) = window?;
        if self.world.is_none() {
            return Some((enter, exit));
        }
        let steps = (((exit - enter) * a.sqrt() / 2.).ceil() as usize).max(1);
        let point = |t: f32| [from[0] + delta[0] * t, from[1] + delta[1] * t];
        let mut start = None;
        let mut previous = enter;
        for i in 0..=steps {
            let t = enter + (exit - enter) * i as f32 / steps as f32;
            let visible = self.sees(owner, point(t));
            if visible && start.is_none() {
                start = Some(t);
            }
            if !visible && let Some(start) = start {
                return (previous > start).then_some((start, previous));
            }
            previous = t;
        }
        start
            .filter(|start| exit > *start)
            .map(|start| (start, exit))
    }
    fn sees(&self, owner: i32, point: Point) -> bool {
        !self.fog
            || self.sources.get(owner as usize).is_some_and(|sources| {
                sources.iter().any(|source| {
                    source.covers(point)
                        && self.world.as_ref().is_none_or(|world| {
                            world.clear(
                                self.game,
                                source.point(),
                                Vec3::new(
                                    point[0],
                                    self.game.terrain.height(point[0], point[1]) + 1.3,
                                    point[1],
                                ),
                                3.,
                            )
                        })
                })
            })
    }
    fn source_sees(&self, source: &SightSource, target: &Entity) -> bool {
        let point = position(target);
        if !source.covers(point) {
            return false;
        }
        let moving = target.speed > 0.3 || target.view.velocity.iter().any(|v| v.abs() > 0.3);
        let firing = target.last_shot > 0 && self.game.tick.saturating_sub(target.last_shot) < 60;
        let infantry = target.view.category == "troop";
        let mut limit = if infantry { 1.3 } else { 2.8 };
        if moving {
            limit += 0.8;
        }
        if firing {
            limit += 1.5;
        }
        if distance(source.at, point) < 10. {
            limit += 2.;
        }
        self.world.as_ref().is_none_or(|world| {
            world.clear(
                self.game,
                source.point(),
                Vec3::new(point[0], self.game.sight_height(target, false), point[1]),
                limit,
            )
        })
    }
    fn garrison_seen(&self, owner: i32, target: &Entity) -> bool {
        self.sources.get(owner as usize).is_some_and(|sources| {
            sources
                .iter()
                .any(|source| source.infantry && self.source_sees(source, target))
        })
    }
    fn sees_entity(&self, owner: i32, target: &Entity) -> bool {
        if target.view.owner == owner || target.view.building {
            return true;
        }
        if target.transport.is_some() {
            return false;
        }
        if target.view.garrison.as_ref().is_some_and(|g| g.entered) {
            return self.garrison_seen(owner, target);
        }
        !self.fog
            || self.sources.get(owner as usize).is_some_and(|sources| {
                sources
                    .iter()
                    .any(|source| self.source_sees(source, target))
            })
    }
    fn mask(&self, entities: &[Entity]) -> Vec<[bool; 2]> {
        entities
            .iter()
            .map(|e| [0, 1].map(|owner| self.sees_entity(owner, e)))
            .collect()
    }
}
impl NativeGame {
    fn visible_events(&self, owner: i32, observation: &Observation) -> Vec<BattleEvent> {
        // A killing hit must not erase its own visibility when the victim supplied
        // the last sight source. These witnesses expose effects, never enemy units.
        let mut effects = Observation {
            fog: observation.fog,
            sources: observation.sources.clone(),
            game: self,
            world: observation.world.clone(),
        };
        if let Some(sources) = effects.sources.get_mut(owner as usize) {
            for prop in &self.props {
                if let Some(wreck) = prop
                    .wreck
                    .as_ref()
                    .filter(|w| w.piece.owner == owner && self.tick.saturating_sub(w.born) <= 40)
                {
                    sources.push(SightSource {
                        at: prop.at,
                        height: self.terrain.height(prop.at[0], prop.at[1]) + 1.7,
                        squared: sight_range(&wreck.piece.kind).powi(2),
                        infantry: false,
                        facing: None,
                    });
                }
            }
            for unit in &self.entities {
                if unit.view.owner == owner
                    && unit.view.downed
                    && self.tick.saturating_sub(unit.last_hit) <= 40
                {
                    sources.push(SightSource::new(self, unit));
                }
            }
        }
        self.events
            .iter()
            .filter_map(|event| {
                if event.owner == owner || effects.sees(owner, event.at) {
                    return Some(event.clone());
                }
                if event.kind != "shot" {
                    return None;
                }
                let end = event.from?;
                let (enter, exit) = effects.segment_window(owner, event.at, end)?;
                let mut clipped = event.clone();
                let lerp = |t: f32| {
                    [
                        event.at[0] + (end[0] - event.at[0]) * t,
                        event.at[1] + (end[1] - event.at[1]) * t,
                    ]
                };
                clipped.at = lerp(enter);
                clipped.from = Some(lerp(exit));
                clipped.tick += (event.duration * enter / DT).ceil() as u64;
                if clipped.tick > self.tick {
                    return None;
                }
                clipped.duration = event.duration * (exit - enter);
                clipped.release_height =
                    Some(self.terrain.height(clipped.at[0], clipped.at[1]) + 1.5);
                clipped.source = format!(
                    "incoming-{:x}-{}",
                    crate::destruction::seed(&event.source),
                    event.tick
                );
                Some(clipped)
            })
            .collect()
    }
    fn observation(&self) -> Observation<'_> {
        let mut observation = Observation {
            fog: flag(&self.config, "fogEnabled"),
            sources: Default::default(),
            game: self,
            world: None,
        };
        {
            for e in &self.entities {
                if (0..=1).contains(&e.view.owner)
                    && e.view.hp > 0.
                    && !e.view.downed
                    && e.transport.is_none()
                {
                    observation.sources[e.view.owner as usize].push(SightSource::new(self, e));
                }
            }
        }
        if observation.fog
            || self
                .entities
                .iter()
                .any(|e| e.view.garrison.as_ref().is_some_and(|g| g.entered))
        {
            observation.world = Some(Arc::new(SpottingWorld::new(self)));
        }
        observation
    }
}
