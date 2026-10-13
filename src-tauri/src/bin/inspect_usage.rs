fn main() {
    let directory = std::env::args().nth(1).expect("需要隔离的统计数据目录");
    let settings = toolbox_core::models::Settings::default();
    match toolbox_core::collection::collect(&settings, std::path::Path::new(&directory)) {
        Ok(snapshot) => {
            serde_json::to_writer(
                std::fs::File::create(std::path::Path::new(&directory).join("snapshot.json"))
                    .expect("无法保存隔离统计结果"),
                &snapshot,
            )
            .expect("无法序列化统计结果");
            for source in snapshot.sources {
                let rows: Vec<_> = snapshot
                    .records
                    .iter()
                    .filter(|r| r.agent == source.source.id)
                    .collect();
                println!(
                    "{} status={} files={} records={} tokens={} diagnostics={:?}",
                    source.source.name,
                    source.status,
                    source.files,
                    rows.len(),
                    rows.iter().map(|r| r.usage.total()).sum::<u64>(),
                    source.diagnostics
                );
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
