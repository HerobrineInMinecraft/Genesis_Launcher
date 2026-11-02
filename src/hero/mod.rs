//! # `src/hero/mod.rs`
//!
//! **功能域：** 首页主视觉标题的规范化、字符样式解析和矢量文字绘制。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `DEFAULT_HERO_TITLE`
//! - `normalize_hero_title`
//! - `default_hero_title_char_colors`
//! - `styled_hero_title_lines`
//! - `paint_styled_hero_title_line`
//! - `parse_rgb_color`
//! - `parse_u16_list`
//! - `parse_rgb_list`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{self, Color32, Painter, Pos2};

use crate::theme;

/// 提供 默认HEROTITLE 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) const DEFAULT_HERO_TITLE: &str = "世界，\n等你回来。";

/// 实现 normalizeherotitle 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn normalize_hero_title(value: &str) -> String {
    value
        .replace('\r', "")
        .lines()
        .take(3)
        .map(|line| line.trim().chars().take(16).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 实现 默认herotitlecharcolors 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<[u8`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn default_hero_title_char_colors() -> Vec<[u8; 3]> {
    let mut second_line = false;
    DEFAULT_HERO_TITLE
        .chars()
        .map(|character| {
            if character == '\n' {
                second_line = true;
            }
            if second_line {
                [217, 245, 191]
            } else {
                [245, 250, 238]
            }
        })
        .collect()
}

/// 实现 styledherotitlelines 相关的业务流程。
///
/// # 参数
/// - `sizes`：由调用方传入的sizes。
/// - `colors`：由调用方传入的colors。
/// # 返回
/// 返回 `Vec<Vec<(char, f32, Color32)>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn styled_hero_title_lines(
    title: &str,
    sizes: &[u16],
    colors: &[[u8; 3]],
) -> Vec<Vec<(char, f32, Color32)>> {
    let mut lines = Vec::new();
    let mut current = Vec::new();
    for (index, character) in title.chars().enumerate() {
        if character == '\n' {
            if !current.is_empty() && lines.len() < 3 {
                lines.push(std::mem::take(&mut current));
            }
        } else if !character.is_control() {
            let size = sizes.get(index).copied().unwrap_or(58) as f32;
            let color = colors.get(index).copied().unwrap_or([245, 250, 238]);
            current.push((
                character,
                size,
                Color32::from_rgb(color[0], color[1], color[2]),
            ));
        }
    }
    if !current.is_empty() && lines.len() < 3 {
        lines.push(current);
    }
    lines
}

/// 实现 绘制styledherotitleline 相关的业务流程。
///
/// # 参数
/// - `origin`：由调用方传入的origin。
/// - `available_width`：由调用方传入的可用width。
/// - `size_scale`：由调用方传入的sizescale。
/// - `line`：由调用方传入的line。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn paint_styled_hero_title_line(
    painter: &Painter,
    origin: Pos2,
    available_width: f32,
    size_scale: f32,
    line: &[(char, f32, Color32)],
) -> f32 {
    let natural_width = line
        .iter()
        .map(|(character, size, color)| {
            painter
                .layout_no_wrap(
                    character.to_string(),
                    theme::bold_font(*size * size_scale),
                    *color,
                )
                .size()
                .x
        })
        .sum::<f32>();
    let fit_scale = if natural_width > 0.0 {
        (available_width / natural_width).min(1.0)
    } else {
        1.0
    };
    let final_scale = size_scale * fit_scale;
    let max_size = line
        .iter()
        .map(|(_, size, _)| *size * final_scale)
        .fold(0.0_f32, f32::max);
    let baseline = origin.y + max_size;
    let mut x = origin.x;
    for (character, size, color) in line {
        let scaled_size = *size * final_scale;
        let text = character.to_string();
        let width = painter
            .layout_no_wrap(text.clone(), theme::bold_font(scaled_size), *color)
            .size()
            .x;
        let position = Pos2::new(x, baseline);
        let color = theme::adaptive_text_color_at(painter, position, *color, scaled_size);
        painter.text(
            position,
            egui::Align2::LEFT_BOTTOM,
            text,
            theme::bold_font(scaled_size),
            color,
        );
        x += width;
    }
    max_size * 1.02
}

/// 实现 解析rgbcolor 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<[u8`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn parse_rgb_color(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(value.get(0..2)?, 16).ok()?,
        u8::from_str_radix(value.get(2..4)?, 16).ok()?,
        u8::from_str_radix(value.get(4..6)?, 16).ok()?,
    ])
}

/// 实现 解析u16list 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<u16>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn parse_u16_list(value: &str) -> Vec<u16> {
    let parsed = value
        .split('|')
        .map(|part| part.trim().parse::<u16>().ok())
        .collect::<Option<Vec<_>>>();
    parsed.unwrap_or_default()
}

/// 实现 解析rgblist 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<[u8`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn parse_rgb_list(value: &str) -> Vec<[u8; 3]> {
    let parsed = value
        .split('|')
        .map(parse_rgb_color)
        .collect::<Option<Vec<_>>>();
    parsed.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// 实现 parsesandlimitshomeherotitlesettings 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn parses_and_limits_home_hero_title_settings() {
        assert_eq!(parse_rgb_color("#12ABF4"), Some([0x12, 0xAB, 0xF4]));
        assert_eq!(parse_rgb_color("12abf4"), Some([0x12, 0xAB, 0xF4]));
        assert_eq!(parse_rgb_color("#FFF"), None);
        assert_eq!(
            normalize_hero_title(" 世界， \r\n 等你回来。\n第三行\n第四行"),
            "世界，\n等你回来。\n第三行"
        );
        assert_eq!(parse_u16_list("8|10|12"), vec![8, 10, 12]);
        assert!(parse_u16_list("8|bad|12").is_empty());
        assert_eq!(
            parse_rgb_list("CFDECA|FFFFFF|12ABF4"),
            vec![[0xCF, 0xDE, 0xCA], [0xFF, 0xFF, 0xFF], [0x12, 0xAB, 0xF4]]
        );
    }
}
