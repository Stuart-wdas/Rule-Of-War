//! Opt-in wall-time measurements for the main-thread systems and navigation.
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
thread_local! { static STACK: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) }; }
static TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn tick(tick: u64) {
    TICK.store(tick, std::sync::atomic::Ordering::Relaxed);
}
fn trace(event: Value) {
    static FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    if let Some(file) = FILE.get_or_init(|| {
        let path = std::env::var("ROW_PROFILE_TRACE").ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(Mutex::new)
    }) {
        use std::io::Write;
        // One complete line under a lock prevents concurrent render/worker
        // measurements from interleaving and corrupting the JSON trace.
        let mut line = event.to_string();
        line.push('\n');
        if let Ok(mut file) = file.lock() {
            let _ = file.write_all(line.as_bytes());
        }
    }
}
pub fn event(mut event: Value) {
    if enabled() {
        event["unixMs"] = json!(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        trace(event);
    }
}
static ENABLED: OnceLock<bool> = OnceLock::new();
static SAMPLES: OnceLock<Mutex<BTreeMap<&'static str, Vec<f64>>>> = OnceLock::new();
#[derive(bevy::prelude::Resource, Default)]
pub struct MainFrame(Option<Instant>);
pub fn begin_frame(mut clock: bevy::prelude::ResMut<MainFrame>) {
    clock.0 = Some(Instant::now());
}
pub fn end_frame(mut clock: bevy::prelude::ResMut<MainFrame>) {
    if let Some(start) = clock.0.take() {
        let ms = start.elapsed().as_secs_f64() * 1000.;
        if ms >= 33.3 {
            trace(
                json!({"stage":"app.main","ms":ms,"tick":TICK.load(std::sync::atomic::Ordering::Relaxed),"unixMs":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()}),
            );
        }
        record("app.main", ms);
    }
}
fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::args().any(|arg| arg == "--profile"))
}
pub struct Span {
    name: &'static str,
    start: Option<Instant>,
    details: Option<Value>,
}
pub fn span(name: &'static str) -> Span {
    if enabled() {
        STACK.with(|s| s.borrow_mut().push(name));
    }
    Span {
        name,
        start: enabled().then(Instant::now),
        details: None,
    }
}
pub fn detailed(name: &'static str, details: impl FnOnce() -> Value) -> Span {
    let mut result = span(name);
    if enabled() {
        result.details = Some(details());
    }
    result
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let ms = start.elapsed().as_secs_f64() * 1000.;
            if ms >= 16.7 {
                let stack = STACK.with(|s| s.borrow().clone());
                trace(
                    json!({"unixMs":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"tick":TICK.load(std::sync::atomic::Ordering::Relaxed),"stage":self.name,"ms":ms,"stack":stack,"details":self.details}),
                );
            }
            STACK.with(|s| {
                s.borrow_mut().pop();
            });
            record(self.name, ms);
        }
    }
}
pub fn record(name: &'static str, ms: f64) {
    if !enabled() {
        return;
    }
    let mut samples = SAMPLES.get_or_init(Default::default).lock().unwrap();
    let values = samples.entry(name).or_default();
    if values.len() < 65536 {
        values.push(ms);
    }
}
pub fn reset() {
    if let Some(samples) = SAMPLES.get() {
        samples.lock().unwrap().clear();
    }
}
pub fn report() -> Value {
    let Some(samples) = SAMPLES.get() else {
        return json!({});
    };
    let samples = samples.lock().unwrap();
    let rows:BTreeMap<_,_>=samples.iter().filter(|(_,v)|!v.is_empty()).map(|(name,values)| {
        let mut sorted=values.clone();sorted.sort_by(f64::total_cmp);
        let percentile=|p:f64|sorted[((sorted.len()-1) as f64*p).ceil() as usize];
        (*name,json!({"calls":values.len(),"totalMs":values.iter().sum::<f64>(),"p50Ms":percentile(0.5),"p95Ms":percentile(0.95),"p99Ms":percentile(0.99),"maxMs":sorted.last().unwrap(),
            "over16_7Ms":values.iter().filter(|v|**v>16.7).count(),"over33_3Ms":values.iter().filter(|v|**v>33.3).count(),
            "over50Ms":values.iter().filter(|v|**v>50.).count(),"over100Ms":values.iter().filter(|v|**v>100.).count()}))
    }).collect();
    json!(rows)
}
