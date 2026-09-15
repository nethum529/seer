use std::ffi::OsString;
use std::fs;

pub(crate) fn counters() -> Vec<(OsString, [u64; 4])> {
    let mut all = Vec::new();
    let tasks = fs::read_dir("/proc/self/task").into_iter().flatten();
    for task in tasks.flatten() {
        let mut total = [0_u64; 4];
        let read = |name| fs::read_to_string(task.path().join(name)).unwrap_or_default();
        let sched: Vec<u64> = read("schedstat")
            .split(' ')
            .map(|v| v.trim().parse().unwrap_or(0))
            .collect();
        total[0] += sched.first().unwrap_or(&0);
        total[1] += sched.get(2).unwrap_or(&0);
        total[2] += field(&read("status"), "voluntary_ctxt_switches:");
        total[3] += field(&read("status"), "nonvoluntary_ctxt_switches:");
        all.push((task.file_name(), total));
    }
    all
}

fn field(text: &str, name: &str) -> u64 {
    text.lines()
        .find_map(|line| line.strip_prefix(name))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}
