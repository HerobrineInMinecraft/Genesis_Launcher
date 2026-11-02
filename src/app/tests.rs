//! 启动器 app 模块的单元测试集合。
//!
//! 本文件集中验证 `app` 模块（以及被它间接引用的各子系统）中的纯逻辑：
//! - 版本菜单排序（最近游玩优先）
//! - 收藏站点的内置图标资源、旧版/新版解析
//! - 离线玩家名规范化校验
//! - 主页 Hero 标题的 RGB 颜色/多行/逐字样式解析
//! - 皮肤解码、比例校验、Steve/Alex 模型比例与 UV 排布
//! - 本地皮肤资源包安装、3D 头部预览、命中检测、历史记录
//! - 动画弹簧平滑（tab 过渡、弹窗）、外观模式迁移、动画等级
//! - 资源分类索引、版本隔离、下载弹窗几何、时间戳解析
//! - 模组工程脚手架（Fabric/Forge/NeoForge）与 Modrinth 导出
//!
//! 所有测试均为 `#[test]` 私有函数，仅在 `cargo test` 时编译运行，
//! 不参与启动器正式构建。通过 `super::*` 引入被测项，
//! 并借助 `crate::exporter`、`crate::skin`、`crate::favorites` 等跨模块辅助。

use super::*;
use crate::exporter::{HostedPackFile, collect_modpack_files, export_pack_with_hosted_files};
use crate::skin::{DEFAULT_PLAYER_SKIN_NAMES, LOCAL_SKIN_PACK_NAME, decode_skin_image};

// 测试辅助函数：构造一个"原版"的 GameVersion 用于测试。
// 参数 id 同时作为内部版本号与对外游戏版本号；加载器家族标记为"原版"，
// Java 版本写死为 "Java 21"，初始为可启动状态，描述为空。
/// 实现 版本 相关的业务流程。
///
/// # 参数
/// - `id`：由调用方传入的id。
/// # 返回
/// 返回 `GameVersion`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version(id: &str) -> GameVersion {
    GameVersion {
        version: id.to_owned(),
        game_version: id.to_owned(),
        custom_name: None,
        loader: "原版".to_owned(),
        loader_family: "原版".to_owned(),
        java: "Java 21".to_owned(),
        description: String::new(),
        launchable: true,
    }
}

#[test]
// 验证版本菜单排序会把"最近游玩过"的版本排到最前面。
// recency 映射中 1.21.11(300) > 1.21.8(100)，1.21.10 未记录排最后，
// 期望排序结果为索引序列 [1, 0, 2]。
/// 实现 版本菜单ordersrecentlyplayedfirst 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_menu_orders_recently_played_first() {
    let versions = vec![version("1.21.8"), version("1.21.11"), version("1.21.10")];
    let recency = HashMap::from([("1.21.8".to_owned(), 100), ("1.21.11".to_owned(), 300)]);
    assert_eq!(version_menu_order(&versions, &recency), vec![1, 0, 2]);
}

#[test]
// 验证内置收藏站点的图标资源数量与 BUILTIN_SITES 一致，
// 且每个图标都是可解码、尺寸不小于 32x32 的有效图片，纹理名非空。
/// 实现 builtinfavoriteshavesixvalid内嵌siteicons 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn builtin_favorites_have_six_valid_embedded_site_icons() {
    let icons = builtin_favorite_icon_assets();
    assert_eq!(icons.len(), crate::favorites::BUILTIN_SITES.len());
    for (index, (url, bytes, texture_name)) in icons.into_iter().enumerate() {
        assert_eq!(url, crate::favorites::BUILTIN_SITES[index].1);
        assert!(!texture_name.is_empty());
        let image = image::load_from_memory(bytes).unwrap();
        assert!(image.width() >= 32 && image.height() >= 32);
    }
}

#[test]
// 验证收藏站点字符串的解析：旧版格式（仅标题|URL，无图标来源）能解析，
// 新版格式（标题|URL|本地图标路径）能正确取出 icon_source。
// 分隔符使用 Unicode 单元分隔符 \u{1f}。
/// 实现 favoritesitesettingskeep图标sourcesand读取legacyentries 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn favorite_site_settings_keep_icon_sources_and_read_legacy_entries() {
    let legacy = parse_favorite_site("Example\u{1f}https://example.com").unwrap();
    assert_eq!(legacy.title, "Example");
    assert_eq!(legacy.url, "https://example.com");
    assert!(legacy.icon_source.is_empty());

    let configured =
        parse_favorite_site("Example\u{1f}https://example.com\u{1f}C:\\icons\\example.png")
            .unwrap();
    assert_eq!(configured.icon_source, "C:\\icons\\example.png");
}

#[test]
// 验证离线玩家名规范化：前后空白被裁掉；过短(2 字符)、含非 ASCII、
// 含连字符、过长(>16) 均被拒绝；合法名称通过。
/// 实现 validatesoffline玩家names 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn validates_offline_player_names() {
    assert_eq!(normalize_player_name(" Steve_01 ").unwrap(), "Steve_01");
    assert!(normalize_player_name("ab").is_err());
    assert!(normalize_player_name("玩家名称").is_err());
    assert!(normalize_player_name("name-with-dash").is_err());
    assert!(normalize_player_name("this_name_is_too_long").is_err());
}

#[test]
// 验证主页 Hero 标题相关设置解析：
// - parse_rgb_color 支持带/不带 # 的 6 位十六进制，3 位格式返回 None
// - normalize_hero_title 折叠空白与多余换行，最多保留 3 行
// - parse_u16_list 解析竖线分隔整数，含非法项则返回空
// - parse_rgb_list 解析竖线分隔的多个颜色
// - sync_hero_title_char_styles 按标题字符数展开逐字尺寸/颜色
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
    let mut settings = LauncherSettings {
        hero_title: "AB\nC".to_owned(),
        hero_title_color: [1, 2, 3],
        hero_title_size: 24,
        hero_title_char_sizes: Vec::new(),
        hero_title_char_colors: Vec::new(),
        ..LauncherSettings::default()
    };
    settings.sync_hero_title_char_styles();
    assert_eq!(settings.hero_title_char_sizes, vec![24; 4]);
    assert_eq!(settings.hero_title_char_colors, vec![[1, 2, 3]; 4]);
}

#[test]
// 验证旧版 64x32 皮肤解码：输出为 64x64x4 的像素；
// 解码会做左右镜像补全（右侧手臂/腿由左侧复制），
// 这里在 (40,16) 写入的颜色应出现在镜像位置的像素中。
/// 实现 decodeslegacy皮肤andfillsmirroredlimbs 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn decodes_legacy_skin_and_fills_mirrored_limbs() {
    let mut legacy = image::RgbaImage::from_pixel(64, 32, image::Rgba([40, 80, 120, 255]));
    legacy.put_pixel(40, 16, image::Rgba([210, 30, 70, 255]));
    let pixels =
        decode_skin_image(image::DynamicImage::ImageRgba8(legacy)).expect("valid legacy skin");
    assert_eq!(pixels.len(), 64 * 64 * 4);
    let mirrored = ((48 * 64 + 47) * 4) as usize;
    assert_eq!(&pixels[mirrored..mirrored + 4], &[210, 30, 70, 255]);
}

#[test]
// 验证高清皮肤解码（128x128 合法）；以及非法宽高比（128x32）被拒绝。
/// 实现 decodes皮肤andrejectsinvalidaspectratio 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn decodes_hd_skin_and_rejects_invalid_aspect_ratio() {
    let hd = image::RgbaImage::from_pixel(128, 128, image::Rgba([20, 30, 40, 255]));
    let pixels = decode_skin_image(image::DynamicImage::ImageRgba8(hd)).unwrap();
    assert_eq!(pixels.len(), 64 * 64 * 4);

    let invalid = image::RgbaImage::from_pixel(128, 32, image::Rgba([20, 30, 40, 255]));
    assert!(decode_skin_image(image::DynamicImage::ImageRgba8(invalid)).is_err());
}

#[test]
// 验证通过"未使用的手臂列"检测 Alex(细臂) 皮肤：
// 默认像素判定为 Steve(非 slim)；把右臂第 54/55 列的 alpha 置 0 后判定为 slim。
/// 实现 detectsslim皮肤从unusedarmcolumns 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn detects_slim_skin_from_unused_arm_columns() {
    let mut pixels = default_skin_pixels();
    assert!(!detect_slim_skin(&pixels));
    for x in [54, 55] {
        let offset = (20 * 64 + x) * 4;
        pixels[offset + 3] = 0;
    }
    assert!(detect_slim_skin(&pixels));
}

#[test]
// 验证安装本地皮肤资源包：生成 zip 包并写入 options.txt 的 resourcePacks 列表，
// 既包含新建的 LOCAL_SKIN_PACK_NAME，又保留原有项；
// 包内含 wide/slim 两种模型、所有默认皮肤名对应的 PNG；
// pack.mcmeta 声明 min_format=[1,0]、max_format=999。
/// 实现 installslocal皮肤资源包andenables 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn installs_local_skin_resource_pack_and_enables_it() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-skin-pack-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("options.txt"),
        "resourcePacks:[\"vanilla\",\"file/Another.zip\"]\n",
    )
    .unwrap();
    let pack = install_local_skin_resource_pack(&root, &default_skin_pixels()).unwrap();
    assert!(pack.is_file());
    let options = std::fs::read_to_string(root.join("options.txt")).unwrap();
    assert!(options.contains(LOCAL_SKIN_PACK_NAME));
    assert!(options.contains("file/Another.zip"));
    let file = std::fs::File::open(&pack).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    for model in ["wide", "slim"] {
        for name in DEFAULT_PLAYER_SKIN_NAMES {
            let path = format!("assets/minecraft/textures/entity/player/{model}/{name}.png");
            assert!(
                archive.file_names().any(|entry| entry == path),
                "missing skin texture {path}"
            );
        }
    }
    use std::io::Read as _;
    let mut metadata = String::new();
    archive
        .by_name("pack.mcmeta")
        .unwrap()
        .read_to_string(&mut metadata)
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["pack"]["min_format"], serde_json::json!([1, 0]));
    assert_eq!(metadata["pack"]["max_format"], serde_json::json!(999));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
// 验证 3D 皮肤模型的身体比例符合 Minecraft：
// 头 8x8x8；身体 8x12x4；右臂(Steve) 4x12x4；右腿 4x12x4；
// 各部位在 y 轴上的衔接关系（头底=身体顶 等）。
// Alex 左臂宽度为 3.0（细臂）。
/// 实现 皮肤模型usesMinecraft主体proportions 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn skin_model_uses_minecraft_body_proportions() {
    let steve = skin_cuboids(false);
    let head = steve
        .iter()
        .find(|part| part.part == SkinPart::Head)
        .unwrap();
    let body = steve
        .iter()
        .find(|part| part.part == SkinPart::Body)
        .unwrap();
    let right_arm = steve
        .iter()
        .find(|part| part.part == SkinPart::RightArm)
        .unwrap();
    let right_leg = steve
        .iter()
        .find(|part| part.part == SkinPart::RightLeg)
        .unwrap();
    assert_eq!(
        (head.size().x, head.size().y, head.size().z),
        (8.0, 8.0, 8.0)
    );
    assert_eq!(
        (body.size().x, body.size().y, body.size().z),
        (8.0, 12.0, 4.0)
    );
    assert_eq!(
        (right_arm.size().x, right_arm.size().y, right_arm.size().z),
        (4.0, 12.0, 4.0)
    );
    assert_eq!(
        (right_leg.size().x, right_leg.size().y, right_leg.size().z),
        (4.0, 12.0, 4.0)
    );
    assert_eq!(head.min.y, body.max.y);
    assert_eq!(body.min.y, right_leg.max.y);
    assert_eq!(right_arm.min.y, body.min.y);
    assert_eq!(right_arm.max.y, body.max.y);

    let alex = skin_cuboids(true);
    let alex_arm = alex
        .iter()
        .find(|part| part.part == SkinPart::LeftArm)
        .unwrap();
    assert_eq!(alex_arm.size().x, 3.0);
}

#[test]
// 验证皮肤所有面（wide 与 slim 两种模型）的 UV 坐标都落在 [0,1] 内且宽高为正；
// 并验证身体正面(face==0) UV 区域对应贴图中的 (20..28, 20..32) 区块。
/// 实现 皮肤uvsstayinsidethetextureandfrontfacesmatchthe布局 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn skin_uvs_stay_inside_the_texture_and_front_faces_match_the_layout() {
    for cuboid in skin_cuboids(false).into_iter().chain(skin_cuboids(true)) {
        for face in skin_cuboid_faces(cuboid) {
            assert!(face.uv.left() >= 0.0);
            assert!(face.uv.top() >= 0.0);
            assert!(face.uv.right() <= 1.0);
            assert!(face.uv.bottom() <= 1.0);
            assert!(face.uv.width() > 0.0);
            assert!(face.uv.height() > 0.0);
        }
    }
    let body = skin_cuboids(false)
        .into_iter()
        .find(|part| part.part == SkinPart::Body)
        .unwrap();
    let front = skin_cuboid_faces(body)
        .into_iter()
        .find(|face| face.face == 0)
        .unwrap();
    assert!((front.uv.left() - 20.0 / 64.0).abs() < 0.0001);
    assert!((front.uv.top() - 20.0 / 64.0).abs() < 0.0001);
    assert!((front.uv.right() - 28.0 / 64.0).abs() < 0.0001);
    assert!((front.uv.bottom() - 32.0 / 64.0).abs() < 0.0001);

    // 躯干不是正方体：宽 8、高 12、深 4。两个侧面与背面必须分别位于
    // x=16、x=28、x=32 的图集区域，不能因为把 width 当成 depth 而互相重叠。
    let body_faces = skin_cuboid_faces(body);
    assert!((body_faces[2].uv.left() - 16.0 / 64.0).abs() < 0.0001);
    assert!((body_faces[3].uv.left() - 28.0 / 64.0).abs() < 0.0001);
    assert!((body_faces[1].uv.left() - 32.0 / 64.0).abs() < 0.0001);
    assert!((body_faces[1].uv.right() - 40.0 / 64.0).abs() < 0.0001);
}

#[test]
// 验证皮肤 3D 投影：把模型点投影到屏幕矩形后坐标有限且角色保持竖直（顶部 y < 底部 y）；
// 预览面列表非空、所有点与深度有限，并按深度由近到远排序（画家算法）。
/// 实现 皮肤projectionkeepsthecharacteruprightandfinite 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn skin_projection_keeps_the_character_upright_and_finite() {
    let stage = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(320.0, 620.0));
    let top = project_skin_point(SkinPoint3::new(0.0, 32.0, 0.0), stage, 15.0, -0.42, -0.12);
    let bottom = project_skin_point(SkinPoint3::new(0.0, 0.0, 0.0), stage, 15.0, -0.42, -0.12);
    assert!(top.0.x.is_finite() && top.0.y.is_finite() && top.1.is_finite());
    assert!(bottom.0.x.is_finite() && bottom.0.y.is_finite() && bottom.1.is_finite());
    assert!(top.0.y < bottom.0.y);

    let faces = skin_preview_faces(stage, 15.0, -0.42, -0.12, false);
    assert!(!faces.is_empty());
    assert!(faces.iter().all(|face| {
        face.points
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite())
            && face.depth.is_finite()
    }));
    assert!(faces.windows(2).all(|pair| pair[0].depth <= pair[1].depth));
}

#[test]
// 验证头部头像预览会绘制外层的 3D 皮肤图层（贴图 x>=0.5 的 atlas 区域）。
/// 实现 head头像renderstheouter皮肤layer 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn head_avatar_renders_the_outer_3d_skin_layer() {
    let stage = Rect::from_center_size(Pos2::new(64.0, 64.0), Vec2::splat(48.0));
    let faces = skin_head_preview_faces(stage, 3.0, -0.48, -0.14);
    assert!(!faces.is_empty());
    assert!(
        faces.iter().any(|face| face.uv.left() >= 0.5),
        "the 3D head uses the outer-layer atlas at x=32"
    );
}

#[test]
// 验证命中检测把屏幕坐标反投影回纹理像素坐标：
// 面四角与中心映射正确，点落在面外时返回 None。
/// 实现 hittestingmapsprojectedfacesback到texturepixels 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn hit_testing_maps_projected_faces_back_to_texture_pixels() {
    let face = SkinPreviewFace {
        key: SkinFaceKey {
            part: SkinPart::Head,
            face: 0,
        },
        points: [
            Pos2::new(10.0, 20.0),
            Pos2::new(50.0, 20.0),
            Pos2::new(50.0, 60.0),
            Pos2::new(10.0, 60.0),
        ],
        uv: Rect::from_min_max(
            Pos2::new(8.0 / 64.0, 8.0 / 64.0),
            Pos2::new(16.0 / 64.0, 16.0 / 64.0),
        ),
        depth: 1.0,
        tint: Color32::WHITE,
    };
    let top_left = face.texture_position(Pos2::new(10.0, 20.0)).unwrap();
    let center = face.texture_position(Pos2::new(30.0, 40.0)).unwrap();
    let bottom_right = face.texture_position(Pos2::new(50.0, 60.0)).unwrap();
    // 当前皮肤图集和 egui 纹理都以左上角为原点，拾取结果应保持原始 V 方向。
    assert!((top_left.x - 8.0 / 64.0).abs() < 0.0001);
    assert!((top_left.y - 8.0 / 64.0).abs() < 0.0001);
    assert!((center.x - 12.0 / 64.0).abs() < 0.0001);
    assert!((center.y - 12.0 / 64.0).abs() < 0.0001);
    assert!((bottom_right.x - 16.0 / 64.0).abs() < 0.0001);
    assert!((bottom_right.y - 16.0 / 64.0).abs() < 0.0001);
    assert!(face.texture_position(Pos2::new(5.0, 10.0)).is_none());
    assert!(face.texture_position(Pos2::new(f32::NAN, 20.0)).is_none());
    assert!(
        face.texture_position(Pos2::new(10.0, f32::INFINITY))
            .is_none()
    );

    // 网格采样与鼠标反投影共用同一个 UV 规范化函数，且不得把图集 Y 整体翻转。
    let top_sample = map_skin_atlas_uv_to_egui(Pos2::new(8.0 / 64.0, 8.0 / 64.0));
    let bottom_sample = map_skin_atlas_uv_to_egui(Pos2::new(16.0 / 64.0, 16.0 / 64.0));
    assert!((top_sample.y - 8.0 / 64.0).abs() < 0.0001);
    assert!((bottom_sample.y - 16.0 / 64.0).abs() < 0.0001);
}

#[test]
// 验证默认皮肤像素包含可见的脸与衣服（alpha 为 255 且非纯黑）。
/// 实现 默认皮肤hasvisiblefaceandclothing 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn default_skin_has_a_visible_face_and_clothing() {
    let pixels = default_skin_pixels();
    let face_offset = (12 * 64 + 12) * 4;
    let shirt_offset = (24 * 64 + 24) * 4;
    assert_eq!(pixels[face_offset + 3], 255);
    assert_eq!(pixels[shirt_offset + 3], 255);
    assert_ne!(&pixels[face_offset..face_offset + 3], &[0, 0, 0]);
}

#[test]
// 验证皮肤 RGB 颜色导出为 6 位十六进制大写字符串（忽略 alpha 通道）。
/// 实现 rgbcolorreadoutkeeps全部channelsexact 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn rgb_color_readout_keeps_all_channels_exact() {
    assert_eq!(skin_rgb_hex([0, 0, 0, 255]), "#000000");
    assert_eq!(skin_rgb_hex([255, 255, 255, 255]), "#FFFFFF");
    assert_eq!(skin_rgb_hex([18, 171, 244, 255]), "#12ABF4");
}

#[test]
// 验证皮肤编辑历史：撤销/重做、重做后再编辑会丢弃重做分支。
/// 实现 皮肤historyundoesredoandinvalidatesbranchafternew编辑 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn skin_history_undoes_redo_and_invalidates_branch_after_new_edit() {
    let mut history = SkinEditHistory::default();
    let first = vec![0, 0, 0, 255];
    let second = vec![20, 40, 60, 255];
    let third = vec![90, 80, 70, 255];
    history.record(first.clone(), &second);
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo(&second).as_deref(), Some(first.as_slice()));
    assert!(history.can_redo());
    assert_eq!(history.redo(&first).as_deref(), Some(second.as_slice()));
    assert_eq!(history.undo(&second).as_deref(), Some(first.as_slice()));
    history.record(first.clone(), &third);
    assert!(!history.can_redo());
    assert_eq!(history.undo(&third).as_deref(), Some(first.as_slice()));
}

#[test]
// 验证皮肤编辑历史有上限且忽略无效（无变化）编辑。
// 记录 SKIN_HISTORY_LIMIT + 12 次后，undo 栈长度被裁剪为 SKIN_HISTORY_LIMIT。
/// 实现 皮肤historyboundedandignoresnoopedits 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn skin_history_is_bounded_and_ignores_noop_edits() {
    let mut history = SkinEditHistory::default();
    let original = vec![0; 16 * 16 * 4];
    history.record(original.clone(), &original);
    assert!(!history.can_undo());
    for index in 0..(SKIN_HISTORY_LIMIT + 12) {
        let before = vec![index as u8; 16 * 16 * 4];
        let current = vec![(index + 1) as u8; 16 * 16 * 4];
        history.record(before, &current);
    }
    assert_eq!(history.undo.len(), SKIN_HISTORY_LIMIT);
}

#[test]
// 验证切页弹簧平滑会"轻微过冲"（峰值 >1 且 <1.12），最终收敛到目标 1.0、速度 0。
/// 实现 创建tabtransitionspringovershootssoftlyandsettles 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn create_tab_transition_spring_overshoots_softly_and_settles() {
    let mut progress = 0.0_f32;
    let mut velocity = 0.0_f32;
    let mut peak = 0.0_f32;
    for _ in 0..120 {
        spring_smooth(&mut progress, &mut velocity, 1.0, 1.0 / 60.0);
        peak = peak.max(progress);
    }
    assert!(peak > 1.0);
    assert!(peak < 1.12);
    assert_eq!(progress, 1.0);
    assert_eq!(velocity, 0.0);
}

#[test]
// 验证资源分类索引与 Tab 顺序一致：None=0，Mod=1，Modpack=2，Shader=3，ResourcePack=4，Datapack=5。
/// 实现 下载categoryindicesmatchthetaborder 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn download_category_indices_match_the_tab_order() {
    assert_eq!(resource_category_index(None), 0);
    assert_eq!(resource_category_index(Some(ResourceCategory::Mod)), 1);
    assert_eq!(resource_category_index(Some(ResourceCategory::Modpack)), 2);
    assert_eq!(resource_category_index(Some(ResourceCategory::Shader)), 3);
    assert_eq!(
        resource_category_index(Some(ResourceCategory::ResourcePack)),
        4
    );
    assert_eq!(resource_category_index(Some(ResourceCategory::Datapack)), 5);
}

#[test]
// 验证外观模式默认是 Simple（简洁）。
/// 实现 appearancedefaults到simple 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn appearance_defaults_to_simple() {
    let settings = LauncherSettings::default();
    assert_eq!(settings.appearance_mode, theme::AppearanceMode::Simple);
    assert_eq!(settings.corner_radius, theme::DEFAULT_CORNER_RADIUS as u16);
}

#[test]
/// 验证自定义版本名按“游戏目录 + 真实 profile”隔离，并可清空恢复原名。
fn custom_version_names_are_scoped_and_reversible() {
    let mut settings = LauncherSettings::default();
    let first = PathBuf::from(r"D:\Games\First");
    let second = PathBuf::from(r"D:\Games\Second");
    settings.set_version_name(&first, "1.21.1", Some("愚者生存"));
    settings.set_version_name(&second, "1.21.1", Some("另一个实例"));
    settings.set_version_memory(&first, "1.21.1", 8192);
    settings.set_version_java_path(&first, "1.21.1", Some(std::path::Path::new("java.exe")));
    settings.recent_versions.push(RecentPlay {
        version: "1.21.1".to_owned(),
        played_at: 1,
    });

    assert_eq!(
        settings.version_name(&first, "1.21.1").as_deref(),
        Some("愚者生存")
    );
    assert_eq!(
        settings.version_name(&second, "1.21.1").as_deref(),
        Some("另一个实例")
    );

    settings.rename_version_references(&first, "1.21.1", "愚者生存版");
    assert!(settings.version_name(&first, "1.21.1").is_none());
    assert_eq!(settings.version_memory_mb(&first, "愚者生存版"), Some(8192));
    assert_eq!(
        settings.version_java_path(&first, "愚者生存版"),
        Some(PathBuf::from("java.exe"))
    );
    assert_eq!(settings.recent_versions[0].version, "愚者生存版");
    assert_eq!(
        settings.version_name(&second, "1.21.1").as_deref(),
        Some("另一个实例")
    );
}

#[test]
/// 验证标题输入增长时会立即补齐逐字符样式，避免下一处索引访问越界。
fn hero_title_style_arrays_follow_live_text_input() {
    let mut settings = LauncherSettings::default();
    settings.hero_title.push_str("新增文字\n第二行");
    settings.sync_hero_title_char_styles();
    let count = settings.hero_title.chars().count();
    assert_eq!(settings.hero_title_char_sizes.len(), count);
    assert_eq!(settings.hero_title_char_colors.len(), count);
}

#[test]
/// 验证在标题中间插入或删除字符时，逐字样式会随原字符位置同步移动。
fn hero_title_styles_follow_inserted_character_positions() {
    let mut settings = LauncherSettings {
        hero_title: "abc".to_owned(),
        hero_title_char_sizes: vec![10, 20, 30],
        hero_title_char_colors: vec![[1, 2, 3], [4, 5, 6], [7, 8, 9]],
        ..LauncherSettings::default()
    };

    settings.hero_title = "aXbc".to_owned();
    settings.reconcile_hero_title_char_styles("abc");
    assert_eq!(settings.hero_title_char_sizes, vec![10, 58, 20, 30]);
    assert_eq!(
        settings.hero_title_char_colors,
        vec![[1, 2, 3], [245, 250, 238], [4, 5, 6], [7, 8, 9]]
    );

    let previous = settings.hero_title.clone();
    settings.hero_title = "aXc".to_owned();
    settings.reconcile_hero_title_char_styles(&previous);
    assert_eq!(settings.hero_title_char_sizes, vec![10, 58, 30]);
    assert_eq!(
        settings.hero_title_char_colors,
        vec![[1, 2, 3], [245, 250, 238], [7, 8, 9]]
    );
}

#[test]
// 验证外观模式从旧版 glass 设置迁移：
// 旧版无玻璃(false)→Simple；旧版有玻璃(true)→Frosted；
// 新版带 Liquid 且 revision 为当前值、旧玻璃关→Frosted（强制玻璃降级）。
/// 实现 appearancemodesmigrate从legacy玻璃settings 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn appearance_modes_migrate_from_legacy_glass_settings() {
    assert_eq!(
        resolve_appearance_mode(0, None, Some(false), None),
        theme::AppearanceMode::Simple
    );
    assert_eq!(
        resolve_appearance_mode(0, None, Some(true), Some("regular")),
        theme::AppearanceMode::Frosted
    );
    assert_eq!(
        resolve_appearance_mode(0, None, Some(true), Some("clear")),
        theme::AppearanceMode::Frosted
    );
    assert_eq!(
        resolve_appearance_mode(
            SETTINGS_REVISION,
            Some(theme::AppearanceMode::Liquid),
            Some(false),
            None
        ),
        theme::AppearanceMode::Frosted
    );
}

#[test]
// 验证动画等级覆盖四种强度：标签为 关/弱/中/强；Off 强度 0；Weak<Medium<Strong。
/// 实现 动画levelscoverfourdistinctstrengths 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn animation_levels_cover_four_distinct_strengths() {
    assert_eq!(
        AnimationLevel::ALL.map(AnimationLevel::label),
        ["关", "弱", "中", "强"]
    );
    assert_eq!(AnimationLevel::Off.strength(), 0.0);
    assert!(AnimationLevel::Weak.strength() < AnimationLevel::Medium.strength());
    assert!(AnimationLevel::Strong.strength() > AnimationLevel::Medium.strength());
}

#[test]
// 验证版本隔离默认开启。
/// 实现 版本isolationenabled默认 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_isolation_is_enabled_by_default() {
    assert!(LauncherSettings::default().version_isolation);
}

#[test]
// 验证版本内容弹窗的关闭按钮位于弹窗内部且尺寸 >=40x40。
/// 实现 版本内容关闭targetlargeandinsidedialog 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_content_close_target_is_large_and_inside_dialog() {
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0));
    let dialog = version_content_dialog_rect(screen);
    let close = version_content_close_rect(dialog);
    assert!(dialog.contains(close.center()));
    assert!(close.width() >= 40.0);
    assert!(close.height() >= 40.0);
}

#[test]
// 验证版本内容文本透明度无跳变：0~0.16 为 0，1.0 为 1，且随进度单调不减。
/// 实现 版本内容文本fadeswithoutopacityjump 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_content_text_fades_without_opacity_jump() {
    assert_eq!(version_content_text_opacity(0.0), 0.0);
    assert_eq!(version_content_text_opacity(0.16), 0.0);
    assert_eq!(version_content_text_opacity(1.0), 1.0);
    let mut previous = 0.0;
    for step in 0..=100 {
        let opacity = version_content_text_opacity(step as f32 / 100.0);
        assert!(opacity + 0.0001 >= previous);
        previous = opacity;
    }
}

#[test]
// 验证官方启动器 ISO 时间戳解析：epoch 为 0，后于前者则更大，非法字符串为 None。
/// 实现 parsesofficiallaunchertimestamps 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn parses_official_launcher_timestamps() {
    assert_eq!(parse_iso_timestamp("1970-01-01T00:00:00Z"), Some(0));
    assert!(
        parse_iso_timestamp("2026-10-31T17:48:22Z") > parse_iso_timestamp("2026-10-31T17:48:21Z")
    );
    assert!(parse_iso_timestamp("not-a-timestamp").is_none());
}

#[test]
// 验证关闭弹窗的弹簧动画一旦透明度归零就不会再回升（不会"淡入回弹"）。
/// 实现 closingpopupneverfadesback 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn closing_popup_never_fades_back_in() {
    let mut value = 1.0_f32;
    let mut velocity = 0.0_f32;
    let mut fully_hidden = false;
    for _ in 0..120 {
        spring_popup(&mut value, &mut velocity, 0.0, 1.0 / 60.0);
        let opacity = popup_opacity(value, false);
        if opacity == 0.0 {
            fully_hidden = true;
        } else {
            assert!(!fully_hidden);
        }
    }
    assert!(fully_hidden);
    assert_eq!(value, 0.0);
}

#[test]
// 验证卡片运动：前沿（靠下）的卡片偏移更大（先动），
// 向下移动时 lower 偏移小于 upper；向上移动时相反。
/// 实现 cardstheleadingedge启动first 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn cards_at_the_leading_edge_start_first() {
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0));
    let upper = Rect::from_min_size(Pos2::new(0.0, 80.0), Vec2::new(100.0, 80.0));
    let lower = Rect::from_min_size(Pos2::new(0.0, 760.0), Vec2::new(100.0, 80.0));
    let moving_down = CardMotion {
        screen,
        direction: 1.0,
        progress: 0.2,
        outgoing: false,
    };
    assert!(moving_down.offset(lower) < moving_down.offset(upper));

    let moving_up = CardMotion {
        screen,
        direction: -1.0,
        progress: 0.2,
        outgoing: false,
    };
    assert!(moving_up.offset(upper) > moving_up.offset(lower));
}

#[test]
// 验证写入 Fabric 模组工程骨架：生成 build.gradle、fabric.mod.json、
// 包路径下的主 Java 类。
/// 实现 writesfabric模组项目skeleton 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn writes_fabric_mod_project_skeleton() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-mod-project-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let project = write_mod_project(
        &root,
        &ModProjectSettings {
            name: "Example Tool".to_owned(),
            id: "example_tool".to_owned(),
            package: "com.example.exampletool".to_owned(),
            version: "1.0.0".to_owned(),
            minecraft: "1.21.1".to_owned(),
            loader: 0,
        },
    )
    .unwrap();
    assert!(project.join("build.gradle").is_file());
    assert!(project.join("src/main/resources/fabric.mod.json").is_file());
    assert!(
        project
            .join("src/main/java/com/example/exampletool/ExampleToolMod.java")
            .is_file()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
// 验证模组类名生成：去掉非字母数字、各单词首字母大写拼接、缺省回退、末尾补 "Mod"。
/// 实现 createssafe模组classnames 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn creates_safe_mod_class_names() {
    assert_eq!(mod_class_name("My Cool Mod", "fallback"), "MyCoolMod");
    assert_eq!(mod_class_name("", "fallback-id"), "FallbackIdMod");
}

#[test]
// 验证导出版本归档（Zip 格式，含 mods 与 config）：
// 归档内含 modrinth.index.json，且 overrides 下包含 sample.jar / sample.toml。
/// 实现 exports版本压缩包withselected内容 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn exports_version_archive_with_selected_content() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-export-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = root.join(".minecraft");
    let profile = "test-version";
    let version_dir = base.join("versions").join(profile);
    std::fs::create_dir_all(&version_dir).unwrap();
    std::fs::write(version_dir.join(format!("{profile}.json")), "{}").unwrap();
    std::fs::create_dir_all(base.join("mods")).unwrap();
    std::fs::write(base.join("mods").join("sample.jar"), b"mod").unwrap();
    std::fs::create_dir_all(base.join("config")).unwrap();
    std::fs::write(base.join("config").join("sample.toml"), b"config").unwrap();
    let output = root.join("export.zip");
    let mut item = version(profile);
    item.game_version = "1.21.1".to_owned();
    export_version_archive(
        &base,
        &base,
        &item,
        &output,
        &VersionExportOptions {
            mods: true,
            config: true,
            saves: false,
            resourcepacks: false,
            shaderpacks: false,
            format: ModpackExportFormat::Zip,
        },
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    assert!(archive.by_name("modrinth.index.json").is_ok());
    assert!(archive.by_name("overrides/mods/sample.jar").is_ok());
    assert!(archive.by_name("overrides/config/sample.toml").is_ok());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
// 验证导出 Modrinth 包（.mrpack，含本地 overrides，无托管文件时 files 为空）：
// 索引 formatVersion=1，依赖 minecraft/fabric-loader 正确，files 为空数组。
/// 实现 exportsmodrinth包withlocaloverrides 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn exports_modrinth_pack_with_local_overrides() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-modrinth-export-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = root.join(".minecraft");
    std::fs::create_dir_all(base.join("mods")).unwrap();
    std::fs::write(base.join("mods").join("sample.jar"), b"mod").unwrap();
    std::fs::create_dir_all(base.join("config")).unwrap();
    std::fs::write(base.join("config").join("sample.toml"), b"config").unwrap();
    let output = root.join("export.mrpack");
    let mut item = version("fabric-1.21.1");
    item.game_version = "1.21.1".to_owned();
    item.loader_family = "Fabric".to_owned();
    item.loader = "Fabric 0.16.14".to_owned();
    let options = VersionExportOptions {
        mods: true,
        config: true,
        saves: false,
        resourcepacks: false,
        shaderpacks: false,
        format: ModpackExportFormat::Modrinth,
    };
    let files = collect_modpack_files(&base, &options).unwrap();
    export_pack_with_hosted_files(&item, &output, &options, &files, &HashMap::new()).unwrap();

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    assert!(archive.by_name("overrides/mods/sample.jar").is_ok());
    assert!(archive.by_name("overrides/config/sample.toml").is_ok());
    let mut index = String::new();
    use std::io::Read as _;
    archive
        .by_name("modrinth.index.json")
        .unwrap()
        .read_to_string(&mut index)
        .unwrap();
    let index: serde_json::Value = serde_json::from_str(&index).unwrap();
    assert_eq!(index["formatVersion"], 1);
    assert_eq!(index["dependencies"]["minecraft"], "1.21.1");
    assert_eq!(index["dependencies"]["fabric-loader"], "0.16.14");
    assert_eq!(index["files"], serde_json::json!([]));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
// 验证 Modrinth 导出会把"托管文件"移出 overrides，改为 files 列表（含 sha512 哈希与下载地址）。
/// 实现 modrinth导出moveshosted文件outoverrides 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn modrinth_export_moves_hosted_files_out_of_overrides() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-modrinth-hosted-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = root.join(".minecraft");
    std::fs::create_dir_all(base.join("mods")).unwrap();
    std::fs::write(base.join("mods").join("hosted.jar"), b"hosted mod").unwrap();
    let options = VersionExportOptions {
        mods: true,
        config: false,
        saves: false,
        resourcepacks: false,
        shaderpacks: false,
        format: ModpackExportFormat::Modrinth,
    };
    let files = collect_modpack_files(&base, &options).unwrap();
    let sha1 = files[0].sha1.clone().unwrap();
    let hosted = HashMap::from([(
        sha1,
        HostedPackFile {
            downloads: vec!["https://cdn.modrinth.com/data/example/hosted.jar".to_owned()],
        },
    )]);
    let output = root.join("hosted.mrpack");
    let mut item = version("fabric-1.21.1");
    item.game_version = "1.21.1".to_owned();
    item.loader_family = "Fabric".to_owned();
    item.loader = "Fabric 0.16.14".to_owned();
    export_pack_with_hosted_files(&item, &output, &options, &files, &hosted).unwrap();

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    assert!(archive.by_name("overrides/mods/hosted.jar").is_err());
    let mut index = String::new();
    use std::io::Read as _;
    archive
        .by_name("modrinth.index.json")
        .unwrap()
        .read_to_string(&mut index)
        .unwrap();
    let index: serde_json::Value = serde_json::from_str(&index).unwrap();
    assert_eq!(index["files"][0]["path"], "mods/hosted.jar");
    assert_eq!(
        index["files"][0]["downloads"][0],
        "https://cdn.modrinth.com/data/example/hosted.jar"
    );
    assert_eq!(
        index["files"][0]["hashes"]["sha512"]
            .as_str()
            .unwrap()
            .len(),
        128
    );
    let _ = std::fs::remove_dir_all(root);
}
