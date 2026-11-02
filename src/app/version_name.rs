//! 版本真实名称编辑。
//!
//! 重命名会同步修改 `versions/<名称>` 目录、`<名称>.json`、JSON `id`、客户端 JAR
//! 引用及其他版本的 `inheritsFrom`。窗口从“重命名”按钮展开，关闭时弹性缩回原位。

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;

/// 版本重命名对话框锁定的游戏目录与原始 profile ID。
#[derive(Clone)]
pub(super) struct VersionNameTarget {
    directory: PathBuf,
    profile: String,
}

impl LauncherApp {
    /// 返回当前版本在启动器中显示的名称。
    pub(super) fn version_display_name(&self, version: &GameVersion) -> String {
        self.version_custom_name(version)
            .unwrap_or_else(|| version.game_version.clone())
    }

    /// 返回版本自定义名称；版本 JSON 标记优先于旧 settings.ini 显示别名。
    pub(super) fn version_custom_name(&self, version: &GameVersion) -> Option<String> {
        version
            .custom_name
            .clone()
            .or_else(|| self.version_alias(version))
    }

    /// 查询当前游戏目录下某个版本遗留的显示别名。
    pub(super) fn version_alias(&self, version: &GameVersion) -> Option<String> {
        self.settings.game_directory.as_ref().and_then(|directory| {
            self.settings
                .version_name(directory, &version.version)
                .filter(|name| !name.trim().is_empty())
        })
    }

    /// 打开当前选中版本的重命名对话框。
    ///
    /// `source` 是“重命名”按钮矩形，作为打开与关闭动画的起止位置。
    pub(super) fn open_version_name_dialog(&mut self, source: Rect) {
        let Some(directory) = self.settings.game_directory.clone() else {
            self.show_toast("请先选择 Minecraft 游戏目录");
            return;
        };
        let version = self.version().clone();
        self.version_name_input = suggested_profile_name(&directory, &version.version);
        self.version_name_target = Some(VersionNameTarget {
            directory,
            profile: version.version.clone(),
        });
        self.version_name_dialog_open = true;
        self.version_name_dialog_value = 0.0;
        self.version_name_dialog_velocity = 0.0;
        self.version_name_dialog_source = source;
        self.version_name_error = None;
    }

    /// 推进重命名窗口的弹簧动画。
    pub(super) fn update_version_name_animation(&mut self, ctx: &egui::Context) {
        if self.version_name_target.is_none() {
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let target = if self.version_name_dialog_open {
            1.0
        } else {
            0.0
        };
        let active = spring_popup(
            &mut self.version_name_dialog_value,
            &mut self.version_name_dialog_velocity,
            target,
            delta,
        );
        if active {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if !self.version_name_dialog_open {
            self.version_name_target = None;
            self.version_name_error = None;
            self.version_name_dialog_source = Rect::NOTHING;
        }
    }

    /// 执行真实版本重命名并关闭窗口。
    pub(super) fn save_version_name(&mut self) {
        let Some(target) = self.version_name_target.clone() else {
            return;
        };
        let name = self.version_name_input.trim().to_owned();
        if let Err(error) = validate_version_name(&name) {
            self.version_name_error = Some(error);
            return;
        }
        if name == target.profile {
            self.version_name_dialog_open = false;
            return;
        }
        if self.install_receiver.is_some()
            || self.loader_install_receiver.is_some()
            || self.modpack_import_receiver.is_some()
        {
            self.version_name_error = Some("请等待当前安装或导入任务完成后再重命名".to_owned());
            return;
        }
        match rename_version_profile(&target.directory, &target.profile, &name) {
            Ok(()) => {
                self.settings
                    .rename_version_references(&target.directory, &target.profile, &name);
                if self
                    .resource_target_profile
                    .as_deref()
                    .is_some_and(|profile| profile == target.profile)
                {
                    self.resource_target_profile = Some(name.clone());
                }
                if self.install_version_id.as_deref() == Some(target.profile.as_str()) {
                    self.install_version_id = Some(name.clone());
                }
                if self.loader_base_version == target.profile {
                    self.loader_base_version = name.clone();
                }
                self.versions = available_versions(Some(&target.directory));
                self.selected_version = self
                    .versions
                    .iter()
                    .position(|version| version.version == name)
                    .unwrap_or(self.selected_version);
                self.version_page_selected = self.selected_version;
                self.refresh_recent_sessions();
                if let Some(target) = self.version_name_target.as_mut() {
                    target.profile = name.clone();
                }
                self.version_name_input = name.clone();
                self.version_name_error = None;
                self.version_name_dialog_open = false;
                self.show_toast(format!("版本已重命名为 {name}"));
            }
            Err(error) => self.version_name_error = Some(error),
        }
    }

    /// 绘制版本重命名模态对话框。
    pub(super) fn paint_version_name_dialog(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
    ) {
        if self.version_name_target.is_none() && self.version_name_dialog_value <= 0.002 {
            return;
        }
        let Some(target) = self.version_name_target.clone() else {
            return;
        };
        let source = if self.version_name_dialog_source.width() > 0.0 {
            self.version_name_dialog_source
        } else {
            Rect::from_center_size(screen.center(), Vec2::splat(1.0))
        };
        let destination = Rect::from_center_size(screen.center(), Vec2::new(470.0, 250.0));
        let progress = self.version_name_dialog_value.clamp(0.0, 1.08);
        let eased = progress.clamp(0.0, 1.0);
        let eased = eased * eased * (3.0 - 2.0 * eased);
        let card = Rect::from_min_max(
            source.min + (destination.min - source.min) * eased,
            source.max + (destination.max - source.max) * eased,
        );
        let opacity = popup_opacity(
            self.version_name_dialog_value,
            self.version_name_dialog_open,
        );
        let mut input = self.version_name_input.clone();
        let mut save_requested = false;
        let mut cancel_requested = false;
        egui::Modal::new(Id::unique("version-name-dialog-modal"))
            .frame(egui::Frame::NONE)
            .backdrop_color(Color32::TRANSPARENT)
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    screen,
                    0,
                    Color32::from_black_alpha((128.0 * eased) as u8),
                );
                let _ = ui.allocate_exact_size(destination.size(), Sense::click_and_drag());
                let mut painter = ui.painter().clone();
                painter.set_opacity(opacity);
                theme::glass_panel(
                    &painter,
                    card,
                    theme::corner_radius(),
                    theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 43.0)
                        .backdrop_blur(18.0),
                );
                let content = card_clip(&painter, card);
                theme::bold_text(
                    &content,
                    Pos2::new(card.left() + 22.0, card.top() + 18.0),
                    "重命名版本",
                    18.0,
                    theme::ink(),
                );
                theme::text(
                    &content,
                    Pos2::new(card.left() + 22.0, card.top() + 47.0),
                    format!(
                        "将重命名 versions/{} 及其 JSON · 支持中文",
                        fit_text_to_width(
                            &content,
                            &target.profile,
                            theme::font(8.0),
                            card.width() - 210.0,
                        )
                    ),
                    8.0,
                    theme::muted(),
                );
                let field = Rect::from_min_size(
                    Pos2::new(card.left() + 22.0, card.top() + 78.0),
                    Vec2::new(card.width() - 44.0, 38.0),
                );
                theme::fill_round(
                    &content,
                    field,
                    theme::corner_radius(),
                    Color32::from_rgb(24, 38, 28),
                );
                theme::stroke_round(
                    &content,
                    field,
                    theme::corner_radius(),
                    1.0,
                    theme::white_alpha(30),
                );
                if self.version_name_dialog_open {
                    ui.put(
                        field.shrink2(Vec2::new(11.0, 6.0)),
                        egui::TextEdit::singleline(&mut input)
                            .frame(egui::Frame::NONE)
                            .font(theme::font(10.0))
                            .text_color(theme::adaptive_text_color_at(
                                &content,
                                field.center(),
                                theme::ink(),
                                10.0,
                            ))
                            .hint_text("输入新的版本名"),
                    );
                } else {
                    theme::text(
                        &content,
                        Pos2::new(field.left() + 11.0, field.top() + 11.0),
                        &input,
                        10.0,
                        theme::ink(),
                    );
                }
                if let Some(error) = self.version_name_error.as_ref() {
                    theme::text(
                        &content,
                        Pos2::new(card.left() + 22.0, card.top() + 124.0),
                        fit_text_to_width(&content, error, theme::font(8.0), card.width() - 44.0),
                        8.0,
                        Color32::from_rgb(226, 172, 91),
                    );
                } else {
                    theme::text(
                        &content,
                        Pos2::new(card.left() + 22.0, card.top() + 124.0),
                        "不能包含 \\ / : * ? \" < > |，且不能使用 Windows 保留名称",
                        8.0,
                        theme::muted(),
                    );
                }

                let cancel = Rect::from_min_size(
                    Pos2::new(card.right() - 174.0, card.bottom() - 52.0),
                    Vec2::new(68.0, 34.0),
                );
                let save = Rect::from_min_size(
                    Pos2::new(card.right() - 98.0, card.bottom() - 52.0),
                    Vec2::new(76.0, 34.0),
                );
                if self.version_name_dialog_open {
                    cancel_requested =
                        settings_button(ui, &content, cancel, "version-name-cancel", "取消", false);
                    save_requested =
                        settings_button(ui, &content, save, "version-name-save", "重命名", true);
                }
            });
        self.version_name_input = input;
        if cancel_requested || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.version_name_dialog_open = false;
            self.version_name_error = None;
            return;
        }
        if self.version_name_dialog_open
            && (save_requested || ctx.input(|input| input.key_pressed(egui::Key::Enter)))
        {
            self.save_version_name();
        }
    }
}

/// 为旧实例生成不带启动器品牌、且不与现有目录冲突的默认重命名建议。
fn suggested_profile_name(directory: &Path, profile: &str) -> String {
    let cleaned = crate::brand::strip_generated_name_prefix(profile);
    if cleaned == profile {
        return cleaned;
    }
    let versions = directory.join("versions");
    if !versions.join(&cleaned).exists() {
        return cleaned;
    }
    for index in 2..1000 {
        let candidate = format!("{cleaned} ({index})");
        if !versions.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{cleaned}-{}", std::process::id())
}

/// 校验用户输入的版本名称。
fn validate_version_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("版本名不能为空".to_owned());
    }
    if name.chars().count() > 64 || name.len() > 200 {
        return Err("版本名过长".to_owned());
    }
    if name == "." || name == ".." || name.ends_with([' ', '.']) {
        return Err("版本名不能以空格或句点结尾".to_owned());
    }
    if name.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
    }) {
        return Err("版本名包含 Windows 不允许的字符".to_owned());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        });
    if reserved {
        return Err("版本名不能使用 Windows 保留名称".to_owned());
    }
    Ok(())
}

/// 事务式重命名版本目录、JSON 和所有引用。
fn rename_version_profile(
    game_directory: &Path,
    old_profile: &str,
    new_profile: &str,
) -> Result<(), String> {
    validate_version_name(new_profile)?;
    let versions = game_directory.join("versions");
    let old_directory = versions.join(old_profile);
    let new_directory = versions.join(new_profile);
    if !old_directory.is_dir() {
        return Err(format!("版本目录不存在：{}", old_directory.display()));
    }
    if new_directory.exists() {
        return Err(format!("版本名“{new_profile}”已经存在"));
    }
    let old_json_path = old_directory.join(format!("{old_profile}.json"));
    let original_json = std::fs::read_to_string(&old_json_path)
        .map_err(|error| format!("无法读取版本 JSON：{error}"))?;
    let mut value: Value = serde_json::from_str(&original_json)
        .map_err(|error| format!("版本 JSON 格式无效：{error}"))?;
    value["id"] = Value::String(new_profile.to_owned());
    value["genesisDisplayName"] = Value::String(new_profile.to_owned());
    if value.get("jar").and_then(Value::as_str) == Some(old_profile) {
        value["jar"] = Value::String(new_profile.to_owned());
    }
    let renamed_json = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("无法生成新版本 JSON：{error}"))?;

    let mut referenced: Vec<(PathBuf, String, String)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&versions) {
        for entry in entries.flatten() {
            let directory = entry.path();
            if !directory.is_dir() || directory == old_directory {
                continue;
            }
            let Some(profile) = directory.file_name().map(|name| name.to_string_lossy()) else {
                continue;
            };
            let path = directory.join(format!("{profile}.json"));
            let Ok(original) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut value) = serde_json::from_str::<Value>(&original) else {
                continue;
            };
            let mut changed = false;
            if value.get("inheritsFrom").and_then(Value::as_str) == Some(old_profile) {
                value["inheritsFrom"] = Value::String(new_profile.to_owned());
                changed = true;
            }
            if value.get("jar").and_then(Value::as_str) == Some(old_profile) {
                value["jar"] = Value::String(new_profile.to_owned());
                changed = true;
            }
            if changed {
                let updated = serde_json::to_string_pretty(&value)
                    .map_err(|error| format!("无法生成 {}：{error}", path.display()))?;
                referenced.push((path, original, updated));
            }
        }
    }
    let launcher_profiles = game_directory.join("launcher_profiles.json");
    if let Ok(original) = std::fs::read_to_string(&launcher_profiles)
        && let Ok(mut value) = serde_json::from_str::<Value>(&original)
        && let Some(profiles) = value.get_mut("profiles").and_then(Value::as_object_mut)
    {
        let mut changed = false;
        for profile in profiles.values_mut() {
            if profile.get("lastVersionId").and_then(Value::as_str) == Some(old_profile) {
                profile["lastVersionId"] = Value::String(new_profile.to_owned());
                changed = true;
            }
        }
        if changed && let Ok(updated) = serde_json::to_string_pretty(&value) {
            referenced.push((launcher_profiles, original, updated));
        }
    }

    std::fs::rename(&old_directory, &new_directory)
        .map_err(|error| format!("无法重命名版本目录：{error}"))?;
    let moved_old_json = new_directory.join(format!("{old_profile}.json"));
    let moved_new_json = new_directory.join(format!("{new_profile}.json"));
    if let Err(error) = std::fs::rename(&moved_old_json, &moved_new_json) {
        let _ = std::fs::rename(&new_directory, &old_directory);
        return Err(format!("无法重命名版本 JSON：{error}"));
    }
    let old_jar = new_directory.join(format!("{old_profile}.jar"));
    let new_jar = new_directory.join(format!("{new_profile}.jar"));
    let jar_renamed = if old_jar.is_file() {
        if let Err(error) = std::fs::rename(&old_jar, &new_jar) {
            let _ = std::fs::rename(&moved_new_json, &moved_old_json);
            let _ = std::fs::rename(&new_directory, &old_directory);
            return Err(format!("无法重命名客户端 JAR：{error}"));
        }
        true
    } else {
        false
    };
    if let Err(error) = std::fs::write(&moved_new_json, renamed_json) {
        rollback_rename(
            &new_directory,
            &old_directory,
            &moved_new_json,
            &moved_old_json,
            &new_jar,
            &old_jar,
            jar_renamed,
            &original_json,
            &referenced,
        );
        return Err(format!("无法写入新版本 JSON：{error}"));
    }
    if let Err(error) = referenced
        .iter()
        .try_for_each(|(path, _, updated)| std::fs::write(path, updated))
    {
        rollback_rename(
            &new_directory,
            &old_directory,
            &moved_new_json,
            &moved_old_json,
            &new_jar,
            &old_jar,
            jar_renamed,
            &original_json,
            &referenced,
        );
        return Err(format!("无法更新版本引用：{error}"));
    }
    Ok(())
}

/// 在重命名中途失败时恢复目录、JSON、JAR 和所有引用文件。
#[allow(clippy::too_many_arguments)]
fn rollback_rename(
    new_directory: &Path,
    old_directory: &Path,
    moved_new_json: &Path,
    moved_old_json: &Path,
    new_jar: &Path,
    old_jar: &Path,
    jar_renamed: bool,
    original_json: &str,
    referenced: &[(PathBuf, String, String)],
) {
    for (path, original, _) in referenced {
        let _ = std::fs::write(path, original);
    }
    let _ = std::fs::remove_file(moved_new_json);
    if jar_renamed {
        let _ = std::fs::rename(new_jar, old_jar);
    }
    let _ = std::fs::write(moved_old_json, original_json);
    let _ = std::fs::rename(new_directory, old_directory);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// 验证中文真实版本名会同步迁移目录、JSON、JAR、依赖和启动配置。
    fn renames_real_profile_with_unicode_and_updates_references() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-version-rename-{}",
            std::process::id()
        ));
        let versions = root.join("versions");
        let old = versions.join("旧版本");
        let dependency = versions.join("fabric-1.21.1");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&dependency).unwrap();
        std::fs::write(
            old.join("旧版本.json"),
            r#"{"id":"旧版本","jar":"旧版本","type":"release"}"#,
        )
        .unwrap();
        std::fs::write(old.join("旧版本.jar"), b"client").unwrap();
        std::fs::write(
            dependency.join("fabric-1.21.1.json"),
            r#"{"id":"fabric-1.21.1","inheritsFrom":"旧版本"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("launcher_profiles.json"),
            r#"{"profiles":{"default":{"lastVersionId":"旧版本"}}}"#,
        )
        .unwrap();

        rename_version_profile(&root, "旧版本", "愚者 生存").unwrap();

        let renamed = versions.join("愚者 生存");
        assert!(renamed.is_dir());
        assert!(renamed.join("愚者 生存.json").is_file());
        assert!(renamed.join("愚者 生存.jar").is_file());
        assert!(!old.exists());
        let profile: Value =
            serde_json::from_str(&std::fs::read_to_string(renamed.join("愚者 生存.json")).unwrap())
                .unwrap();
        assert_eq!(profile["id"], "愚者 生存");
        assert_eq!(profile["jar"], "愚者 生存");
        assert_eq!(profile["genesisDisplayName"], "愚者 生存");
        let dependency: Value = serde_json::from_str(
            &std::fs::read_to_string(dependency.join("fabric-1.21.1.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(dependency["inheritsFrom"], "愚者 生存");
        let launcher: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("launcher_profiles.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            launcher["profiles"]["default"]["lastVersionId"],
            "愚者 生存"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    /// 中文名称合法，同时拒绝 Windows 禁止字符和保留设备名。
    fn validates_unicode_and_windows_safe_names() {
        assert!(validate_version_name("愚者生存版").is_ok());
        assert!(validate_version_name("A B C").is_ok());
        assert!(validate_version_name("CON").is_err());
        assert!(validate_version_name("bad/name").is_err());
        assert!(validate_version_name("trailing.").is_err());
    }

    #[test]
    /// 旧实例重命名时默认去掉启动器前缀，并避开已存在的原版版本目录。
    fn suggests_clean_non_conflicting_rename_for_legacy_profile() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-version-suggestion-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("versions").join("1.21.1")).unwrap();
        assert_eq!(
            suggested_profile_name(&root, "Genesis Launcher · 1.21.1"),
            "1.21.1 (2)"
        );
        assert_eq!(suggested_profile_name(&root, "愚者生存"), "愚者生存");
        let _ = std::fs::remove_dir_all(root);
    }
}
