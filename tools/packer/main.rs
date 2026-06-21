//! Wind Packer —— 通用安装器生成工具。
//!
//! 由 `app.toml` 单一配置驱动：应用身份、打包文件、品牌（logo/icon）全部在其中描述。
//! 同一个预编译 stub（wind-installer.exe）配不同 app.toml 即可生成不同应用的安装包，
//! 无需重新编译。
//!
//! 三个子命令：
//! - `pack`   压缩源目录为 .bin（含 manifest + logo，慢，仅需一次）
//! - `bundle` 给 stub 写图标后拼接 .bin 为安装程序（快，可反复执行）
//! - `build`  pack + bundle 一步到位

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use wind_installer::archive::{self, ArchiveWriter, CompressionType};
use wind_installer::manifest::ProjectConfig;

#[derive(Parser, Debug)]
#[command(name = "wind-packer")]
#[command(about = "Generic installer generator driven by app.toml")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 压缩源目录为 .bin 归档（嵌入 manifest 与 logo）
    Pack {
        /// app.toml 配置文件
        #[arg(short, long, default_value = "app.toml")]
        config: PathBuf,
        /// 输出 .bin（默认 <output_dir>/<output_name>-<version>.bin）
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 给 stub 写图标后拼接 .bin 为最终安装程序
    Bundle {
        /// app.toml 配置文件
        #[arg(short, long, default_value = "app.toml")]
        config: PathBuf,
        /// stub 文件（wind-installer.exe）
        #[arg(short, long)]
        stub: PathBuf,
        /// 输入 .bin（默认 <output_dir>/<output_name>-<version>.bin）
        #[arg(short, long)]
        archive: Option<PathBuf>,
        /// 输出安装程序（默认 <output_dir>/<output_name>-<version>.exe）
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// pack + bundle 一步到位
    Build {
        /// app.toml 配置文件
        #[arg(short, long, default_value = "app.toml")]
        config: PathBuf,
        /// stub 文件（wind-installer.exe）
        #[arg(short, long)]
        stub: PathBuf,
        /// 输出安装程序（默认 <output_dir>/<output_name>-<version>.exe）
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 读取已生成的安装程序（或 .bin），打印其嵌入的清单摘要
    Inspect {
        /// 安装程序 .exe 或归档 .bin
        #[arg(short, long)]
        file: PathBuf,
    },
}

fn main() {
    let args = Args::parse();
    let result = match args.command {
        Command::Pack { config, output } => cmd_pack(&config, output).map(|_| ()),
        Command::Bundle { config, stub, archive, output } => cmd_bundle(&config, &stub, archive, output),
        Command::Build { config, stub, output } => cmd_build(&config, &stub, output),
        Command::Inspect { file } => cmd_inspect(&file),
    };
    if let Err(e) = result {
        eprintln!("错误: {}", e);
        std::process::exit(1);
    }
}

// ── 配置加载与路径解析 ──────────────────────────────────────────────────────

struct Loaded {
    cfg: ProjectConfig,
    /// app.toml 所在目录，相对路径以此为基准
    base: PathBuf,
}

fn load(config: &Path) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(config)
        .map_err(|e| format!("读取配置 {:?} 失败: {}", config, e))?;
    let cfg = ProjectConfig::from_toml_str(&text)?;
    let base = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(Loaded { cfg, base })
}

impl Loaded {
    /// 相对 app.toml 解析路径
    fn resolve(&self, p: &str) -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base.join(path)
        }
    }

    fn version(&self) -> &str {
        &self.cfg.manifest.app.version
    }

    fn output_base(&self) -> String {
        let name = if self.cfg.package.output_name.is_empty() {
            self.cfg.manifest.app.id.clone()
        } else {
            self.cfg.package.output_name.clone()
        };
        format!("{}-{}", name, self.version())
    }

    fn default_bin(&self) -> PathBuf {
        self.resolve(&self.cfg.package.output_dir)
            .join(format!("{}.bin", self.output_base()))
    }

    fn default_exe(&self) -> PathBuf {
        self.resolve(&self.cfg.package.output_dir)
            .join(format!("{}.exe", self.output_base()))
    }
}

// ── pack ────────────────────────────────────────────────────────────────────

fn cmd_pack(config: &Path, output: Option<PathBuf>) -> Result<PathBuf, String> {
    let l = load(config)?;
    let output = output.unwrap_or_else(|| l.default_bin());
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir).ok();
    }

    let source = l.resolve(&l.cfg.package.source_dir);
    if !source.exists() {
        return Err(format!("源目录不存在: {:?}", source));
    }

    let compression = match l.cfg.package.compression.to_lowercase().as_str() {
        "lzma" | "xz" => CompressionType::Lzma,
        _ => CompressionType::Zstd,
    };

    // 运行期清单（TOML 文本字节）
    let manifest_bytes = l.cfg.manifest.to_toml_bytes()?;

    // logo 字节（可选）
    let logo_bytes = if l.cfg.package.logo.is_empty() {
        Vec::new()
    } else {
        let logo_path = l.resolve(&l.cfg.package.logo);
        match std::fs::read(&logo_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("警告: 无法读取 logo {:?}: {}（将使用空 logo）", logo_path, e);
                Vec::new()
            }
        }
    };

    println!("Wind Packer · pack");
    println!("  应用:   {} {}", l.cfg.manifest.app.display_name, l.version());
    println!("  源目录: {:?}", source);
    println!("  压缩:   {:?}", compression);
    println!("  清单:   {} 字节", manifest_bytes.len());
    println!("  logo:   {} 字节", logo_bytes.len());

    let mut writer = ArchiveWriter::new(&output, compression)?;
    writer.set_manifest(manifest_bytes, logo_bytes);
    writer.add_directory(&source, "")?;
    let entry_count = writer.entry_count();
    writer.finish()?;

    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    println!("  → {:?}（{} 个文件, {:.2} MB）", output, entry_count, size as f64 / 1048576.0);
    Ok(output)
}

// ── bundle ──────────────────────────────────────────────────────────────────

fn cmd_bundle(
    config: &Path,
    stub: &Path,
    archive: Option<PathBuf>,
    output: Option<PathBuf>,
) -> Result<(), String> {
    let l = load(config)?;
    let archive = archive.unwrap_or_else(|| l.default_bin());
    let output = output.unwrap_or_else(|| l.default_exe());
    bundle_inner(&l, stub, &archive, &output)
}

fn bundle_inner(l: &Loaded, stub: &Path, archive: &Path, output: &Path) -> Result<(), String> {
    if !stub.exists() {
        return Err(format!("stub 不存在: {:?}", stub));
    }
    if !archive.exists() {
        return Err(format!("归档不存在: {:?}（请先 pack）", archive));
    }
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir).ok();
    }

    println!("Wind Packer · bundle");

    // 先给干净 stub 写图标（在追加归档 overlay 之前），规避 PE overlay 被重建丢弃的风险
    let mut iconned_stub: Option<PathBuf> = None;
    let effective_stub: PathBuf = if l.cfg.package.icon.is_empty() {
        println!("  图标:   （未指定，沿用 stub 自带图标）");
        stub.to_path_buf()
    } else {
        let icon = l.resolve(&l.cfg.package.icon);
        if !icon.exists() {
            return Err(format!("图标文件不存在: {:?}", icon));
        }
        let tmp = output.with_extension("stub.tmp");
        set_exe_icon(stub, &icon, &tmp)?;
        println!("  图标:   {:?} → 已写入 stub", icon);
        iconned_stub = Some(tmp.clone());
        tmp
    };

    let stub_size = std::fs::metadata(&effective_stub).map(|m| m.len()).unwrap_or(0);
    archive::bundle_exe(&effective_stub, archive, output)?;

    if let Some(tmp) = iconned_stub {
        let _ = std::fs::remove_file(&tmp);
    }

    let total = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    println!("  → {:?}（stub={} 字节, 总计 {:.2} MB）", output, stub_size, total as f64 / 1048576.0);
    Ok(())
}

// ── build（pack + bundle）───────────────────────────────────────────────────

fn cmd_build(config: &Path, stub: &Path, output: Option<PathBuf>) -> Result<(), String> {
    let bin = cmd_pack(config, None)?;
    let l = load(config)?;
    let output = output.unwrap_or_else(|| l.default_exe());
    bundle_inner(&l, stub, &bin, &output)
}

// ── inspect ─────────────────────────────────────────────────────────────────

fn cmd_inspect(file: &Path) -> Result<(), String> {
    use wind_installer::archive::ArchiveReader;
    use wind_installer::manifest::AppManifest;

    let reader = ArchiveReader::open(file)?;
    println!("文件:     {:?}", file);
    println!("文件数:   {}", reader.entries().len());
    println!("压缩:     {:?}", reader.compression_type());
    println!("logo:     {} 字节", reader.logo_bytes().len());

    let mb = reader.manifest_bytes();
    if mb.is_empty() {
        println!("清单:     （无——该归档不含运行期清单，无法作为安装器使用）");
        return Ok(());
    }
    let m = AppManifest::from_toml_bytes(mb)?;
    println!("清单:     {} 字节", mb.len());
    println!("  id            = {}", m.app.id);
    println!("  display_name  = {}", m.app.display_name);
    println!("  version       = {}", m.app.version);
    println!("  main_exe      = {}", m.app.main_exe);
    println!("  url_protocol  = {}", m.app.url_protocol);
    println!("  ime           = {}", if m.ime.is_some() { "有" } else { "无" });
    println!("  font          = {} 项", m.font.len());
    Ok(())
}

// ── 图标写入（纯 Rust，无外部依赖）──────────────────────────────────────────

/// 将 `icon` 写入 `stub` 的 PE 资源，输出到 `out`。
///
/// 仅对无 overlay 的干净 stub 使用——重建 PE 资源目录时尾部 overlay 不保证保留，
/// 因此本函数必须在 bundle 追加归档之前调用。
fn set_exe_icon(stub: &Path, icon: &Path, out: &Path) -> Result<(), String> {
    let mut image = editpe::Image::parse_file(stub)
        .map_err(|e| format!("解析 stub PE 失败: {}", e))?;
    let mut resources = image.resource_directory().cloned().unwrap_or_default();
    resources
        .set_main_icon_file(&icon.to_string_lossy())
        .map_err(|e| format!("设置图标失败: {}", e))?;
    image
        .set_resource_directory(resources)
        .map_err(|e| format!("写入资源目录失败: {}", e))?;
    image
        .write_file(out)
        .map_err(|e| format!("写出带图标 stub 失败: {}", e))?;
    Ok(())
}
