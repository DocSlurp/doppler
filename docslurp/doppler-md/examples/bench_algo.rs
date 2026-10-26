//! In-process timing: `cargo run --release --example bench_algo -- REPS FILE...`.
//! Reads every file once, then converts each REPS times and prints the median CPU time per file
//! (thread CPU from /proc/thread-self/schedstat, falling back to wall time).

use std::time::Instant;

fn cpu_ns() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    s.split_whitespace().next()?.parse().ok()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let reps: usize = args.next().and_then(|r| r.parse().ok()).unwrap_or(5);
    let files: Vec<String> = args.collect();
    let inputs: Vec<(String, String)> = files
        .iter()
        .map(|f| (f.clone(), String::from_utf8_lossy(&std::fs::read(f).expect("read")).into_owned()))
        .collect();
    let p = doppler_md::Params::default();
    let mut total = 0.0;
    let mut pages_total = 0usize;
    for (name, x) in &inputs {
        let mut times = Vec::new();
        let pages;
        // batches of at least ~100 ms so the CPU clock's granularity does not matter
        let t0 = Instant::now();
        pages = std::hint::black_box(doppler_md::to_markdown(x, &p)).len();
        let once = t0.elapsed().as_secs_f64().max(1e-6);
        let k = ((0.1 / once).ceil() as usize).clamp(1, 1000);
        for _ in 0..reps {
            let (c0, t0) = (cpu_ns(), Instant::now());
            for _ in 0..k {
                std::hint::black_box(doppler_md::to_markdown(x, &p));
            }
            let dt = match (c0, cpu_ns()) {
                (Some(a), Some(b)) => (b - a) as f64 / 1e9,
                _ => t0.elapsed().as_secs_f64(),
            };
            times.push(dt / k as f64);
        }
        times.sort_by(|a, b| a.total_cmp(b));
        let med = times[times.len() / 2];
        total += med;
        pages_total += pages;
        let base = name.rsplit('/').next().unwrap_or(name);
        println!("{med:>9.4}s {pages:>5}p {base}");
    }
    println!("{total:>9.4}s {pages_total:>5}p TOTAL ({:.0} pages/min)", pages_total as f64 / total * 60.0);
}
