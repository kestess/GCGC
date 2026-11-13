//! Dump every analysis for the example runs as JSON (native build), for comparing with the wasm build.
//! Usage: cargo run --example dump -- <datasets dir> <out.json>
use gcgc_wasm::Session;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&args[1]);
    let runs: &[(&str, &[&str])] = &[
        ("Serial", &["SerialGC.log"]),
        ("Parallel", &["ParallelGC.log"]),
        ("G1", &["G1GC.log"]),
        ("Shenandoah", &["ShenandoahGC.log"]),
        ("ZGC", &["ZGC.log.0", "ZGC.log.1"]),
        ("GenZGC", &["GenZGC.log"]),
        ("GraalVM", &["GraalVM.log"]),
        ("Code Cache", &["SegmentedCodeCache.log"]),
    ];
    let mut s = Session::new();
    for (label, files) in runs {
        let names = files.iter().map(|f| f.to_string()).collect();
        let texts =
            files.iter().map(|f| String::from_utf8_lossy(&std::fs::read(dir.join(f)).unwrap()).into_owned()).collect();
        s.add_run(label.to_string(), names, texts, true, false);
    }
    let csv = std::fs::read_to_string(dir.join("example_external_data.csv")).unwrap();
    s.set_external("Reads".into(), csv, "Time Seconds".into(), "Reads Completed".into());
    let cat: serde_json::Value = serde_json::from_str(&Session::catalogue()).unwrap();
    let mut out = serde_json::Map::new();
    for a in cat.as_array().unwrap() {
        let id = a["id"].as_str().unwrap();
        out.insert(id.into(), serde_json::from_str(&s.analyse(id, "")).unwrap());
    }
    std::fs::write(&args[2], serde_json::to_string(&out).unwrap()).unwrap();
}
