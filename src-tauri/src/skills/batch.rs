use super::*;
mod activation;
pub use activation::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillAgentState {
    pub enabled: Option<bool>,
    pub activation_method: String,
    pub capability: String,
    pub capability_reason: Option<String>,
    pub agent_id: String,
    pub state: String,
    pub target_path: String,
    pub mode: String,
    pub managed: bool,
    pub content_hash: Option<String>,
    pub error: Option<String>,
    pub modified: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMatrixRow {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub central_state: String,
    pub targets: Vec<SkillAgentState>,
    pub sources: Vec<SkillAgentState>,
    pub duplicate: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMatrix {
    pub rows: Vec<SkillMatrixRow>,
    pub diagnostics: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSkillRequest {
    pub skill_ids: Vec<String>,
    pub agent_ids: Vec<String>,
    pub project_path: Option<String>,
    pub action: String,
    pub conflict_policy: String,
    pub allow_copy: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSkillRow {
    pub id: String,
    pub skill_id: String,
    pub agent_id: String,
    pub action: String,
    pub source_path: String,
    pub target_path: String,
    pub mode: String,
    pub conflict: bool,
    pub modified: bool,
    pub default_choice: String,
    pub source_options: Vec<String>,
    pub differences: Vec<FileDifference>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSkillPreview {
    pub id: String,
    pub rows: Vec<BatchSkillRow>,
    pub source_fingerprint: String,
    pub target_fingerprints: BTreeMap<String, String>,
    pub conflicts: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResolution {
    pub id: String,
    pub choice: String,
    pub source_path: Option<String>,
}
pub(super) struct SavedBatch {
    request: BatchSkillRequest,
    preview: BatchSkillPreview,
    fingerprints: BTreeMap<String, String>,
    parents: BTreeMap<String, String>,
    config_changes: BTreeMap<String, Vec<(PathBuf, bool)>>,
}

fn mode_at(path: &Path) -> Result<String> {
    if !is_link(path) {
        return Ok("copy".into());
    }
    #[cfg(windows)]
    if junction::exists(path).map_err(|e| e.to_string())? {
        return Ok("junction".into());
    }
    Ok("symlink".into())
}

impl SkillsService {
    fn matrix_entry(
        &self,
        agent: &str,
        path: &Path,
        central_hash: Option<&str>,
        direct: bool,
    ) -> Result<SkillAgentState> {
        let managed = self.managed(path)?;
        let read = if exists(path) {
            Some(read_skill(path))
        } else {
            None
        };
        let hash = read
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|s| s.content_hash.clone());
        let error = read.as_ref().and_then(|r| r.as_ref().err()).cloned();
        let modified = managed
            .as_ref()
            .is_some_and(|t| t.mode == "copy" && hash.as_deref() != Some(&t.source_hash));
        let (capability, capability_reason) = if error.is_some() {
            ("unsupported", Some("读取失败，无法安全启停".into()))
        } else if modified {
            ("requires_confirmation", Some("目标副本已被修改，需要确认后处理".into()))
        } else if read.is_some() && managed.is_none() && !direct {
            ("requires_confirmation", Some("这是外部目录，需要确认备份后处理".into()))
        } else {
            ("toggleable", None)
        };
        let state = if error.is_some() {
            "error"
        } else if read.is_none() {
            "missing"
        } else if direct {
            "central_direct"
        } else if central_hash.is_none() {
            "target_only"
        } else if hash.as_deref() != central_hash {
            "conflict"
        } else {
            "installed"
        };
        Ok(SkillAgentState {
            enabled: if error.is_some() { None } else { Some(read.is_some()) },
            activation_method: if agent == "codex" { "codex_config" } else { "entry" }.into(),
            capability: capability.into(),
            capability_reason,
            agent_id: agent.into(),
            state: state.into(),
            target_path: display(path),
            mode: if direct {
                "central".into()
            } else {
                mode_at(path)?
            },
            managed: managed.is_some(),
            content_hash: hash,
            error,
            modified,
        })
    }
    pub fn skill_matrix(&mut self, project: Option<&str>) -> Result<SkillMatrix> {
        self.skill_matrix_inner(project, None)
    }
    pub fn skill_matrix_for(&mut self, project: Option<&str>, id: &str) -> Result<SkillMatrix> {
        validate_name(id)?;
        self.skill_matrix_inner(project, Some(id))
    }
    fn skill_matrix_for_ids(&mut self, project: Option<&str>, ids: &[String]) -> Result<SkillMatrix> {
        let mut matrix = SkillMatrix { rows: vec![], diagnostics: vec![] };
        for id in ids.iter().collect::<std::collections::BTreeSet<_>>() {
            let part = self.skill_matrix_for(project, id)?;
            matrix.rows.extend(part.rows);
            matrix.diagnostics.extend(part.diagnostics);
        }
        Ok(matrix)
    }
    fn skill_matrix_inner(&mut self, project: Option<&str>, only_id: Option<&str>) -> Result<SkillMatrix> {
        let (mut names, mut diagnostics): (BTreeMap<String, String>, Vec<String>) = if let Some(id) = only_id {
            (BTreeMap::from([(id.to_string(), String::new())]), Vec::new())
        } else {
            let scan = self.scan()?;
            (scan.skills.into_iter().map(|s| (s.id, s.description)).collect(), scan.diagnostics)
        };
        let mut folders = vec![(
            "legacy".to_string(),
            self.home.join(".skills-manager/skills"),
        )];
        let agents = if project.is_none() {
            vec!["codex-legacy", "claude", "dsh"]
        } else {
            vec!["codex", "claude", "dsh"]
        };
        for agent in &agents {
            let path = self.target_path("placeholder", agent, project)?;
            folders.push((agent.to_string(), path.parent().unwrap().to_path_buf()));
        }
        let mut discovery = folders.clone();
        discovery.push(("central".into(), self.central.clone()));
        for (_, folder) in &discovery {
            if only_id.is_some() { break; }
            if !folder.is_dir() {
                continue;
            }
            let entries = match fs::read_dir(folder) {
                Ok(entries) => entries,
                Err(e) => {
                    diagnostics.push(format!("{}：{e}", display(folder)));
                    continue;
                }
            };
            for entry in entries {
                let path = match entry {
                    Ok(e) => e.path(),
                    Err(e) => {
                        diagnostics.push(format!("{}：{e}", display(folder)));
                        continue;
                    }
                };
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with('.')
                    || validate_name(name).is_err()
                    || (!path.is_dir() && !is_link(&path))
                {
                    continue;
                }
                match read_skill(&path) {
                    Ok(s) if s.id == name => {
                        names.entry(name.into()).or_insert(s.description);
                    }
                    Ok(_) => {
                        names.entry(name.into()).or_default();
                        diagnostics.push(format!("{}：目录名与 name 不一致", display(&path)));
                    }
                    Err(e) => {
                        names.entry(name.into()).or_default();
                        diagnostics.push(format!("{}：{e}", display(&path)));
                    }
                }
            }
        }
        let mut rows = vec![];
        for (id, mut description) in names {
            let central = self.matrix_entry("central", &self.central.join(&id), None, false)?;
            let central_hash = central.content_hash.as_deref();
            let mut sources = vec![];
            if central.state != "missing" {
                sources.push(central.clone());
            }
            let mut targets = vec![self.matrix_entry(
                "codex",
                &self.target_path(&id, "codex", project)?,
                central_hash,
                project.is_none(),
            )?];
            for agent in ["claude", "dsh"] {
                targets.push(self.matrix_entry(
                    agent,
                    &self.target_path(&id, agent, project)?,
                    central_hash,
                    false,
                )?);
            }
            for (agent, folder) in &folders {
                let entry = self.matrix_entry(agent, &folder.join(&id), central_hash, false)?;
                if entry.state != "missing" {
                    sources.push(entry);
                }
            }
            if description.is_empty() {
                if let Some(source) = sources.iter().find(|s| s.content_hash.is_some()) {
                    description = read_skill(Path::new(&source.target_path))?.description;
                }
            }
            let hashes: std::collections::BTreeSet<_> = sources
                .iter()
                .filter_map(|s| s.content_hash.as_deref())
                .collect();
            let duplicate = sources.iter().any(|s| {
                !["central", "legacy"].contains(&s.agent_id.as_str())
                    && (!s.managed || s.agent_id == "codex-legacy")
                    && s.content_hash.as_deref() == central_hash
                    && central_hash.is_some()
            });
            rows.push(SkillMatrixRow {
                skill_id: id.clone(),
                name: id,
                description,
                central_state: if central.error.is_some() || hashes.len() > 1 {
                    "conflict"
                } else if central_hash.is_some() {
                    "present"
                } else {
                    "missing"
                }
                .into(),
                targets,
                sources,
                duplicate,
            });
        }
        self.codex_matrix_states(&mut rows)?;
        Ok(SkillMatrix { rows, diagnostics })
    }
    fn batch_boundary(&self, path: &Path) -> Result<String> {
        let parent = path.parent().ok_or("目标无父目录")?;
        if nested(&self.central, parent)? {
            return Err("目标目录与中央库嵌套，禁止修改".into());
        }
        if key(&resolved(parent)?) != key(&absolute(parent)?) {
            return Err(
                "Agent 父目录是联接，禁止跨目录修改；指向旧 .skills-manager 库时请先使用迁移预览"
                    .into(),
            );
        }
        Ok(key(&resolved(parent)?))
    }
    pub fn preview_batch_skill_operation(
        &mut self,
        request: BatchSkillRequest,
    ) -> Result<BatchSkillPreview> {
        let matrix = self.skill_matrix_for_ids(request.project_path.as_deref(), &request.skill_ids)?;
        self.preview_batch_with_matrix(request, &matrix)
    }
    fn preview_batch_with_matrix(&mut self, request: BatchSkillRequest, matrix: &SkillMatrix) -> Result<BatchSkillPreview> {
        if request.skill_ids.is_empty() || request.agent_ids.is_empty() {
            return Err("请选择技能和 Agent".into());
        }
        if !["install", "remove", "import-and-install"].contains(&request.action.as_str())
            || !["keep-target", "choose-per-item"].contains(&request.conflict_policy.as_str())
        {
            return Err("未知批量操作".into());
        }
        let mut rows = vec![];
        let mut fingerprints = BTreeMap::new();
        let mut parents = BTreeMap::new();
        let mut ids = request.skill_ids.clone();
        ids.sort();
        ids.dedup();
        let mut agents = request.agent_ids.clone();
        agents.sort();
        agents.dedup();
        for id in ids {
            validate_name(&id)?;
            let item = matrix
                .rows
                .iter()
                .find(|r| r.skill_id == id)
                .filter(|r| !r.sources.is_empty())
                .ok_or("技能已不存在，请重新扫描")?;
            let central = self.central.join(&id);
            let central_exists = exists(&central);
            if request.action == "install" && !central_exists {
                return Err("Agent 独有技能请使用导入中央库并安装".into());
            }
            let options: Vec<_> = item
                .sources
                .iter()
                .filter(|s| s.content_hash.is_some())
                .map(|s| s.target_path.clone())
                .collect();
            let source = if central_exists {
                display(&central)
            } else {
                options.first().cloned().ok_or("没有可用的技能来源")?
            };
            fingerprints.insert(display(&central), fingerprint(&central)?);
            for option in &options {
                fingerprints.insert(option.clone(), fingerprint(Path::new(option))?);
            }
            if request.action != "remove" && !central_exists {
                let distinct: std::collections::BTreeSet<_> = item
                    .sources
                    .iter()
                    .filter_map(|s| s.content_hash.as_deref())
                    .collect();
                rows.push(BatchSkillRow {
                    id: format!("import:{id}"),
                    skill_id: id.clone(),
                    agent_id: "central".into(),
                    action: "import".into(),
                    source_path: source.clone(),
                    target_path: display(&central),
                    mode: "copy".into(),
                    conflict: distinct.len() > 1,
                    modified: false,
                    default_choice: if distinct.len() > 1 {
                        "skip"
                    } else {
                        "central"
                    }
                    .into(),
                    source_options: options.clone(),
                    differences: vec![],
                });
            }
            for agent in &agents {
                if !["codex", "claude", "dsh", "codex-legacy"].contains(&agent.as_str()) {
                    return Err("未知 Agent".into());
                }
                if agent == "codex-legacy" && request.action != "remove" {
                    return Err("Codex 兼容目录仅用于清理入口".into());
                }
                let actual_agent = if request.action == "remove"
                    && agent == "codex"
                    && request.project_path.is_none()
                {
                    "codex-legacy"
                } else {
                    agent
                };
                let target =
                    self.target_path(&id, actual_agent, request.project_path.as_deref())?;
                let direct = agent == "codex"
                    && request.project_path.is_none()
                    && request.action != "remove";
                if !direct {
                    parents.insert(display(&target), self.batch_boundary(&target)?);
                }
                fingerprints.insert(display(&target), fingerprint(&target)?);
                let target_hash = if target.is_dir() {
                    Some(hash_directory(&target)?.0)
                } else {
                    None
                };
                let source_hash = read_skill(Path::new(&source))?.content_hash;
                let managed = self.managed(&target)?;
                let modified = managed.as_ref().is_some_and(|t| {
                    t.mode == "copy" && target_hash.as_deref() != Some(&t.source_hash)
                });
                let conflict = !direct
                    && exists(&target)
                    && (target_hash.as_deref() != Some(&source_hash) || modified);
                let default = if direct || (!exists(&target) && request.action == "remove") {
                    "noop"
                } else if conflict {
                    if request.conflict_policy == "choose-per-item" {
                        ""
                    } else {
                        "skip"
                    }
                } else {
                    "central"
                };
                rows.push(BatchSkillRow {
                    id: format!("{agent}:{id}"),
                    skill_id: id.clone(),
                    agent_id: agent.clone(),
                    action: if direct { "direct" } else { &request.action }.into(),
                    source_path: source.clone(),
                    target_path: display(&target),
                    mode: if direct {
                        "central".into()
                    } else if request.action == "remove" {
                        mode_at(&target)?
                    } else {
                        "auto".into()
                    },
                    conflict,
                    modified,
                    default_choice: default.into(),
                    source_options: options.clone(),
                    differences: if conflict {
                        difference(Path::new(&source), &target)?
                    } else {
                        vec![]
                    },
                });
            }
        }
        let mut hash = Sha256::new();
        for (path, fp) in &fingerprints {
            hash.update(path.as_bytes());
            hash.update(fp.as_bytes());
        }
        let preview = BatchSkillPreview {
            id: Uuid::new_v4().to_string(),
            conflicts: rows
                .iter()
                .filter(|r| r.conflict)
                .map(|r| r.id.clone())
                .collect(),
            rows,
            source_fingerprint: format!("{:x}", hash.finalize()),
            target_fingerprints: fingerprints.clone(),
        };
        if self.batch_plans.len() >= 100 {
            self.batch_plans.clear();
        }
        self.batch_plans.insert(
            preview.id.clone(),
            SavedBatch {
                request,
                preview: preview.clone(),
                fingerprints,
                parents,
                config_changes: BTreeMap::new(),
            },
        );
        Ok(preview)
    }
    pub fn remove_agent_skill_entries(
        &mut self,
        mut request: BatchSkillRequest,
    ) -> Result<BatchSkillPreview> {
        request.action = "remove".into();
        self.preview_batch_skill_operation(request)
    }
    pub fn apply_batch_skill_operation(
        &mut self,
        id: &str,
        resolutions: &[BatchResolution],
    ) -> Result<OperationResult> {
        let plan = self
            .batch_plans
            .remove(id)
            .ok_or("批量预览失效，请重新预览")?;
        for (path, fp) in &plan.fingerprints {
            if fingerprint(Path::new(path))? != *fp {
                return Err(format!("源或目标已变化，请重新预览：{path}"));
            }
        }
        for (path, parent) in &plan.parents {
            if self.batch_boundary(Path::new(path))? != *parent {
                return Err("Agent 路径边界已变化".into());
            }
        }
        for resolution in resolutions {
            if !plan.preview.rows.iter().any(|r| r.id == resolution.id) {
                return Err("未知预览条目".into());
            }
        }
        let mut selected = vec![];
        for row in &plan.preview.rows {
            let resolution = resolutions.iter().find(|r| r.id == row.id);
            let choice = resolution
                .map(|r| r.choice.as_str())
                .unwrap_or(&row.default_choice);
            if !["central", "target", "skip", "noop"].contains(&choice) {
                return Err("请明确处理所有冲突".into());
            }
            if choice == "noop" && row.default_choice != "noop" {
                return Err("条目不能忽略为无变化".into());
            }
            let requested_source = resolution
                .and_then(|r| r.source_path.as_deref())
                .unwrap_or(&row.source_path);
            let source = row
                .source_options
                .iter()
                .find(|p| key(Path::new(p)) == key(Path::new(requested_source)))
                .map(String::as_str)
                .ok_or("来源不属于此预览")?;
            if choice == "target"
                && (row.action == "direct"
                    || row.action == "config"
                    || row.action == "import"
                    || !Path::new(&row.target_path).is_dir())
            {
                return Err("此条目没有可保留的目标版本".into());
            }
            selected.push((row, choice, source));
        }
        // 同一技能只能选择一个中央版本；先发布中央内容，再发布所有入口。
        let mut incoming: BTreeMap<String, String> = BTreeMap::new();
        let mut skipped_imports = std::collections::BTreeSet::new();
        for (row, choice, source) in &selected {
            if row.action == "import" && *choice == "skip" {
                skipped_imports.insert(row.skill_id.clone());
            }
            let path = if *choice == "target" {
                Some(row.target_path.as_str())
            } else if row.action == "import" && *choice == "central" {
                Some(*source)
            } else {
                None
            };
            if let Some(path) = path {
                let skill = read_skill(Path::new(path))?;
                if skill.id != row.skill_id {
                    return Err("来源名称与技能不一致".into());
                }
                if let Some(other) = incoming.get(&row.skill_id) {
                    if hash_directory(Path::new(other))?.0 != skill.content_hash {
                        return Err("同一技能选择了不同的中央版本，请统一选择".into());
                    }
                }
                incoming.insert(row.skill_id.clone(), path.into());
            }
        }
        let mut published: Vec<(PathBuf, Option<PathBuf>)> = vec![];
        let mut staged = vec![];
        let mut backups = vec![];
        let mut applied = 0;
        let mut skipped = 0;
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let outcome = (|| -> Result<()> {
            for (skill_id, path) in &incoming {
                if skipped_imports.contains(skill_id) {
                    continue;
                }
                let target = self.central.join(skill_id);
                if is_link(&target) {
                    return Err("中央内容为链接，禁止替换".into());
                }
                let stage = self.stage(Path::new(path), &self.central)?;
                staged.push(stage.clone());
                let old = self.publish(&stage, &target)?;
                if let Some(ref b) = old {
                    backups.push(display(b));
                }
                published.push((target.clone(), old.clone()));
                let s = self.get(skill_id)?;
                self.save_skill(&s)?;
                self.log("batch-import", &target, old.as_deref())?;
                applied += 1;
            }
            let mut config_updates = vec![];
            for (row, choice, _) in &selected {
                if !["skip", "noop"].contains(choice) && !skipped_imports.contains(&row.skill_id) {
                    if let Some(changes) = plan.config_changes.get(&row.id) { config_updates.extend(changes.clone()); }
                }
            }
            if !config_updates.is_empty() {
                let target = self.home.join(".codex/config.toml");
                let text = self.edited_codex_config(&config_updates)?;
                io(fs::create_dir_all(target.parent().unwrap()))?;
                let stage = target.parent().unwrap().join(format!(".toolbox-stage-{}", Uuid::new_v4()));
                staged.push(stage.clone());
                io(fs::write(&stage, text))?;
                let old = self.publish(&stage, &target)?;
                if let Some(ref b) = old { backups.push(display(b)); }
                published.push((target.clone(), old.clone()));
                self.log("codex-activation", &target, old.as_deref())?;
            }
            for (row, choice, _) in &selected {
                if row.action == "import" {
                    if *choice == "skip" {
                        skipped += 1;
                    }
                    continue;
                }
                if *choice == "skip" || *choice == "noop" || skipped_imports.contains(&row.skill_id)
                {
                    skipped += 1;
                    continue;
                }
                if row.action == "config" { applied += 1; continue; }
                let target = PathBuf::from(&row.target_path);
                self.batch_boundary(&target)?;
                if row.action == "remove" {
                    let b = target
                        .parent()
                        .unwrap()
                        .join(format!(".toolbox-backup-{}", Uuid::new_v4()));
                    io(fs::rename(&target, &b))?;
                    published.push((target.clone(), Some(b.clone())));
                    backups.push(display(&b));
                    self.conn
                        .execute(
                            "DELETE FROM skill_targets WHERE target_path=?1",
                            [&row.target_path],
                        )
                        .map_err(|e| e.to_string())?;
                    self.log("batch-remove", &target, Some(&b))?;
                } else {
                    let s = self.get(&row.skill_id)?;
                    io(fs::create_dir_all(target.parent().unwrap()))?;
                    let stage = target
                        .parent()
                        .unwrap()
                        .join(format!(".toolbox-stage-{}", Uuid::new_v4()));
                    staged.push(stage.clone());
                    let mode = create_link(
                        Path::new(&s.central_path),
                        &stage,
                        "auto",
                        plan.request.allow_copy,
                    )?;
                    let old = self.publish(&stage, &target)?;
                    if let Some(ref b) = old {
                        backups.push(display(b));
                    }
                    published.push((target.clone(), old.clone()));
                    self.save_target(&SkillTarget {
                        id: self
                            .managed(&target)?
                            .map(|t| t.id)
                            .unwrap_or_else(|| Uuid::new_v4().to_string()),
                        skill_id: row.skill_id.clone(),
                        agent_id: row.agent_id.clone(),
                        scope: if plan.request.project_path.is_some() {
                            "project"
                        } else {
                            "global"
                        }
                        .into(),
                        project_path: plan.request.project_path.clone(),
                        target_path: row.target_path.clone(),
                        mode,
                        status: "in_sync".into(),
                        last_synced_at: Some(now()),
                        source_hash: s.content_hash,
                        error: None,
                        managed: true,
                    })?;
                    self.log("batch-install", &target, old.as_deref())?;
                }
                applied += 1;
            }
            self.conn
                .execute_batch("COMMIT")
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(e) = outcome {
            let mut errors = vec![];
            if let Err(r) = self.conn.execute_batch("ROLLBACK") {
                errors.push(r.to_string());
            }
            for (path, backup) in published.iter().rev() {
                if exists(path) {
                    if let Err(r) = remove_owned(path) {
                        errors.push(r);
                        continue;
                    }
                }
                if let Some(b) = backup {
                    if let Err(r) = fs::rename(b, path) {
                        errors.push(r.to_string());
                    }
                }
            }
            for path in staged {
                if exists(&path) {
                    if let Err(r) = remove_owned(&path) {
                        errors.push(r);
                    }
                }
            }
            return Err(format!(
                "批量操作失败：{e}；回滚错误：{}",
                if errors.is_empty() {
                    "无".into()
                } else {
                    errors.join("；")
                }
            ));
        }
        Ok(OperationResult {
            message: format!(
                "已完成 {applied} 项，跳过/无需修改 {skipped} 项；中央库与联接来源保留"
            ),
            backup_path: if backups.is_empty() {
                None
            } else {
                Some(backups.join("\n"))
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    fn fixture() -> (TempDir, SkillsService) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("home")).unwrap();
        let service =
            SkillsService::new(root.path().join("data"), root.path().join("home")).unwrap();
        (root, service)
    }
    fn document(name: &str, body: &str) -> String {
        format!("---\nname: {name}\ndescription: test\n---\n{body}\n")
    }
    fn write(path: &Path, name: &str, body: &str) {
        fs::create_dir_all(path.join("references")).unwrap();
        fs::write(path.join("SKILL.md"), document(name, body)).unwrap();
        fs::write(path.join("references/example.txt"), body).unwrap();
    }
    fn request(ids: &[&str], agents: &[&str], action: &str) -> BatchSkillRequest {
        BatchSkillRequest {
            skill_ids: ids.iter().map(|s| s.to_string()).collect(),
            agent_ids: agents.iter().map(|s| s.to_string()).collect(),
            project_path: None,
            action: action.into(),
            conflict_policy: "keep-target".into(),
            allow_copy: false,
        }
    }
    #[test]
    fn matrix_deduplicates_all_sources_and_reports_errors() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        write(&s.home.join(".claude/skills/alpha"), "alpha", "one");
        write(&s.home.join(".skills-manager/skills/alpha"), "alpha", "two");
        write(&s.home.join(".dsh/skills/unique"), "unique", "unique");
        fs::create_dir_all(s.home.join(".claude/skills/broken")).unwrap();
        fs::create_dir_all(s.central.join("broken-central")).unwrap();
        let matrix = s.skill_matrix(None).unwrap();
        assert_eq!(matrix.rows.len(), 4);
        let broken = matrix
            .rows
            .iter()
            .find(|r| r.name == "broken-central")
            .unwrap();
        assert_eq!(broken.central_state, "conflict");
        assert_eq!(broken.targets[0].state, "error");
        let alpha = matrix.rows.iter().find(|r| r.name == "alpha").unwrap();
        assert_eq!(alpha.central_state, "conflict");
        assert!(alpha.duplicate);
        assert_eq!(alpha.targets[0].state, "central_direct");
        assert_eq!(alpha.targets[1].state, "installed");
        let unique = matrix.rows.iter().find(|r| r.name == "unique").unwrap();
        assert_eq!(unique.central_state, "missing");
        assert_eq!(unique.targets[2].state, "target_only");
        assert_eq!(
            matrix
                .rows
                .iter()
                .find(|r| r.name == "broken")
                .unwrap()
                .targets[1]
                .state,
            "error"
        );
        assert!(!matrix.diagnostics.is_empty());
    }
    #[test]
    fn batch_links_and_external_removal_preserve_central_and_sources() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        write(&s.central.join("beta"), "beta", "two");
        let p = s
            .preview_batch_skill_operation(request(
                &["alpha", "beta"],
                &["codex", "claude", "dsh"],
                "install",
            ))
            .unwrap();
        s.apply_batch_skill_operation(&p.id, &[]).unwrap();
        for agent in ["claude", "dsh"] {
            for id in ["alpha", "beta"] {
                let target = s.target_path(id, agent, None).unwrap();
                assert!(is_link(&target));
                assert!(same(&target, &s.central.join(id)).unwrap());
                #[cfg(windows)]
                assert_eq!(mode_at(&target).unwrap(), "junction");
            }
        }
        let codex = s.home.join(".codex/skills/alpha");
        fs::create_dir_all(codex.parent().unwrap()).unwrap();
        create_link(&s.central.join("alpha"), &codex, "auto", false).unwrap();
        let p = s
            .remove_agent_skill_entries(request(
                &["alpha", "beta"],
                &["codex", "claude", "dsh"],
                "install",
            ))
            .unwrap();
        let result = s.apply_batch_skill_operation(&p.id, &[]).unwrap();
        assert!(result.backup_path.is_some());
        assert!(!exists(&codex));
        assert!(s.central.join("alpha/SKILL.md").is_file());
        assert!(s.central.join("beta/references/example.txt").is_file());
        assert_eq!(
            s.skill_matrix(None).unwrap().rows[0].targets[0].state,
            "central_direct"
        );
        assert!(s.stored::<SkillTarget>("skill_targets").unwrap().is_empty());
    }
    #[test]
    fn conflict_defaults_keep_target_and_changed_preview_aborts() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let target = s.home.join(".claude/skills/alpha");
        write(&target, "alpha", "local");
        for action in ["install", "remove"] {
            let p = s
                .preview_batch_skill_operation(request(&["alpha"], &["claude"], action))
                .unwrap();
            assert_eq!(p.rows[0].default_choice, "skip");
            s.apply_batch_skill_operation(&p.id, &[]).unwrap();
            assert!(read_skill(&target).unwrap().document.contains("local"));
        }
        let p = s
            .preview_batch_skill_operation(request(&["alpha"], &["claude"], "install"))
            .unwrap();
        fs::write(target.join("references/example.txt"), "changed").unwrap();
        assert!(s
            .apply_batch_skill_operation(
                &p.id,
                &[BatchResolution {
                    id: p.rows[0].id.clone(),
                    choice: "central".into(),
                    source_path: None
                }]
            )
            .unwrap_err()
            .contains("已变化"));
        assert!(read_skill(&s.central.join("alpha"))
            .unwrap()
            .document
            .contains("one"));
    }
    #[test]
    fn import_agent_only_then_link_and_retain_original() {
        let (_root, mut s) = fixture();
        let origin = s.home.join(".claude/skills/unique");
        write(&origin, "unique", "external");
        let p = s
            .preview_batch_skill_operation(request(
                &["unique"],
                &["dsh", "codex"],
                "import-and-install",
            ))
            .unwrap();
        assert_eq!(p.rows[0].action, "import");
        s.apply_batch_skill_operation(&p.id, &[]).unwrap();
        assert!(!is_link(&origin));
        assert!(origin.join("SKILL.md").is_file());
        assert!(s.central.join("unique/references/example.txt").is_file());
        assert!(is_link(&s.home.join(".dsh/skills/unique")));
    }
    #[test]
    fn batch_database_failure_restores_all_files_and_metadata() {
        let (_root, mut s) = fixture();
        write(&s.home.join(".claude/skills/unique"), "unique", "external");
        let p = s
            .preview_batch_skill_operation(request(
                &["unique"],
                &["claude", "dsh"],
                "import-and-install",
            ))
            .unwrap();
        s.conn.execute_batch("CREATE TRIGGER fail_batch BEFORE INSERT ON skill_targets WHEN NEW.agent_id='dsh' BEGIN SELECT RAISE(FAIL, 'injected'); END;").unwrap();
        let error = s.apply_batch_skill_operation(&p.id, &[]).unwrap_err();
        assert!(error.contains("injected"), "{error}");
        assert!(!exists(&s.central.join("unique")));
        assert!(!exists(&s.home.join(".dsh/skills/unique")));
        assert!(!is_link(&s.home.join(".claude/skills/unique")));
        assert!(s.stored::<SkillTarget>("skill_targets").unwrap().is_empty());
        assert!(s.stored::<Skill>("skills").unwrap().is_empty());
    }
    #[test]
    fn target_version_can_be_imported_and_selected_agents_receive_it() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        write(&s.home.join(".claude/skills/alpha"), "alpha", "local");
        let p = s
            .preview_batch_skill_operation(request(&["alpha"], &["claude", "dsh"], "install"))
            .unwrap();
        s.apply_batch_skill_operation(
            &p.id,
            &[BatchResolution {
                id: "claude:alpha".into(),
                choice: "target".into(),
                source_path: None,
            }],
        )
        .unwrap();
        assert!(s.get("alpha").unwrap().document.contains("local"));
        assert!(same(&s.home.join(".dsh/skills/alpha"), &s.central.join("alpha")).unwrap());
    }
    #[test]
    fn modified_managed_copy_is_not_deleted_or_overwritten() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let p = s
            .preview_sync(SyncRequest {
                skill_id: "alpha".into(),
                agent_id: "claude".into(),
                project_path: None,
                mode: "copy".into(),
                allow_copy: false,
            })
            .unwrap();
        s.apply_sync(&p.id, "central").unwrap();
        let target = s.home.join(".claude/skills/alpha");
        fs::write(target.join("references/example.txt"), "local change").unwrap();
        let p = s
            .remove_agent_skill_entries(request(&["alpha"], &["claude"], "remove"))
            .unwrap();
        assert!(p.rows[0].modified);
        assert_eq!(p.rows[0].default_choice, "skip");
        s.apply_batch_skill_operation(&p.id, &[]).unwrap();
        assert!(target.is_dir());
    }
    #[test]
    fn importing_ambiguous_versions_requires_explicit_source() {
        let (_root, mut s) = fixture();
        write(&s.home.join(".claude/skills/unique"), "unique", "first");
        write(&s.home.join(".dsh/skills/unique"), "unique", "second");
        let p = s
            .preview_batch_skill_operation(request(&["unique"], &["codex"], "import-and-install"))
            .unwrap();
        assert_eq!(p.rows[0].default_choice, "skip");
        s.apply_batch_skill_operation(&p.id, &[]).unwrap();
        assert!(!exists(&s.central.join("unique")));
        let p = s
            .preview_batch_skill_operation(request(&["unique"], &["codex"], "import-and-install"))
            .unwrap();
        s.apply_batch_skill_operation(
            &p.id,
            &[BatchResolution {
                id: "import:unique".into(),
                choice: "central".into(),
                source_path: Some(display(&s.home.join(".dsh/skills/unique"))),
            }],
        )
        .unwrap();
        assert!(s.get("unique").unwrap().document.contains("second"));
    }
    #[cfg(windows)]
    #[test]
    fn linked_agent_root_is_migrated_without_modifying_old_library() {
        let (_root, mut s) = fixture();
        let source = s.home.join(".skills-manager/skills");
        write(&source.join("alpha"), "alpha", "old");
        for agent in ["codex-legacy", "claude", "dsh"] {
            let root = s
                .target_path("alpha", agent, None)
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            fs::create_dir_all(root.parent().unwrap()).unwrap();
            junction::create(&source, &root).unwrap();
        }
        assert!(s
            .preview_batch_skill_operation(request(&["alpha"], &["claude"], "remove"))
            .unwrap_err()
            .contains("父目录是联接"));
        let settings = SkillsSettings::default();
        let p = s.preview_migration(&settings).unwrap();
        let result = s.apply_migration(&p.id, &[]).unwrap();
        assert!(result.backup_path.is_some());
        assert!(source.join("alpha/SKILL.md").is_file());
        assert!(!is_link(&s.home.join(".claude/skills")));
        assert!(same(
            &s.home.join(".claude/skills/alpha"),
            &s.central.join("alpha")
        )
        .unwrap());
        assert!(!exists(&s.home.join(".codex/skills/alpha")));
        let hash = s.get("alpha").unwrap().content_hash;
        let p = s.preview_migration(&settings).unwrap();
        assert_eq!(p.rows[0].status, "identical");
        s.apply_migration(&p.id, &[]).unwrap();
        assert_eq!(s.get("alpha").unwrap().content_hash, hash);
    }
    #[test]
    fn removal_log_failure_restores_external_directory() {
        let (_root, mut s) = fixture();
        let path = s.home.join(".claude/skills/unique");
        write(&path, "unique", "external");
        let p = s
            .remove_agent_skill_entries(request(&["unique"], &["claude"], "remove"))
            .unwrap();
        s.conn.execute_batch("CREATE TRIGGER fail_remove BEFORE INSERT ON skill_operations BEGIN SELECT RAISE(FAIL, 'remove-log'); END;").unwrap();
        assert!(s
            .apply_batch_skill_operation(&p.id, &[])
            .unwrap_err()
            .contains("remove-log"));
        assert!(path.join("references/example.txt").is_file());
        assert!(!is_link(&path));
    }
    #[test]
    fn changed_import_source_aborts_before_central_or_agents_are_written() {
        let (_root, mut s) = fixture();
        let path = s.home.join(".claude/skills/unique");
        write(&path, "unique", "external");
        let p = s
            .preview_batch_skill_operation(request(&["unique"], &["dsh"], "import-and-install"))
            .unwrap();
        fs::write(path.join("new.txt"), "changed").unwrap();
        assert!(s
            .apply_batch_skill_operation(&p.id, &[])
            .unwrap_err()
            .contains("已变化"));
        assert!(!exists(&s.central.join("unique")));
        assert!(!exists(&s.home.join(".dsh/skills/unique")));
    }
    #[test]
    fn project_codex_requires_entry_and_does_not_use_global_direct_state() {
        let (root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        let matrix = s.skill_matrix(Some(&display(&project))).unwrap();
        assert_eq!(matrix.rows[0].targets[0].state, "missing");
        let mut r = request(&["alpha"], &["codex"], "install");
        r.project_path = Some(display(&project));
        let p = s.preview_batch_skill_operation(r).unwrap();
        s.apply_batch_skill_operation(&p.id, &[]).unwrap();
        assert!(is_link(&project.join(".agents/skills/alpha")));
        assert_eq!(
            s.skill_matrix(Some(&display(&project))).unwrap().rows[0].targets[0].state,
            "installed"
        );
    }
    #[test]
    fn codex_activation_preserves_config_and_disables_legacy_document() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        write(&s.home.join(".skills-manager/skills/alpha"), "alpha", "old");
        let config = s.home.join(".codex/config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "# keep this comment\nmodel = \"test\"\n").unwrap();
        let p = s.preview_skill_activation(SkillActivationRequest {
            changes: vec![SkillActivationChange { skill_id: "alpha".into(), agent_id: "codex".into(), enabled: false }],
            project_path: None,
            allow_copy: false,
        }).unwrap();
        assert!(p.restart_required);
        assert!(!p.requires_confirmation);
        assert_eq!(p.preview.rows[0].action, "config");
        s.apply_skill_activation(&p.preview.id, &[]).unwrap();
        let text = fs::read_to_string(&config).unwrap();
        assert!(text.contains("# keep this comment"));
        assert!(text.contains("model = \"test\""));
        assert_eq!(text.matches("enabled = false").count(), 2);
        let state = &s.skill_matrix(None).unwrap().rows[0].targets[0];
        assert_eq!(state.enabled, Some(false));
        assert!(s.central.join("alpha/SKILL.md").is_file());
        assert!(s.home.join(".skills-manager/skills/alpha/SKILL.md").is_file());
    }
    #[test]
    fn quick_activation_is_scoped_and_preserves_backups() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        write(&s.central.join("unrelated"), "unrelated", "must not be scanned");
        s.conn.execute_batch("CREATE TRIGGER forbid_scan BEFORE INSERT ON skills BEGIN SELECT RAISE(FAIL, 'full-scan-forbidden'); END;").unwrap();
        let changes = |enabled| ["codex", "claude", "dsh"].into_iter().map(|agent| SkillActivationChange { skill_id: "alpha".into(), agent_id: agent.into(), enabled }).collect();
        let enabled = s.quick_skill_activation(QuickSkillActivationRequest { skill_id: "alpha".into(), changes: changes(true), project_path: None, allow_copy: false }).unwrap();
        assert!(!enabled.requires_confirmation);
        assert!(enabled.delta.unwrap().targets.iter().all(|t| t.enabled == Some(true)));
        let disabled = s.quick_skill_activation(QuickSkillActivationRequest { skill_id: "alpha".into(), changes: changes(false), project_path: None, allow_copy: false }).unwrap();
        let delta = disabled.delta.unwrap();
        assert_eq!(delta.targets.len(), 3);
        assert!(delta.targets.iter().all(|t| t.enabled == Some(false)));
        assert!(delta.backup_paths.len() >= 2);
        assert!(delta.backup_paths.iter().all(|path| exists(Path::new(path))));
        assert!(s.central.join("alpha/SKILL.md").is_file());
        assert!(s.stored::<Skill>("skills").unwrap().is_empty());
        assert!(s.scan().unwrap_err().contains("full-scan-forbidden"));
    }
    #[test]
    fn quick_activation_only_returns_requested_agents_and_rolls_back_on_failure() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let request = |changes| QuickSkillActivationRequest { skill_id: "alpha".into(), changes, project_path: None, allow_copy: false };
        let change = |agent: &str, enabled| SkillActivationChange { skill_id: "alpha".into(), agent_id: agent.into(), enabled };
        let result = s.quick_skill_activation(request(vec![change("claude", true)])).unwrap();
        assert_eq!(result.delta.unwrap().targets.iter().map(|t| t.agent_id.as_str()).collect::<Vec<_>>(), vec!["claude"]);
        s.conn.execute_batch("CREATE TRIGGER fail_quick BEFORE INSERT ON skill_targets WHEN NEW.agent_id='dsh' BEGIN SELECT RAISE(FAIL, 'quick-failure'); END;").unwrap();
        let error = s.quick_skill_activation(request(vec![change("codex", false), change("dsh", true)])).unwrap_err();
        assert!(error.contains("quick-failure"));
        assert!(!s.home.join(".codex/config.toml").exists());
        assert!(!exists(&s.home.join(".dsh/skills/alpha")));
        assert!(exists(&s.home.join(".claude/skills/alpha")));
        assert_eq!(s.stored::<SkillTarget>("skill_targets").unwrap().len(), 1);
    }
    #[test]
    fn quick_activation_requires_confirmation_for_external_content_and_import() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let external = s.home.join(".claude/skills/alpha");
        write(&external, "alpha", "different");
        let request = |id: &str, agent: &str, enabled| QuickSkillActivationRequest { skill_id: id.into(), changes: vec![SkillActivationChange { skill_id: id.into(), agent_id: agent.into(), enabled }], project_path: None, allow_copy: false };
        let result = s.quick_skill_activation(request("alpha", "claude", false)).unwrap();
        assert!(result.requires_confirmation);
        assert!(result.delta.is_none());
        assert!(fs::read_to_string(external.join("SKILL.md")).unwrap().contains("different"));
        write(&s.home.join(".dsh/skills/unique"), "unique", "agent only");
        let result = s.quick_skill_activation(request("unique", "claude", true)).unwrap();
        assert!(result.requires_confirmation);
        assert!(!s.central.join("unique").exists());
        assert!(result.preview.unwrap().preview.rows.iter().any(|r| r.action == "import"));
    }
    #[test]
    fn codex_activation_rejects_changed_or_contradictory_config() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let config = s.home.join(".codex/config.toml");
        let p = s.preview_skill_activation(SkillActivationRequest {
            changes: vec![SkillActivationChange { skill_id: "alpha".into(), agent_id: "codex".into(), enabled: false }],
            project_path: None,
            allow_copy: false,
        }).unwrap();
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "model = \"external\"\n").unwrap();
        assert!(s.apply_skill_activation(&p.preview.id, &[]).unwrap_err().contains("已变化"));
        assert_eq!(fs::read_to_string(&config).unwrap(), "model = \"external\"\n");
        let path = display(&s.central.join("alpha/SKILL.md"));
        fs::write(&config, format!("[[skills.config]]\npath = '{path}'\nenabled = true\n[[skills.config]]\npath = '{path}'\nenabled = false\n")).unwrap();
        assert!(s.preview_skill_activation(SkillActivationRequest {
            changes: vec![SkillActivationChange { skill_id: "alpha".into(), agent_id: "codex".into(), enabled: false }],
            project_path: None,
            allow_copy: false,
        }).unwrap_err().contains("互相矛盾"));
    }
    #[test]
    fn mixed_activation_failure_restores_config_and_entries() {
        let (_root, mut s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let config = s.home.join(".codex/config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "# original\nmodel = \"test\"\n").unwrap();
        let p = s.preview_skill_activation(SkillActivationRequest {
            changes: vec![
                SkillActivationChange { skill_id: "alpha".into(), agent_id: "codex".into(), enabled: false },
                SkillActivationChange { skill_id: "alpha".into(), agent_id: "dsh".into(), enabled: true },
            ],
            project_path: None,
            allow_copy: false,
        }).unwrap();
        s.conn.execute_batch("CREATE TRIGGER fail_activation BEFORE INSERT ON skill_targets WHEN NEW.agent_id='dsh' BEGIN SELECT RAISE(FAIL, 'mixed-activation'); END;").unwrap();
        assert!(s.apply_skill_activation(&p.preview.id, &[]).unwrap_err().contains("mixed-activation"));
        assert_eq!(fs::read_to_string(&config).unwrap(), "# original\nmodel = \"test\"\n");
        assert!(!exists(&s.home.join(".dsh/skills/alpha")));
        assert!(s.stored::<SkillTarget>("skill_targets").unwrap().is_empty());
    }
    #[cfg(windows)]
    #[test]
    fn auto_link_failure_reports_attempt_order_and_does_not_copy() {
        let (_root, s) = fixture();
        write(&s.central.join("alpha"), "alpha", "one");
        let parent = s.home.join("blocked");
        fs::write(&parent, "file").unwrap();
        let target = parent.join("entry");
        let error = create_link(&s.central.join("alpha"), &target, "auto", false).unwrap_err();
        assert!(
            error.contains("junction:") && error.contains("symlink:"),
            "{error}"
        );
        assert!(error.find("junction:").unwrap() < error.find("symlink:").unwrap());
        assert!(!exists(&target));
        assert!(s.central.join("alpha/SKILL.md").is_file());
    }
    #[cfg(windows)]
    #[test]
    fn root_migration_failure_restores_directory_links_and_database() {
        let (_root, mut s) = fixture();
        let source = s.home.join(".skills-manager/skills");
        write(&source.join("alpha"), "alpha", "old");
        let folder = s.home.join(".claude/skills");
        fs::create_dir_all(folder.parent().unwrap()).unwrap();
        junction::create(&source, &folder).unwrap();
        let p = s.preview_migration(&SkillsSettings::default()).unwrap();
        s.conn.execute_batch("CREATE TRIGGER fail_migration BEFORE INSERT ON skill_targets BEGIN SELECT RAISE(FAIL, 'root-migration'); END;").unwrap();
        assert!(s
            .apply_migration(&p.id, &[])
            .unwrap_err()
            .contains("root-migration"));
        assert!(is_link(&folder));
        assert!(same(&folder, &source).unwrap());
        assert!(source.join("alpha/SKILL.md").is_file());
        assert!(!exists(&s.central.join("alpha")));
        assert!(s.stored::<SkillTarget>("skill_targets").unwrap().is_empty());
    }
}
