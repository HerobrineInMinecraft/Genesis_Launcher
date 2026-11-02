//! animation 动画模块：页面卡片滚动入场/退场动画、弹簧物理动画、透明度缓动曲线
use eframe::egui::{Rect, Vec2};
use std::sync::atomic::{AtomicU16, Ordering};

/// 全局动画强度百分比（0~100），原子变量，支持UI设置实时生效
static ANIMATION_STRENGTH: AtomicU16 = AtomicU16::new(100);

/// 卡片位移动画数据：用于页面切换时卡片从上/下滑入滑出
#[derive(Clone, Copy)]
pub(crate) struct CardMotion {
    /// 当前屏幕视口矩形，用来计算延迟偏移
    pub(crate) screen: Rect,
    /// 运动方向：正数向下，负数向上
    pub(crate) direction: f32,
    /// 全局页面切换进度，范围为 `[0,1]`
    pub(crate) progress: f32,
    /// 是否是退场（离开页面），true=退场，false=入场
    pub(crate) outgoing: bool,
}

impl CardMotion {
    /// 根据动画偏移，把原始矩形平移后返回最终绘制矩形
    pub(crate) fn rect(self, rect: Rect) -> Rect {
        rect.translate(Vec2::new(0.0, self.offset(rect)))
    }

    /// 计算单张卡片的局部进度：带视差延迟，越远的卡片动画越晚启动
    fn local_progress(self, rect: Rect) -> f32 {
        // 根据运动方向，计算卡片中心距离屏幕边缘的归一化距离
        let distance = if self.direction > 0.0 {
            // 向下运动：距离屏幕底边的比例
            (self.screen.bottom() - rect.center().y) / self.screen.height().max(1.0)
        } else {
            // 向上运动：距离屏幕顶边的比例
            (rect.center().y - self.screen.top()) / self.screen.height().max(1.0)
        };
        // 距离映射为延迟系数，最远卡片最多延迟 0.36
        let delay = distance.clamp(0.0, 1.0) * 0.36;
        // 扣除延迟后重映射进度，上限1.12允许轻微过冲
        ((self.progress - delay) / (1.0 - delay)).clamp(0.0, 1.12)
    }

    /// 计算卡片Y轴偏移量（核心滑动动画）
    pub(crate) fn offset(self, rect: Rect) -> f32 {
        let local = self.local_progress(rect);
        if self.outgoing {
            // 退场：沿着方向滑出屏幕
            -self.direction * self.screen.height() * local
        } else {
            // 入场：从屏幕外滑入，进度1时归位
            self.direction * self.screen.height() * (1.0 - local)
        }
    }
}

/// 设置全局动画强度百分比，UI设置项调用
pub(crate) fn configure_animation_strength(percent: u16) {
    ANIMATION_STRENGTH.store(percent, Ordering::Relaxed);
}

/// 获取归一化动画强度系数 [0.0, 1.0]，弹簧动画、缓动函数统一读取
pub(crate) fn animation_strength() -> f32 {
    ANIMATION_STRENGTH.load(Ordering::Relaxed) as f32 / 100.0
}

/// 旧页面退场透明度：页面切换时，上一页快速淡出
pub(crate) fn outgoing_page_opacity(progress: f32) -> f32 {
    ((1.0 - progress.clamp(0.0, 1.0)) / 0.45).clamp(0.0, 1.0)
}

/// 版本卡片文字透明度，别名，和卡片文字淡入曲线一致
pub(crate) fn version_content_text_opacity(progress: f32) -> f32 {
    card_text_opacity(progress)
}

/// 卡片文字淡入缓动：先延迟，再平滑s曲线淡入（smoothstep）
pub(crate) fn card_text_opacity(progress: f32) -> f32 {
    // 0.16前完全透明，0.16~0.7区间淡入
    let fade = ((progress - 0.16) / 0.54).clamp(0.0, 1.0);
    // smoothstep 缓动公式：fade²*(3-2fade)，S型，起止速度平缓
    fade * fade * (3.0 - 2.0 * fade)
}

/// 弹窗弹簧动画（高刚度，回弹POP效果）
/// 返回true：动画还在运行；false：已经静止到位
pub(crate) fn spring_popup(value: &mut f32, velocity: &mut f32, target: f32, delta: f32) -> bool {
    spring(value, velocity, target, delta, 310.0, 21.0, true)
}

/// 通用平滑弹簧（低刚度，柔和跟随，无强制归零吸附）
pub(crate) fn spring_smooth(value: &mut f32, velocity: &mut f32, target: f32, delta: f32) -> bool {
    spring(value, velocity, target, delta, 270.0, 20.0, false)
}

/// 底层弹簧物理积分器（隐式欧拉分步积分）
/// stiffness：刚度，越大拉回越快；damping：阻尼，越大抑制弹跳
/// snap_at_zero：目标为0时，一旦小于等于0直接吸附到0
fn spring(
    value: &mut f32,
    velocity: &mut f32,
    target: f32,
    delta: f32,
    stiffness: f32,
    damping: f32,
    snap_at_zero: bool,
) -> bool {
    let strength = animation_strength();
    // 动画强度0：直接硬切，关闭所有动画
    if strength <= 0.0 {
        *value = target;
        *velocity = 0.0;
        return false;
    }
    // 强度限制区间，防止动画爆炸或者过弱
    let strength = strength.clamp(0.35, 1.7);
    // 子步细分：大delta时分步迭代，稳定物理，防止弹簧发散
    let substeps = (delta / 0.008).ceil().clamp(1.0, 8.0) as usize;
    let step = delta / substeps as f32;
    for _ in 0..substeps {
        // F = k*x - d*v 胡克弹簧力
        let acceleration = (target - *value) * stiffness * strength - *velocity * damping;
        *velocity += acceleration * step;
        *value += *velocity * step;
    }
    // 开启归零吸附，目标是0且数值<=0时强制锁死在0
    if snap_at_zero && target == 0.0 && *value <= 0.0 {
        *value = 0.0;
        *velocity = 0.0;
        return false;
    }
    // 位置+速度足够接近目标，判定停止动画
    if (*value - target).abs() < 0.001 && velocity.abs() < 0.01 {
        *value = target;
        *velocity = 0.0;
        false
    } else {
        true
    }
}

/// 弹窗淡入淡出透明度曲线：打开时正常S缓动；关闭时延后淡出
pub(crate) fn popup_opacity(value: f32, open: bool) -> f32 {
    let fade = if open {
        // 打开：直接使用进度
        value.clamp(0.0, 1.0)
    } else {
        // 关闭：进度到0.1之后才开始淡出，延后消失
        ((value - 0.10) / 0.90).clamp(0.0, 1.0)
    };
    // smoothstep S曲线
    fade * fade * (3.0 - 2.0 * fade)
}
