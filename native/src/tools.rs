//! Headless developer commands. These use the same Rust code as the game.
use crate::{generation, simulation::NativeGame, terrain};
use serde_json::json;
use std::{io::Write, path::Path, time::Instant};

const PRESETS: [(&str, usize); 3] = [
    ("grand-front", 128),
    ("world-war", 512),
    ("galaxy-front", 1024),
];

pub fn run(args: &[String]) -> Option<Result<(), String>> {
    match args.get(1).map(String::as_str) {
        Some("--help" | "-h") => Some({
            println!(
                "Rule of War (Rust/Bevy)\n\
                 Play: cargo run --locked\n\
                 Import: --import <file>\n\
                 Screenshot: --smoke-test <png> [--menu | --operations | --guide | --models | --weapons | --tactics | --towns | --actions | --world | --editor | --lobby | --settings] [--compact]\n\
                 Input/loading/scenery checks: --smoke-test <png> [--camera-check | --loading-check | --scenery-check]\n\
                 Diagnostics: --profile [--uncapped]\n\
                 Headless tools:\n\
                   --generate-terrain <file> <cols> <rows> <seed>\n\
                   --prebuild-terrain <output-directory>\n\
                   --benchmark-scale\n\
                   --benchmark-navigation\n\
                   --benchmark-warfare [--profile]\n\
                   --benchmark-battle <rifles|urban|armour|artillery|air|combined|siege|airstrike> --ticks 1200 --profile\n\
                 Rendered battle: --mock-battle <case> --battle-seconds 90 --profile --offscreen --compact --smoke-test <absolute.png>"
            );
            Ok(())
        }),
        Some("--generate-terrain") => Some(generate(args)),
        Some("--prebuild-terrain") => Some(prebuild(args)),
        Some("--prepare-terrain-textures") => Some({
            for size in [512, 1024, 2048] {
                for suffix in ["albedo", "normal", "orm"] {
                    let start = Instant::now();
                    let _ = crate::surfaces::terrain_array(suffix, size);
                    println!(
                        "Terrain {size} {suffix}: {:.1} ms",
                        start.elapsed().as_secs_f64() * 1000.
                    );
                }
            }
            Ok(())
        }),
        Some("--benchmark-navigation") => Some({
            let report = NativeGame::default().navigation_benchmark();
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            Ok(())
        }),
        Some("--benchmark-battle") => Some({
            let case = args.get(2).map(String::as_str).unwrap_or("combined");
            if !crate::simulation::BATTLE_BENCH_CASES.contains(&case) {
                Err("Unknown benchmark battle case".into())
            } else {
                let ticks = args
                    .iter()
                    .position(|a| a == "--ticks")
                    .and_then(|i| args.get(i + 1))
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1200)
                    .clamp(1, 12000);
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &NativeGame::default().benchmark_battle(case, ticks)
                    )
                    .unwrap()
                );
                Ok(())
            }
        }),
        Some("--benchmark-warfare") => Some({
            println!(
                "{}",
                serde_json::to_string_pretty(&NativeGame::default().warfare_benchmark()).unwrap()
            );
            Ok(())
        }),
        Some("--benchmark-scale") => Some(benchmark(args)),
        Some("--export-art") => Some(
            args.get(2)
                .ok_or_else(|| "Usage: --export-art <directory>".into())
                .and_then(|path| crate::art_export::export(Path::new(path))),
        ),
        Some("--export-vehicle-materials") => Some(
            args.get(2)
                .ok_or_else(|| "Usage: --export-vehicle-materials <directory>".into())
                .and_then(|path| crate::material_export::export(Path::new(path))),
        ),
        _ => None,
    }
}

fn dimension(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|n| (10..=crate::terrain::MAX_SIZE as usize).contains(n))
        .ok_or_else(|| "Terrain dimensions must be integers from 10 to 2048.".into())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
    file.write_all(bytes).map_err(|e| e.to_string())
}

fn generate(args: &[String]) -> Result<(), String> {
    if args.len() != 6 {
        return Err("Usage: --generate-terrain <file> <cols> <rows> <seed>".into());
    }
    let (cols, rows) = (dimension(&args[3])?, dimension(&args[4])?);
    let started = Instant::now();
    let data = generation::generate(cols, rows, &args[5]);
    let bytes = terrain::encode(&data, &args[5])?;
    write_new(Path::new(&args[2]), &bytes)?;
    println!(
        "{}: {} x {}, {} bytes, {:.2} ms",
        args[2],
        cols,
        rows,
        bytes.len(),
        started.elapsed().as_secs_f64() * 1000.
    );
    Ok(())
}

fn prebuild(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err("Usage: --prebuild-terrain <output-directory>".into());
    }
    let directory = Path::new(&args[2]);
    for (id, _) in PRESETS {
        let path = directory.join(format!("{id}-v3.terrain"));
        if path.exists() {
            return Err(format!("Output already exists: {}", path.display()));
        }
    }
    for (id, size) in PRESETS {
        let path = directory.join(format!("{id}-v3.terrain"));
        generate(&[
            args[0].clone(),
            "--generate-terrain".into(),
            path.to_string_lossy().into_owned(),
            size.to_string(),
            size.to_string(),
            id.into(),
        ])?;
    }
    Ok(())
}

fn benchmark(args: &[String]) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Usage: --benchmark-scale".into());
    }
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../config.json")).map_err(|e| e.to_string())?;
    let mut results = vec![];
    for (id, size) in PRESETS {
        let started = Instant::now();
        let mut game =
            NativeGame::operation(id, size as i32, size as i32, config["defaults"].clone());
        let startup_ms = started.elapsed().as_secs_f64() * 1000.;
        let entities = game.entities.len();
        let started = Instant::now();
        for _ in 0..200 {
            if game.winner.is_some() {
                break;
            }
            game.step();
        }
        let simulation_ms = started.elapsed().as_secs_f64() * 1000.;
        let started = Instant::now();
        let snapshot = game.snapshot();
        let snapshot_ms = started.elapsed().as_secs_f64() * 1000.;
        results.push(json!({
            "map": id, "cols": size, "rows": size, "initialEntities": entities,
            "startupMs": startup_ms, "ticks": game.tick, "simulationMs": simulation_ms,
            "averageTickMs": simulation_ms / game.tick.max(1) as f64,
            "snapshotMs": snapshot_ms, "visibleEntities": snapshot.entities.len(),
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&results).map_err(|e| e.to_string())?
    );
    Ok(())
}
