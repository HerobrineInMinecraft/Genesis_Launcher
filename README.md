# Genesis Launcher

方块启动器（Genesis Launcher）

一个使用 Rust 和 egui 构建的原生 Minecraft 风格桌面启动器。程序直接创建 Win32 窗口，不使用 WebView、HTML、CSS、JavaScript 或本地网页服务。

当前主界面包含：

- 原生 GPU 绘制的方块世界场景
- iOS 风格液态玻璃：GPU 背景折射与模糊、透镜边缘、液态焦散高光和环境光
- 跟随鼠标位置的双层柔光与高光，可被液态玻璃折射
- 外观设置可实时调整玻璃透明度、鼠标光亮度、范围和刷新率
- 648 DPI 固定矢量设计基准，保持原 1440×900 界面尺寸
- 玩家账户与版本切换菜单
- 右上角版本选择按最近游玩时间排序，并读取官方启动器的游玩记录
- 自动发现 `.minecraft` 目录与可启动版本
- 主视觉卡片直接显示最近存档的 `icon.png` 世界截图或服务器图标，并支持快速继续
- 读取版本清单、解压 natives 并调用本机 Java 启动游戏
- 接入 Mojang Piston Meta 与官方资源 CDN，启动时自动补全缺失文件
- 浏览下载正式版、快照及历史版本，校验 SHA1、ETag 与 `.part` 临时文件
- 自动复用官方启动器 Java 运行时；缺失时读取 Mojang Java Runtime Manifest
- 显示文件补全阶段、进度、速度和剩余文件数
- 游戏运行期间显示运行状态，再次点击可强制关闭
- 可配置游戏窗口尺寸、全屏模式、额外 JVM 参数和启动后行为
- Mojang 官方 Minecraft 最新动态、封面、文章详情和官网跳转
- 启动状态反馈、快捷键与窗口控制
- 自适应桌面布局

## 运行

```powershell
cargo run --release
```

也可以直接运行构建后的 `target\release\Genesis Launcher.exe`。

正式文件的版本安装与启动前补全使用 Mojang Piston Meta、资源 CDN 和 Java Runtime Manifest。Java 运行时原始文件由 Mojang 的 `piston-data.mojang.com` 提供。所有下载地址都经过官方域名白名单检查和 SHA1 校验。

当前使用离线玩家档案 `Steve` 启动游戏，可以进入单人游戏。连接需要正版验证的服务器仍需另行接入 Microsoft OAuth、Xbox Live/XSTS 和 Minecraft Services 登录。官方版本安装完成后可选择 Fabric、Forge 或 NeoForge，并按所选加载器版本执行实际安装。
