//! # `src/resources/route.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 测量并记忆网络线路延迟与优先级
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `preferred_modrinth_route`
//! - `remember_modrinth_route`
//! - `fastest_modrinth_route`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::sync::{Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::brand;

use super::mirror::{MCIM_HOST, ModrinthRoute};

/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const USER_AGENT: &str = brand::USER_AGENT;
/// 提供 线路缓存TTL 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const ROUTE_CACHE_TTL: Duration = Duration::from_secs(120);

#[derive(Default)]
/// 定义 RoutePreference 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct RoutePreference {
    route: Option<ModrinthRoute>,
    measured_at: Option<Instant>,
}

/// 实现 线路preference 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `&'static Mutex<RoutePreference>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn route_preference() -> &'static Mutex<RoutePreference> {
    /// 提供 PREFERENCE 使用的进程级静态状态。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    static PREFERENCE: OnceLock<Mutex<RoutePreference>> = OnceLock::new();
    PREFERENCE.get_or_init(|| Mutex::new(RoutePreference::default()))
}

/// 实现 preferredmodrinth线路 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `ModrinthRoute`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn preferred_modrinth_route() -> ModrinthRoute {
    {
        let Ok(preference) = route_preference().lock() else {
            return ModrinthRoute::Official;
        };
        if let (Some(route), Some(measured_at)) = (preference.route, preference.measured_at)
            && measured_at.elapsed() < ROUTE_CACHE_TTL
        {
            return route;
        }
    }
    let route = measure_modrinth_routes();
    remember_modrinth_route(route);
    route
}

/// 实现 remembermodrinth线路 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn remember_modrinth_route(route: ModrinthRoute) {
    if let Ok(mut preference) = route_preference().lock() {
        preference.route = Some(route);
        preference.measured_at = Some(Instant::now());
    }
}

/// 实现 measuremodrinthroutes 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `ModrinthRoute`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn measure_modrinth_routes() -> ModrinthRoute {
    let (sender, receiver) = mpsc::channel();
    for (route, url) in [
        (
            ModrinthRoute::Official,
            "https://api.modrinth.com/v2/search?query=&limit=1".to_owned(),
        ),
        (
            ModrinthRoute::Mirror,
            format!("{MCIM_HOST}/modrinth/v2/search?query=&limit=1"),
        ),
    ] {
        let sender = sender.clone();
        thread::spawn(move || {
            let started = Instant::now();
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(3))
                .timeout_read(Duration::from_secs(5))
                .build();
            let reachable = agent
                .get(&url)
                .set("User-Agent", USER_AGENT)
                .call()
                .ok()
                .and_then(|response| response.into_string().ok())
                .is_some();
            if reachable {
                let _ = sender.send((route, started.elapsed()));
            }
        });
    }
    drop(sender);

    let deadline = Instant::now() + Duration::from_secs(4);
    let mut samples = Vec::new();
    for _ in 0..2 {
        let comparison_deadline = if samples.is_empty() {
            deadline
        } else {
            (Instant::now() + Duration::from_millis(400)).min(deadline)
        };
        let remaining = comparison_deadline.saturating_duration_since(Instant::now());
        let Ok(result) = receiver.recv_timeout(remaining) else {
            break;
        };
        samples.push(result);
    }
    fastest_modrinth_route(&samples)
}

/// 实现 fastestmodrinth线路 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `ModrinthRoute`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn fastest_modrinth_route(samples: &[(ModrinthRoute, Duration)]) -> ModrinthRoute {
    samples
        .iter()
        .min_by_key(|(_, latency)| *latency)
        .map_or(ModrinthRoute::Official, |(route, _)| *route)
}
