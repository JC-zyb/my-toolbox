use crate::models::{Record, Settings, Snapshot, Source, SourceStatus, Usage};
use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};
use walkdir::WalkDir;

type Result<T> = std::result::Result<T, String>;
const MAX_LINE: usize = 16 * 1024 * 1024;

pub fn day(timestamp: &str) -> Result<String> {
    let t = DateTime::parse_from_rfc3339(timestamp).map_err(|_| "缺少有效时间戳".to_string())?;
    Ok(t.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
        .format("%Y-%m-%d")
        .to_string())
}

fn count(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}
fn codex_usage(v: &Value) -> Usage {
    Usage {
        input: count(v, "input_tokens"),
        output: count(v, "output_tokens"),
        cached: count(v, "cached_input_tokens"),
        cache_write: 0,
        reasoning: v.get("reasoning_output_tokens").and_then(Value::as_u64),
        calls: None,
        cost: None,
    }
}
fn difference(now: &Usage, previous: &Usage) -> Usage {
    Usage {
        input: now.input.saturating_sub(previous.input),
        output: now.output.saturating_sub(previous.output),
        cached: now.cached.saturating_sub(previous.cached),
        cache_write: 0,
        reasoning: now
            .reasoning
            .zip(previous.reasoning)
            .map(|(a, b)| a.saturating_sub(b)),
        calls: None,
        cost: None,
    }
}

pub fn parse_jsonl(path: &Path, source: &Source) -> Result<Vec<Record>> {
    let file = File::open(path).map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut bytes = Vec::new();
    let mut session = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut project = String::new();
    let mut model = "未知模型".to_string();
    let mut start = String::new();
    let mut parent = None;
    let mut previous = Usage::default();
    let mut has_previous = false;
    let mut events: HashMap<String, Record> = HashMap::new();
    let mut sequence = 0;
    loop {
        bytes.clear();
        let len = reader
            .by_ref()
            .take((MAX_LINE + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if len == 0 {
            break;
        }
        if len > MAX_LINE {
            return Err("单条日志超过 16 MiB，未继续读取；请查看来源诊断".into());
        }
        if !bytes.ends_with(b"\n") {
            break;
        }
        sequence += 1;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|_| format!("第 {sequence} 行格式无效（未记录日志正文）"))?;
        let kind = text(&v, "type").unwrap_or_default();
        let timestamp = text(&v, "timestamp").unwrap_or_default();
        let payload = &v["payload"];
        if kind == "session_meta" {
            session = text(payload, "id").unwrap_or(&session).to_string();
            project = text(payload, "cwd").unwrap_or_default().to_string();
            start = text(payload, "timestamp").unwrap_or(timestamp).to_string();
            parent = text(payload, "forked_from_id")
                .or_else(|| text(payload, "parent_thread_id"))
                .map(String::from);
        }
        if kind == "turn_context" {
            model = text(payload, "model").unwrap_or(&model).to_string();
        }
        if source.id == "claude" {
            if let Some(id) = text(&v, "sessionId") {
                session = id.to_string();
            }
            if let Some(cwd) = text(&v, "cwd") {
                project = cwd.to_string();
            }
        }
        let (identity, usage) = if source.id == "codex" {
            if kind != "event_msg" || text(payload, "type") != Some("token_count") {
                continue;
            }
            let info = &payload["info"];
            if info.is_null() || !info["total_token_usage"].is_object() {
                continue;
            }
            if info["total_token_usage"]["input_tokens"].as_u64().is_none()
                || info["total_token_usage"]["output_tokens"]
                    .as_u64()
                    .is_none()
            {
                return Err("Codex 累计用量缺少输入或输出字段，无法作为零值处理".into());
            }
            let current = codex_usage(&info["total_token_usage"]);
            if !has_previous {
                // 第一条记录可能继承父会话历史，只记本次 last_token_usage 的增量。
                if info["last_token_usage"].is_object() {
                    let last = codex_usage(&info["last_token_usage"]);
                    previous = difference(&current, &last);
                }
                has_previous = true;
            }
            if current.input < previous.input || current.output < previous.output {
                return Err("累计 Token 计数回退；保留上次成功数据，等待兼容的解析规则".into());
            }
            let delta = difference(&current, &previous);
            previous = current;
            if delta.total() == 0 {
                continue;
            }
            (format!("{sequence}"), delta)
        } else {
            if kind != "assistant" {
                continue;
            }
            let msg = &v["message"];
            let u = &msg["usage"];
            if !u.is_object()
                || u["input_tokens"].as_u64().is_none()
                || u["output_tokens"].as_u64().is_none()
            {
                continue;
            }
            model = text(msg, "model").unwrap_or(&model).to_string();
            let id = text(msg, "id")
                .or_else(|| text(&v, "requestId"))
                .ok_or("Claude 用量缺少请求身份，无法安全去重")?;
            let cache_read = count(u, "cache_read_input_tokens");
            let cache_write = count(u, "cache_creation_input_tokens");
            (
                id.to_string(),
                Usage {
                    input: count(u, "input_tokens")
                        .saturating_add(cache_read)
                        .saturating_add(cache_write),
                    output: count(u, "output_tokens"),
                    cached: cache_read,
                    cache_write,
                    reasoning: None,
                    calls: Some(1),
                    cost: None,
                },
            )
        };
        let date = day(timestamp)?;
        if start.is_empty() {
            start = timestamp.into();
        }
        events.insert(
            identity.clone(),
            Record {
                id: identity,
                session_id: session.clone(),
                agent: source.id.clone(),
                project: project.clone(),
                model: model.clone(),
                day: date,
                timestamp: timestamp.into(),
                started_at: start.clone(),
                source_file: path.to_string_lossy().into_owned(),
                parent_id: parent.clone(),
                usage,
            },
        );
    }
    let mut grouped: BTreeMap<String, Record> = BTreeMap::new();
    for record in events.into_values() {
        let key = format!(
            "{}|{}|{}|{}",
            record.agent, record.session_id, record.day, record.model
        );
        match grouped.get_mut(&key) {
            Some(existing) => {
                existing.usage.add(&record.usage);
                if record.timestamp > existing.timestamp {
                    existing.timestamp = record.timestamp;
                }
            }
            None => {
                grouped.insert(key.clone(), Record { id: key, ..record });
            }
        }
    }
    Ok(grouped.into_values().collect())
}

fn ledger_usage(v: &Value) -> Usage {
    Usage {
        input: count(v, "input")
            .saturating_add(count(v, "cacheRead"))
            .saturating_add(count(v, "cacheWrite")),
        output: count(v, "output"),
        cached: count(v, "cacheRead"),
        cache_write: count(v, "cacheWrite"),
        reasoning: v.get("reasoning").and_then(Value::as_u64),
        calls: v.get("calls").and_then(Value::as_u64),
        cost: v
            .get("cost")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0),
    }
}

pub fn parse_dsh(path: &Path) -> Result<Vec<Record>> {
    let f = File::open(path).map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("DSH 账本超过 64 MiB，需升级读取策略".into());
    }
    let v: Value =
        serde_json::from_reader(BufReader::new(f)).map_err(|_| "DSH 账本无效（未记录正文）")?;
    if v["version"].as_u64() != Some(1) {
        return Err("DSH 账本版本不支持；当前仅支持 version 1".into());
    }
    let days = v["days"].as_object().ok_or("DSH 账本缺少 days")?;
    let mut records = vec![];
    for (date, bucket) in days {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "DSH 账本日期无效")?;
        let sessions = bucket["sessions"]
            .as_array()
            .ok_or("DSH 日统计缺少会话明细，无法归因")?;
        for session in sessions {
            let id = text(session, "id").ok_or("DSH 会话缺少 id")?;
            let timestamp = session["at"]
                .as_i64()
                .and_then(|ts| {
                    DateTime::from_timestamp_millis(ts).or_else(|| DateTime::from_timestamp(ts, 0))
                })
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| format!("{date}T00:00:00+08:00"));
            let models = session["byProviderModel"]
                .as_object()
                .ok_or("DSH 会话缺少模型明细")?;
            for (model, usage) in models {
                if usage["input"].as_u64().is_none() || usage["output"].as_u64().is_none() {
                    return Err("DSH 模型用量缺少输入或输出字段".into());
                }
                records.push(Record {
                    id: format!("dsh|{id}|{date}|{model}"),
                    session_id: id.into(),
                    agent: "dsh".into(),
                    project: "DSH 本地账本".into(),
                    model: model.clone(),
                    day: date.clone(),
                    started_at: timestamp.clone(),
                    timestamp: timestamp.clone(),
                    source_file: path.to_string_lossy().into_owned(),
                    parent_id: None,
                    usage: ledger_usage(usage),
                });
            }
        }
    }
    Ok(records)
}

pub fn collect(settings: &Settings, directory: &Path) -> Result<Snapshot> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut conn = Connection::open(directory.join("usage.sqlite")).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, source TEXT NOT NULL, stamp TEXT NOT NULL, records TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);").map_err(|e|e.to_string())?;
    let mut records: BTreeMap<String, Record> = BTreeMap::new();
    let mut statuses = vec![];
    let now = Utc::now().to_rfc3339();
    for source in &settings.sources {
        let mut diagnostics = vec![];
        let mut paths = vec![];
        let root = Path::new(&source.root);
        if source.enabled && root.is_dir() {
            if source.id == "dsh" {
                let path = root.join("storages/cost-meter/ledger.json");
                if path.is_file() {
                    paths.push(path);
                } else {
                    diagnostics.push("未找到 cost-meter/ledger.json；不推算原始会话用量".into());
                }
            } else {
                let folders: &[&str] = if source.id == "codex" {
                    &["sessions", "archived_sessions"]
                } else {
                    &["projects"]
                };
                for folder in folders {
                    let directory = root.join(folder);
                    if !directory.exists() {
                        continue;
                    }
                    for entry in WalkDir::new(directory).follow_links(false) {
                        match entry {
                            Ok(entry)
                                if entry.file_type().is_file()
                                    && entry.path().extension().is_some_and(|s| s == "jsonl") =>
                            {
                                paths.push(entry.into_path())
                            }
                            Err(_) => {
                                diagnostics.push("部分会话目录无法读取，覆盖范围不完整".into())
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else if source.enabled {
            diagnostics.push("数据目录不存在；可在数据源中指定目录".into());
        }
        let mut source_records = vec![];
        let mut updates = vec![];
        for path in &paths {
            let path_string = path.to_string_lossy().into_owned();
            let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
            let stamp = format!(
                "parser-v2:{}:{:?}",
                meta.len(),
                meta.modified().map_err(|e| e.to_string())?
            );
            let cached: Option<(String, String)> = conn
                .query_row(
                    "SELECT stamp,records FROM files WHERE path=?1",
                    params![path_string],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some((old_stamp, json)) = &cached {
                if old_stamp == &stamp {
                    source_records.extend(
                        serde_json::from_str::<Vec<Record>>(json).map_err(|e| e.to_string())?,
                    );
                    continue;
                }
            }
            let parsed = if source.id == "dsh" {
                parse_dsh(path)
            } else {
                parse_jsonl(path, source)
            };
            match parsed {
                Ok(parsed) => {
                    updates.push((
                        path_string,
                        stamp,
                        serde_json::to_string(&parsed).map_err(|e| e.to_string())?,
                    ));
                    source_records.extend(parsed);
                }
                Err(error) => {
                    if diagnostics.len() < 20 {
                        diagnostics.push(format!(
                            "{}：{error}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                    if let Some((_, json)) = cached {
                        source_records.extend(
                            serde_json::from_str::<Vec<Record>>(&json)
                                .map_err(|e| e.to_string())?,
                        );
                    }
                }
            }
        }
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for (path, stamp, json) in updates {
            tx.execute("INSERT INTO files(path,source,stamp,records) VALUES(?1,?2,?3,?4) ON CONFLICT(path) DO UPDATE SET stamp=excluded.stamp,records=excluded.records,source=excluded.source",params![path,source.id,stamp,json]).map_err(|e|e.to_string())?;
        }
        if !source.enabled || paths.is_empty() {
            let mut stmt = tx
                .prepare("SELECT records FROM files WHERE source=?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![source.id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            for row in rows {
                source_records.extend(
                    serde_json::from_str::<Vec<Record>>(&row.map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?,
                );
            }
        }
        let last_activity = source_records.iter().map(|r| r.timestamp.clone()).max();
        let key = format!("scan:{}", source.id);
        if source.enabled && diagnostics.is_empty() {
            tx.execute(
                "INSERT OR REPLACE INTO metadata(key,value) VALUES(?1,?2)",
                params![key, now],
            )
            .map_err(|e| e.to_string())?;
        }
        let last_scan = tx
            .query_row(
                "SELECT value FROM metadata WHERE key=?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        let status = if !source.enabled {
            "paused"
        } else if !root.is_dir() {
            "missing"
        } else if !diagnostics.is_empty() {
            if source_records.is_empty() {
                "error"
            } else {
                "partial"
            }
        } else if source_records.is_empty() {
            "empty"
        } else {
            "ready"
        };
        for record in source_records {
            // 活动目录与归档可能包含同一会话，取同一会话桶的完整记录，不叠加。
            match records.get(&record.id) {
                Some(old) if old.usage.total() >= record.usage.total() => {}
                _ => {
                    records.insert(record.id.clone(), record);
                }
            }
        }
        statuses.push(SourceStatus {
            source: source.clone(),
            status: status.into(),
            last_scan,
            last_activity,
            files: paths.len(),
            diagnostics,
        });
    }
    Ok(Snapshot {
        sources: statuses,
        records: records.into_values().collect(),
        collected_at: now,
        data_directory: directory.to_string_lossy().into_owned(),
    })
}

pub fn load_settings(directory: &Path) -> Result<Settings> {
    let path = directory.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    serde_json::from_reader(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("设置文件损坏，未覆盖：{e}"))
}
pub fn save_settings(directory: &Path, settings: &Settings) -> Result<()> {
    if settings.skills.central_path != "~/.agents/skills" && !std::path::Path::new(&settings.skills.central_path).is_absolute()
        || !["auto","symlink","junction","copy"].contains(&settings.skills.default_sync_mode.as_str())
        || !["pending","completed","skipped"].contains(&settings.skills.migration.status.as_str())
        || settings.skills.projects.iter().any(|p| !std::path::Path::new(p).is_absolute())
    { return Err("Skills 路径、同步模式或迁移状态无效".into()); }
    if !["light", "dark", "system"].contains(&settings.theme.as_str())
        || !(30..=3600).contains(&settings.refresh_seconds)
    {
        return Err("设置的主题或采集间隔无效".into());
    }
    let mut ids = std::collections::HashSet::new();
    for source in &settings.sources {
        if !["codex", "claude", "dsh"].contains(&source.id.as_str()) || !ids.insert(&source.id) {
            return Err("数据源重复或不支持".into());
        }
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let temporary = directory.join("settings.tmp");
    let file = File::create(&temporary).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&file, settings).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(temporary, directory.join("settings.json")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn source() -> Source {
        Source {
            id: "codex".into(),
            name: "Codex".into(),
            root: String::new(),
            executable: String::new(),
            enabled: true,
            version: None,
        }
    }
    #[test]
    fn cumulative_usage_excludes_inherited_history_and_repeats() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for (input, last) in [(1100, 100), (1100, 100), (1300, 200)] {
            writeln!(f,"{}",serde_json::json!({"timestamp":"2026-10-12T12:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":input,"output_tokens":0},"last_token_usage":{"input_tokens":last,"output_tokens":0}}}})).unwrap();
        }
        let r = parse_jsonl(f.path(), &source()).unwrap();
        assert_eq!(r[0].usage.input, 300);
        assert_eq!(r[0].usage.calls, None);
    }
    #[test]
    fn claude_replaces_streaming_and_cache_is_a_subset() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for output in [1, 10] {
            writeln!(f,"{}",serde_json::json!({"type":"assistant","timestamp":"2026-10-12T12:00:00Z","sessionId":"session","message":{"id":"request","model":"claude","usage":{"input_tokens":20,"output_tokens":output,"cache_read_input_tokens":100,"cache_creation_input_tokens":50}}})).unwrap();
        }
        let mut s = source();
        s.id = "claude".into();
        let r = parse_jsonl(f.path(), &s).unwrap();
        assert_eq!(r[0].usage.total(), 180);
        assert_eq!(r[0].usage.cached, 100);
        assert_eq!(r[0].usage.calls, Some(1));
    }
    #[test]
    fn partial_tail_and_shanghai_day() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{{unfinished").unwrap();
        assert!(parse_jsonl(f.path(), &source()).unwrap().is_empty());
        assert_eq!(day("2026-10-11T23:30:00Z").unwrap(), "2026-10-12");
    }
    #[test]
    fn dsh_models_do_not_add_day_totals_twice() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        serde_json::to_writer(&mut f,&serde_json::json!({"version":1,"days":{"2026-10-12":{"input":9999,"sessions":[{"id":"one","at":1791806400000i64,"byProviderModel":{"p:m":{"input":20,"output":10,"cacheRead":100,"cacheWrite":5,"calls":1,"cost":0.1}}}]}}})).unwrap();
        let r = parse_dsh(f.path()).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].usage.total(), 135);
    }
    #[test]
    fn repeated_scan_is_idempotent_and_failure_keeps_cache() {
        let source_dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        std::fs::create_dir(source_dir.path().join("sessions")).unwrap();
        let path = source_dir.path().join("sessions/one.jsonl");
        std::fs::write(&path,format!("{}\n",serde_json::json!({"timestamp":"2026-10-12T12:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":10},"last_token_usage":{"input_tokens":100,"output_tokens":10}}}}))).unwrap();
        let mut source = source();
        source.root = source_dir.path().to_string_lossy().into_owned();
        let mut settings = Settings::default();
        settings.sources = vec![source];
        let first = collect(&settings, cache.path()).unwrap();
        let second = collect(&settings, cache.path()).unwrap();
        assert_eq!(
            first.records[0].usage.total(),
            second.records[0].usage.total()
        );
        std::fs::write(&path, "invalid log\n").unwrap();
        let failed = collect(&settings, cache.path()).unwrap();
        assert_eq!(failed.records[0].usage.total(), 110);
        assert_eq!(failed.sources[0].status, "partial");
        assert!(!failed.sources[0].diagnostics.is_empty());
    }
}
