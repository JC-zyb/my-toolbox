use super::*;
use std::collections::BTreeSet;
use toml_edit::{value, ArrayOfTables, Document, Item, Table};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationChange {
    pub skill_id: String,
    pub agent_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationRequest {
    pub changes: Vec<SkillActivationChange>,
    pub project_path: Option<String>,
    pub allow_copy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationPreview {
    #[serde(flatten)]
    pub preview: BatchSkillPreview,
    pub requires_confirmation: bool,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSkillActivationRequest {
    pub skill_id: String,
    pub changes: Vec<SkillActivationChange>,
    pub project_path: Option<String>,
    pub allow_copy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationDelta {
    pub skill_id: String,
    pub targets: Vec<SkillAgentState>,
    pub backup_paths: Vec<String>,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSkillActivationResult {
    pub requires_confirmation: bool,
    pub preview: Option<SkillActivationPreview>,
    pub result: Option<OperationResult>,
    pub delta: Option<SkillActivationDelta>,
}

fn codex_config_path(home: &Path) -> PathBuf { home.join(".codex/config.toml") }

fn config_path_key(path: &Path) -> String {
    key(path)
}

fn read_config(home: &Path) -> Result<(PathBuf, String, Document)> {
    let path = codex_config_path(home);
    if !exists(&path) { return Ok((path, String::new(), Document::new())); }
    if is_link(&path) { return Err("Codex 配置文件是联接，拒绝写入".into()); }
    let text = String::from_utf8(io(fs::read(&path))?).map_err(|e| format!("Codex 配置不是 UTF-8：{e}"))?;
    let doc = text.parse::<Document>().map_err(|e| format!("Codex 配置解析失败：{e}"))?;
    validate_config(&doc)?;
    Ok((path, text, doc))
}

fn config_entries(doc: &Document) -> Result<Vec<(String, Option<bool>)>> {
    let Some(item) = doc.get("skills").and_then(|s| s.get("config")) else { return Ok(vec![]); };
    let Some(array) = item.as_array_of_tables() else { return Err("skills.config 必须是表数组".into()); };
    let mut out = vec![];
    for table in array.iter() {
        let path = table.get("path").and_then(Item::as_str).ok_or("skills.config 缺少字符串 path")?;
        let enabled = table.get("enabled").map(|v| v.as_bool().ok_or("skills.config.enabled 必须是布尔值")).transpose()?;
        out.push((path.to_string(), enabled));
    }
    Ok(out)
}

fn validate_config(doc: &Document) -> Result<()> {
    let mut seen: BTreeMap<String, Option<bool>> = BTreeMap::new();
    for (path, enabled) in config_entries(doc)? {
        let k = config_path_key(Path::new(&path));
        if let Some(old) = seen.insert(k, enabled) {
            if old != enabled { return Err("Codex 配置中存在互相矛盾的重复技能项".into()); }
        }
    }
    Ok(())
}

impl SkillsService {
    pub fn quick_skill_activation(&mut self, request: QuickSkillActivationRequest) -> Result<QuickSkillActivationResult> {
        if request.changes.is_empty() || request.changes.iter().any(|c| c.skill_id != request.skill_id) {
            return Err("快速启停请求中的技能不一致".into());
        }
        let preview = self.preview_skill_activation(SkillActivationRequest {
            changes: request.changes,
            project_path: request.project_path.clone(),
            allow_copy: request.allow_copy,
        })?;
        if preview.requires_confirmation {
            return Ok(QuickSkillActivationResult { requires_confirmation: true, preview: Some(preview), result: None, delta: None });
        }
        let resolutions = preview.preview.rows.iter().map(|row| BatchResolution {
            id: row.id.clone(), choice: row.default_choice.clone(), source_path: Some(row.source_path.clone()),
        }).collect::<Vec<_>>();
        let restart_required = preview.restart_required;
        let result = self.apply_skill_activation(&preview.preview.id, &resolutions)?;
        let matrix = self.skill_matrix(request.project_path.as_deref())?;
        let targets = matrix.rows.iter().find(|row| row.skill_id == request.skill_id)
            .map(|row| row.targets.clone()).unwrap_or_default();
        let backup_paths = result.backup_path.clone().into_iter().collect();
        Ok(QuickSkillActivationResult {
            requires_confirmation: false,
            preview: None,
            result: Some(result),
            delta: Some(SkillActivationDelta { skill_id: request.skill_id, targets, backup_paths, restart_required }),
        })
    }
    pub fn get_discovered_skill(&mut self, id: &str, project: Option<&str>) -> Result<Skill> {
        validate_name(id)?;
        if exists(&self.central.join(id)) { return self.get(id); }
        let matrix = self.skill_matrix(project)?;
        let row = matrix.rows.iter().find(|r| r.skill_id == id).ok_or("技能已不存在")?;
        let path = row.sources.iter().find(|s| s.content_hash.is_some()).ok_or("没有可读的技能内容")?;
        read_skill(Path::new(&path.target_path))
    }
    fn codex_document_paths(&self, row: &SkillMatrixRow) -> Vec<PathBuf> {
        row.sources.iter().filter(|s| ["central", "codex-legacy", "legacy"].contains(&s.agent_id.as_str()))
            .filter_map(|s| document_path(Path::new(&s.target_path)).ok()).collect()
    }

    pub(super) fn codex_matrix_states(&self, rows: &mut [SkillMatrixRow]) -> Result<()> {
        let config = read_config(&self.home);
        for row in rows {
            let global = row.targets.iter().any(|s| s.agent_id == "codex" && s.mode == "central");
            let paths = if global { self.codex_document_paths(row) } else {
                row.targets.iter().filter(|s| s.agent_id == "codex").filter_map(|s| document_path(Path::new(&s.target_path)).ok()).collect()
            };
            let state = row.targets.iter_mut().find(|s| s.agent_id == "codex");
            let Some(state) = state else { continue; };
            state.activation_method = if global { "codex_config" } else { "entry_and_config" }.into();
            if state.error.is_some() { continue; }
            match &config {
                Ok((_, _, doc)) => {
                    let entries = config_entries(doc)?;
                    state.enabled = Some(!paths.is_empty() && paths.iter().all(|path| {
                        entries.iter().find(|(p, _)| config_path_key(Path::new(p)) == config_path_key(path))
                            .map(|(_, enabled)| enabled.unwrap_or(true)).unwrap_or(true)
                    }));
                }
                Err(e) => { state.enabled = None; state.error = Some(e.clone()); }
            }
        }
        Ok(())
    }

    pub(super) fn edited_codex_config(&self, changes: &[(PathBuf, bool)]) -> Result<String> {
        let (_path, text, mut doc) = read_config(&self.home)?;
        let mut wanted: BTreeMap<String, (PathBuf, bool)> = BTreeMap::new();
        for (p, enabled) in changes { wanted.insert(config_path_key(p), (p.clone(), *enabled)); }
        if wanted.is_empty() { return Ok(text); }
        if doc.get("skills").is_none() { doc["skills"] = Item::Table(Table::new()); }
        if doc["skills"].get("config").is_none() { doc["skills"]["config"] = Item::ArrayOfTables(ArrayOfTables::new()); }
        let array = doc["skills"]["config"].as_array_of_tables_mut().ok_or("skills.config 必须是表数组")?;
        let mut changed = BTreeSet::new();
        for table in array.iter_mut() {
            let Some(path_value) = table.get("path").and_then(Item::as_str).map(str::to_string) else { continue; };
            let path_key = config_path_key(Path::new(&path_value));
            if let Some((_, enabled)) = wanted.get(&path_key) {
                table["enabled"] = value(*enabled);
                changed.insert(path_key);
            }
        }
        for (path_key, (path, enabled)) in wanted {
            if changed.contains(&path_key) { continue; }
            let mut table = toml_edit::Table::new();
            table["path"] = value(display(&path));
            table["enabled"] = value(enabled);
            array.push(table);
        }
        Ok(doc.to_string())
    }

    pub fn preview_skill_activation(&mut self, request: SkillActivationRequest) -> Result<SkillActivationPreview> {
        if request.changes.is_empty() { return Err("请选择要变更的技能".into()); }
        let mut all_rows = vec![];
        let mut all_fingerprints = BTreeMap::new();
        let mut all_parents = BTreeMap::new();
        let mut config_changes: BTreeMap<String, Vec<(PathBuf, bool)>> = BTreeMap::new();
        let mut requires_confirmation = false;
        let mut restart_required = false;
        let mut seen = BTreeSet::new();
        for change in &request.changes {
            validate_name(&change.skill_id)?;
            if !["codex", "claude", "dsh"].contains(&change.agent_id.as_str()) { return Err("未知 Agent".into()); }
            if !seen.insert((change.skill_id.clone(), change.agent_id.clone())) { return Err("同一技能与 Agent 只能提交一次".into()); }
            let global_codex = change.agent_id == "codex" && request.project_path.is_none();
            let matrix = self.skill_matrix(request.project_path.as_deref())?;
            let discovered = matrix.rows.iter().find(|r| r.skill_id == change.skill_id).ok_or("技能已不存在")?;
            let state = discovered.targets.iter().find(|s| s.agent_id == change.agent_id).ok_or("未知 Agent")?;
            if let Some(error) = &state.error { return Err(error.clone()); }
            let action = if global_codex || change.enabled { "import-and-install" } else { "remove" };
            let mut batch = self.preview_batch_skill_operation(BatchSkillRequest {
                skill_ids: vec![change.skill_id.clone()], agent_ids: vec![change.agent_id.clone()],
                project_path: request.project_path.clone(), action: action.into(), conflict_policy: "keep-target".into(), allow_copy: request.allow_copy,
            })?;
            let saved = self.batch_plans.remove(&batch.id).ok_or("启停预览生成失败")?;
            for (p, f) in saved.fingerprints { all_fingerprints.insert(p, f); }
            for (p, f) in saved.parents { all_parents.insert(p, f); }
            for row in &mut batch.rows {
                if row.action == "import" {
                    if !change.enabled { continue; }
                    requires_confirmation = true;
                    if !all_rows.iter().any(|r: &BatchSkillRow| r.id == row.id) { all_rows.push(row.clone()); }
                    continue;
                }
                if global_codex {
                    let id = format!("config:{}", change.skill_id);
                    row.id = id.clone(); row.action = "config".into(); row.target_path = display(&codex_config_path(&self.home)); row.mode = "config".into(); row.conflict = false; row.default_choice = "central".into();
                    read_config(&self.home)?;
                    let mut paths = self.codex_document_paths(discovered);
                    if change.enabled && !exists(&self.central.join(&change.skill_id)) {
                        let filename = document_path(Path::new(&row.source_path))?.file_name().unwrap().to_owned();
                        paths.push(self.central.join(&change.skill_id).join(filename));
                    }
                    if paths.is_empty() { row.default_choice = "noop".into(); }
                    config_changes.insert(id, paths.into_iter().map(|p| (p, change.enabled)).collect());
                    all_fingerprints.entry(display(&codex_config_path(&self.home))).or_insert(fingerprint(&codex_config_path(&self.home))?);
                    all_parents.insert(display(&codex_config_path(&self.home)), self.batch_boundary(&codex_config_path(&self.home))?);
                    restart_required = true;
                } else {
                    if row.conflict || (!state.managed && exists(Path::new(&row.target_path)) && !is_link(Path::new(&row.target_path))) { requires_confirmation = true; }
                    if change.enabled && state.enabled == Some(true) && !row.conflict { row.default_choice = "noop".into(); }
                    if change.agent_id == "codex" && change.enabled {
                        read_config(&self.home)?;
                        let filename = document_path(Path::new(&row.source_path))?.file_name().unwrap().to_owned();
                        config_changes.insert(row.id.clone(), vec![(Path::new(&row.target_path).join(filename), true)]);
                        let config_path = codex_config_path(&self.home);
                        all_fingerprints.insert(display(&config_path), fingerprint(&config_path)?);
                        all_parents.insert(display(&config_path), self.batch_boundary(&config_path)?);
                        restart_required = true;
                    }
                }
                all_rows.push(row.clone());
            }
        }
        let mut hash = Sha256::new();
        for (p, f) in &all_fingerprints { hash.update(p.as_bytes()); hash.update(f.as_bytes()); }
        let conflicts = all_rows.iter().filter(|r| r.conflict).map(|r| r.id.clone()).collect();
        let preview = BatchSkillPreview { id: Uuid::new_v4().to_string(), rows: all_rows, source_fingerprint: format!("{:x}", hash.finalize()), target_fingerprints: all_fingerprints.clone(), conflicts };
        let id = preview.id.clone();
        self.batch_plans.insert(id.clone(), SavedBatch { request: BatchSkillRequest { skill_ids: vec![], agent_ids: vec![], project_path: request.project_path, action: "install".into(), conflict_policy: "keep-target".into(), allow_copy: request.allow_copy }, preview: preview.clone(), fingerprints: all_fingerprints, parents: all_parents, config_changes });
        Ok(SkillActivationPreview { preview, requires_confirmation, restart_required })
    }

    pub fn apply_skill_activation(&mut self, id: &str, resolutions: &[BatchResolution]) -> Result<OperationResult> {
        let restart = self.batch_plans.get(id).map(|p| !p.config_changes.is_empty()).unwrap_or(false);
        let mut result = self.apply_batch_skill_operation(id, resolutions)?;
        if restart { result.message.push_str("；Codex 配置已更新，请重启 Codex 后生效"); }
        Ok(result)
    }
}
