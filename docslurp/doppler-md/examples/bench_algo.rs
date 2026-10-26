//! In-process timing: `cargo run --release --example bench_algo -- REPS FILE...`.
//! Reads every file once, then converts each REPS times (in batches of at least ~100 ms) and
//! prints the median CPU time per file of the whole conversion and of `model::parse` alone;
//! their difference is the layout/table/paragraph share. CPU time is the thread's
//! (/proc/thread-self/schedstat), falling back to wall time.

use std::time::Instant;

fn cpu_ns() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    s.split_whitespace().next()?.parse().ok()
}

fn timed(reps: usize, f: &dyn Fn()) -> f64 {
    let t0 = Instant::now();
    f();
    let once = t0.elapsed().as_secs_f64().max(1e-6);
    let k = ((0.1 / once).ceil() as usize).clamp(1, 1000);
    let mut times = Vec::new();
    for _ in 0..reps {
        let (c0, t0) = (cpu_ns(), Instant::now());
        for _ in 0..k {
            f();
        }
        let dt = match (c0, cpu_ns()) {
            (Some(a), Some(b)) => (b - a) as f64 / 1e9,
            _ => t0.elapsed().as_secs_f64(),
        };
        times.push(dt / k as f64);
    }
    times.sort_by(|a, b| a.total_cmp(b));
    times[times.len() / 2]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let reps: usize = args.next().and_then(|r| r.parse().ok()).unwrap_or(5);
    let files: Vec<String> = args.collect();
    let p = doppler_md::Params::default();
    let (mut total, mut parse_total, mut pages_total) = (0.0, 0.0, 0usize);
    for name in &files {
        let x = String::from_utf8_lossy(&std::fs::read(name).expect("read")).into_owned();
        let pages = doppler_md::to_markdown(&x, &p).len();
        let all = timed(reps, &|| {
            std::hint::black_box(doppler_md::to_markdown(&x, &p));
        });
        let parse = timed(reps, &|| {
            std::hint::black_box(doppler_md::model::parse(&x));
        });
        total += all;
        parse_total += parse;
        pages_total += pages;
        let base = name.rsplit('/').next().unwrap_or(name);
        println!("{all:>9.4}s parse {parse:>8.4}s algo {:>8.4}s {pages:>5}p {base}", all - parse);
    }
    println!(
        "{total:>9.4}s parse {parse_total:>8.4}s algo {:>8.4}s {pages_total:>5}p TOTAL ({:.0} pages/min, algo-only {:.0} pages/min)",
        total - parse_total,
        pages_total as f64 / total * 60.0,
        pages_total as f64 / (total - parse_total) * 60.0
    );
}
