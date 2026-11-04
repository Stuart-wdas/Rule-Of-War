// Included in simulation.rs so import can restore the complete private state.
impl NativeGame {
    pub fn import_file(path: &std::path::Path) -> Result<Self, String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
            return Err("Import exceeds 128 MiB.".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("Import exceeds 128 MiB.".into());
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if v["protocol"] == 1 && v["game"]["entities"].is_array() {
            let mut game: Self =
                serde_json::from_value(v["game"].clone()).map_err(|e| e.to_string())?;
            game.upgrade_house_layouts();
            game.validate()?;
            game.terrain.hydrate();
            game.repair_base_foundations();
            game.refresh_towns();
            game.ground_roster_cleanup();
            game.paused = true;
            game.human_opponent = false;
            return Ok(game);
        }
        if v["entities"].is_array() {
            return Self::load_from(path);
        }
        if v["sectors"].is_array() {
            let world: crate::world::World =
                serde_json::from_value(v).map_err(|e| e.to_string())?;
            if world.sectors.is_empty() || world.sectors.len() > 10000 {
                return Err("Invalid world sectors.".into());
            }
            let game = Self {
                world,
                paused: true,
                notice: "World campaign imported. Open F7 to continue.".into(),
                ..Self::default()
            };
            return Ok(game);
        }
        if v["authoredTerrain"]["elevation"].is_array() {
            let cols = v["cols"].as_i64().unwrap_or(0) as i32;
            let rows = v["rows"].as_i64().unwrap_or(0) as i32;
            if !(10..=crate::terrain::MAX_SIZE).contains(&cols) || !(10..=crate::terrain::MAX_SIZE).contains(&rows) {
                return Err("Map dimensions must be 10-2048.".into());
            }
            let heights: Vec<f32> =
                serde_json::from_value(v["authoredTerrain"]["elevation"].clone())
                    .map_err(|e| e.to_string())?;
            if heights.len() != (cols * rows) as usize
                || heights
                    .iter()
                    .any(|h| !h.is_finite() || !(0. ..=1.).contains(h))
            {
                return Err("Invalid authored elevation.".into());
            }
            let mut game = Self::operation(
                v["id"].as_str().unwrap_or("custom-imported"),
                cols,
                rows,
                default_config(),
            );
            game.terrain.authored = heights;
            game.terrain.water_level =
                number(&v["authoredTerrain"], "waterLevel", 0.28).clamp(0.02, 0.75);
            game.terrain.authored_scale = number(
                &v["authoredTerrain"],
                "maxElevation",
                number(&v["terrainConfig"], "maxElevation", 160.),
            )
            .clamp(0., 1000.);
            game.terrain.data = None;
            game.redeploy();
            game.paused = true;
            game.notice = "Authored map imported. F8 opens the editor.".into();
            return Ok(game);
        }
        let checkpoint = if v["game"].is_object() {
            &v
        } else if v["state"]["game"].is_object() {
            &v["state"]
        } else if v["snapshot"]["game"].is_object() {
            &v["snapshot"]
        } else {
            return Err(
                "Expected a native save, web checkpoint, world, or authored map JSON.".into(),
            );
        };
        Self::from_web_checkpoint(checkpoint)
    }
    fn redeploy(&mut self) {
        let forces = std::mem::take(&mut self.entities);
        for mut entity in forces {
            let at = position(&entity);
            if let Some(p) = self.open_land(at, &entity.view.kind) {
                for block in &mut entity.blocking {
                    block[0] += p[0] - at[0];
                    block[1] += p[1] - at[1];
                }
                entity.view.x = p[0];
                entity.view.z = p[1];
                entity.view.elevation = self.terrain.height(p[0], p[1]);
                self.entities.push(entity);
            }
        }
    }
    pub fn from_web_checkpoint(v: &Value) -> Result<Self, String> {
        let cols = v["config"]["cols"]
            .as_i64()
            .ok_or("Checkpoint has no columns")? as i32;
        let rows = v["config"]["rows"]
            .as_i64()
            .ok_or("Checkpoint has no rows")? as i32;
        if !(10..=crate::terrain::MAX_SIZE).contains(&cols) || !(10..=crate::terrain::MAX_SIZE).contains(&rows) {
            return Err("Checkpoint map dimensions are invalid.".into());
        }
        let id = v["terrain"]["mapId"].as_str().unwrap_or("custom");
        let mut out = Self::operation(id, cols, rows, v["config"].clone());
        out.entities.clear();
        out.jobs.clear();
        out.projectiles.clear();
        out.tick = v["game"]["simulationTick"].as_u64().unwrap_or(0);
        let state = &v["game"];
        out.winner = state["winner"].as_i64().map(|v| v as i32);
        for i in 0..2 {
            out.supply[i] = number(&state["players"][i], "resources", 0.);
            out.hold[i] = number(&state["objectiveVictoryProgress"], &i.to_string(), 0.);
        }
        let units = state["units"].as_array().ok_or("Checkpoint has no units")?;
        let structures = state["structures"]
            .as_array()
            .ok_or("Checkpoint has no structures")?;
        if units.len() + structures.len() > 10000 {
            return Err("Checkpoint entity limit exceeded.".into());
        }
        for source in units.iter().chain(structures) {
            let kind = source["type"].as_str().ok_or("Piece has no type")?;
            if !spec(kind).is_object() {
                return Err(format!("Unsupported piece: {kind}"));
            }
            let owner = source["owner"].as_i64().ok_or("Piece has no owner")? as i32;
            if !(-1..=1).contains(&owner) {
                return Err("Invalid piece owner".into());
            }
            let x = number(source, "worldX", number(source, "x", -1.));
            let z = number(source, "worldY", number(source, "y", -1.));
            if x < 0. || z < 0. || x >= cols as f32 || z >= rows as f32 {
                return Err("Piece outside checkpoint map".into());
            }
            out.spawn(kind, owner, [x, z]);
            let e = out.entities.last_mut().unwrap();
            e.view.id = source["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| source["id"].to_string());
            e.view.hp = number(source, "hp", e.view.hp);
            e.view.strength = number(source, "strength", e.view.strength as f32) as u32;
            e.view.heading = -number(source, "heading", 0.);
            e.view.airborne = flag(source, "airborne");
            e.cooldown = number(source, "weaponCooldownTicks", 0.) as u32;
            e.windup = number(source, "weaponWindupTicks", 0.) as u32;
            e.stationary = number(source, "stationaryTicks", 0.) as u32;
            e.stance = source["stance"].as_str().unwrap_or("attack").into();
            e.home = source["homeAirfieldId"].as_str().map(str::to_owned);
            e.transport = source["transportId"].as_str().map(str::to_owned);
            e.target = source["acquiredTargetId"].as_str().map(str::to_owned);
            e.fuel = number(source, "fuel", e.fuel);
            e.capture_owner = number(source, "captureOwner", -1.) as i32;
            e.capture_progress = number(source, "captureProgress", 0.);
            e.view.town_blocks = source["townBlocks"].as_array().cloned().unwrap_or_default();
            if let Some(blocks) = source["blockingFootprint"].as_array() {
                e.blocking = blocks.iter().filter_map(read_point).collect();
            }
            e.patrol_origin = match (
                source["patrolOriginX"].as_f64(),
                source["patrolOriginY"].as_f64(),
            ) {
                (Some(x), Some(z)) => Some([x as f32, z as f32]),
                _ => None,
            };
            if kind == "bridge" {
                e.bridge_end = match (source["endX"].as_f64(), source["endY"].as_f64()) {
                    (Some(x), Some(z)) => Some([x as f32, z as f32]),
                    _ => None,
                };
            }
            if let Some(path) = source["path"].as_array() {
                e.path = path
                    .iter()
                    .skip(number(source, "pathIndex", 0.) as usize)
                    .filter_map(read_point)
                    .collect();
            }
        }
        if let Some(orders) = state["orders"].as_array() {
            for o in orders {
                let id = o["unitId"].as_str().unwrap_or("");
                if let Some(e) = out.entities.iter_mut().find(|e| e.view.id == id) {
                    e.orders.push_back(Order {
                        kind: o["commandType"]
                            .as_str()
                            .or(o["type"].as_str())
                            .unwrap_or("move")
                            .into(),
                        destination: read_point(&o["destination"]).or_else(|| read_point(o)),
                        target: o["targetId"].as_str().map(str::to_owned),
                    });
                }
            }
        }
        if let Some(jobs) = state["jobs"].as_array() {
            for j in jobs {
                let kind = j["type"].as_str().ok_or("Job has no unit type")?;
                if !spec(kind).is_object() {
                    return Err("Unknown job type.".into());
                }
                let owner = number(j, "owner", -1.) as i32;
                if !(0..=1).contains(&owner) {
                    return Err("Invalid job owner".into());
                }
                let id = out.id();
                out.jobs.push(Job {
                    id,
                    owner,
                    kind: j["kind"].as_str().unwrap_or("recruit").into(),
                    unit_type: kind.into(),
                    producer: j["producerId"]
                        .as_str()
                        .or(j["barracksId"].as_str())
                        .map(str::to_owned),
                    at: [number(j, "x", 0.), number(j, "y", 0.)],
                    cost: number(j, "cost", 0.),
                    progress: number(j, "progressTicks", 0.) as u32,
                    duration: number(j, "durationTicks", number(j, "durationSeconds", 8.) * 20.)
                        .max(1.) as u32,
                    end: match (j["endX"].as_f64(), j["endY"].as_f64()) {
                        (Some(x), Some(z)) => Some([x as f32, z as f32]),
                        _ => None,
                    },
                });
            }
        }
        if let Some(ps) = state["projectiles"].as_array() {
            for p in ps {
                let to = [number(p, "impactX", 0.), number(p, "impactY", 0.)];
                out.projectiles.push(Projectile {
                    owner: number(p, "owner", 0.) as i32,
                    from: to,
                    to,
                    born: out.tick,
                    impact: (number(p, "impactTick", out.tick as f32 + 1.) as u64)
                        .max(out.tick + 1),
                    damage: number(p, "damage", 0.),
                    radius: number(p, "damageRadius", 0.2),
                    hit: flag(p, "hit"),
                    weapon: String::new(),
                    ground_hit: false,
                });
            }
        }
        if let Some(modifications) = v["terrain"]["modifications"].as_array() {
            for m in modifications {
                let x = number(m, "x", 0.);
                let z = number(m, "y", 0.);
                let p = &m["patch"];
                let mut cell = out.terrain.cell(x, z);
                cell.elevation = number(p, "elevation", cell.elevation);
                cell.surface = number(p, "surfaceElevation", cell.surface);
                if let Some(kind) = p["terrainType"].as_str() {
                    cell.kind = [
                        "deep_water",
                        "shallow_water",
                        "river",
                        "lowland",
                        "plains",
                        "hill",
                        "mountain",
                    ]
                    .iter()
                    .position(|k| *k == kind)
                    .unwrap_or(4) as u8;
                }
                out.terrain
                    .edits
                    .insert(format!("{},{}", x as i32, z as i32), cell);
            }
        }
        out.counter = out.counter.max(number(state, "nextRuntimeId", 0.) as u64) + 10000;
        out.paused = true;
        out.notice =
            "Web checkpoint imported and paused. Space resumes in the native simulation.".into();
        out.refresh_towns();
        out.ground_roster_cleanup();
        out.validate()?;
        Ok(out)
    }
    pub fn export_map(&self) -> Result<std::path::PathBuf, String> {
        let path = save_path().with_file_name("native-map.json");
        let mut elevation = Vec::with_capacity((self.terrain.cols * self.terrain.rows) as usize);
        for z in 0..self.terrain.rows {
            for x in 0..self.terrain.cols {
                elevation.push(self.terrain.cell(x as f32, z as f32).elevation);
            }
        }
        let scale = self
            .terrain
            .data
            .as_ref()
            .map(|data| data.scale)
            .unwrap_or(self.terrain.authored_scale);
        let value = json!({"id":"custom-native-map","name":"Native authored map","cols":self.terrain.cols,"rows":self.terrain.rows,"authoredTerrain":{"elevation":elevation,"waterLevel":self.terrain.water_level,"maxElevation":scale}});
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            &path,
            serde_json::to_vec(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(path)
    }
}
