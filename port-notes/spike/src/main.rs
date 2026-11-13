use fancy_regex::Regex;
use std::{fs, io::{BufRead, BufReader}};

fn main() {
    let s = env!("CARGO_MANIFEST_DIR").to_string() + "/..";
    let rx = Regex::new(&fs::read_to_string(format!("{s}/golden/regex/regex.txt")).unwrap()).unwrap();
    let (mut total, mut bad_files) = (0, 0);
    let mut names: Vec<_> = fs::read_dir(format!("{s}/golden/regex/captures")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    let t0 = std::time::Instant::now();
    for g in names {
        let log = g.trim_end_matches(".json");
        let golden: Vec<Vec<serde_json::Value>> = serde_json::from_str(&fs::read_to_string(format!("{s}/golden/regex/captures/{g}")).unwrap()).unwrap();
        let bytes = fs::read(format!("/Users/ivy/GCGC/datasets/{log}")).unwrap();
        let mut ours = Vec::new();
        for (ln, line) in BufReader::new(&bytes[..]).split(b'\n').enumerate() {
            let line = String::from_utf8_lossy(&line.unwrap()).into_owned();
            if let Some(c) = rx.captures(&line).unwrap() {
                let mut row = vec![serde_json::json!(ln)];
                for i in 1..c.len() { row.push(c.get(i).map_or(serde_json::Value::Null, |m| m.as_str().into())); }
                ours.push(row);
            }
        }
        let same = ours == golden;
        total += ours.len();
        if !same {
            bad_files += 1;
            let first = ours.iter().zip(&golden).position(|(a, b)| a != b);
            println!("MISMATCH {log}: rust={} py={} first_diff={:?}", ours.len(), golden.len(), first.map(|i| (&ours[i], &golden[i])));
        } else { println!("ok {log} ({} rows)", ours.len()); }
    }
    println!("total rows {total}, mismatched files {bad_files}, {:?}", t0.elapsed());
}
