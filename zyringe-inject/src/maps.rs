

use std::fs;
use std::path::Path;


const MONO_MARKERS: &[&str] = &[
    "libmonobdwgc-2.0",
    "libmonosgen-2.0",
    "libmono-2.0",
    "libmonoboehm",
    "libmono",
];


pub fn load_base<F>(maps_path: &str, pred: F) -> Option<u64>
where
    F: Fn(&str) -> bool,
{
    let content = fs::read_to_string(maps_path).ok()?;
    let mut best: Option<u64> = None;
    for line in content.lines() {
        let (range, path) = split_line(line)?;
        let base = match path {
            Some(p) => p,
            None => continue,
        };
        let name = Path::new(base).file_name().and_then(|f| f.to_str()).unwrap_or("");
        if pred(name) {
            let start = parse_start(range)?;
            best = Some(match best {
                Some(b) => b.min(start),
                None => start,
            });
        }
    }
    best
}


pub fn find_mono_so(pid: i32) -> Option<String> {
    let content = fs::read_to_string(format!("/proc/{pid}/maps")).ok()?;
    let mut native_fallback: Option<String> = None;
    for line in content.lines() {
        if let Some((_, Some(path))) = split_line(line) {
            let name = Path::new(path).file_name().and_then(|f| f.to_str()).unwrap_or("");
            if MONO_MARKERS.iter().any(|m| name.contains(m)) {
                if name.contains("native") {
                    native_fallback.get_or_insert_with(|| path.to_string());
                } else {
                    return Some(path.to_string());
                }
            }
        }
    }
    native_fallback
}


pub fn is_libc(name: &str) -> bool {
    name.starts_with("libc.so") || name.starts_with("libc-")
}


pub fn libc_mapping(pid: i32) -> Option<(String, u64)> {
    let content = fs::read_to_string(format!("/proc/{pid}/maps")).ok()?;
    let mut best: Option<(String, u64)> = None;
    for line in content.lines() {
        if let Some((range, Some(path))) = split_line(line) {
            let name = Path::new(path).file_name().and_then(|f| f.to_str()).unwrap_or("");
            if is_libc(name) {
                if let Some(start) = parse_start(range) {
                    best = Some(match best {
                        Some((p, b)) if b <= start => (p, b),
                        _ => (path.to_string(), start),
                    });
                }
            }
        }
    }
    best
}


fn split_line(line: &str) -> Option<(&str, Option<&str>)> {
    let mut it = line.splitn(6, ' ');
    let range = it.next()?;

    let path = line.splitn(6, ' ').nth(5).map(str::trim).filter(|s| !s.is_empty());
    Some((range, path))
}

fn parse_start(range: &str) -> Option<u64> {
    let start = range.split('-').next()?;
    u64::from_str_radix(start, 16).ok()
}
