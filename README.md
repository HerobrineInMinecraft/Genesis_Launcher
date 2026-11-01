# Genesis Launcher

这是一个使用 Rust 构建的原生 Minecraft 启动器。它不依赖 WebView、HTML、CSS 或 JavaScript，从界面渲染到游戏启动均由桌面程序直接完成。

## 下载

GitHub Releases：

https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases

下载 `Genesis.Launcher.exe` 后直接运行即可。

## 主要功能

- 接入 Mojang Piston Meta 官方下载源，可下载、校验并补全游戏版本、资源文件和 Windows Native 文件。
- 支持启动游戏、查看运行状态，并可强制关闭当前游戏。
- 支持 Fabric、Forge、NeoForge 加载器版本获取与安装。
- 支持多 Minecraft 文件夹、默认版本隔离和单版本内存分配。
- 支持 Modrinth 模组、整合包、光影、资源包和数据包浏览与下载。
- 支持 `.mrpack` 和 Modrinth 整合包导入，并自动补全对应的游戏与加载器环境。
- 内置 3D 皮肤编辑器，支持 Steve/Alex、完整 RGB 调色、模型表面绘制、撤销/重做和 PNG 导入导出；当前仅支持本地编辑与导出。
- 已预留 Fabric、Forge、NeoForge 模组制作界面。
- 收藏页内置 Minecraft 官网、Wiki、Modrinth、CurseForge、MCMod 和 Planet Minecraft，并支持自定义网站和图标。
- 提供液态玻璃、动态界面、鼠标光效和强制自动更新等体验功能。

## 更新方式

启动器会从 GitHub Releases 检查正式版本。检测到更高版本后，会自动下载新程序、校验 SHA-256、替换旧文件并重新启动，不需要手动覆盖。

## 当前说明

1.0.0 使用离线玩家 `Steve`，尚未接入 Microsoft OAuth、Xbox Live 和 Minecraft Services 在线认证，因此暂时不能连接需要正版验证的服务器。

当前版本暂不支持更换账户皮肤。

当前版本暂不支持更改玩家名称。

模组制作暂不可用，相关功能将在后续版本开放。

当前版本面向 Windows 10/11 64 位系统。下载和安装 Minecraft 时需要网络连接，启动游戏则需要对应版本的 Java。

## 问题反馈

如果遇到启动失败、下载异常、界面显示或版本兼容问题，请在 GitHub Issues 中提供：

- Genesis Launcher 版本
- Minecraft 版本和加载器版本
- 问题截图
- `launcher-java.log` 或更新错误日志

感谢试用 Genesis Launcher。
