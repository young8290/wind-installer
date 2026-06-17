use std::path::PathBuf;

use clap::Parser;

/// Wind Packer - 打包工具
#[derive(Parser, Debug)]
#[command(name = "wind-packer")]
#[command(about = "Pack files into Wind Installer archive")]
struct Args {
    /// 源目录
    #[arg(short, long)]
    source: PathBuf,

    /// 输出文件
    #[arg(short, long)]
    output: PathBuf,

    /// 压缩算法 (zstd/lzma)
    #[arg(short, long, default_value = "zstd")]
    compression: String,

    /// Stub 文件路径（可选，用于生成完整的安装包）
    #[arg(long)]
    stub: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    println!("Wind Packer v0.1.0");
    println!("==================");
    println!();
    println!("Source: {:?}", args.source);
    println!("Output: {:?}", args.output);
    println!("Compression: {}", args.compression);

    // 验证源目录
    if !args.source.exists() {
        eprintln!("Error: Source directory does not exist: {:?}", args.source);
        std::process::exit(1);
    }

    // 确定压缩类型
    let compression_type = match args.compression.to_lowercase().as_str() {
        "lzma" | "xz" => wind_installer::archive::CompressionType::Lzma,
        _ => wind_installer::archive::CompressionType::Zstd,
    };

    // 创建临时归档文件
    let temp_archive = args.output.with_extension("tmp");

    println!();
    println!("Creating archive...");

    let mut writer = match wind_installer::archive::ArchiveWriter::new(&temp_archive, compression_type) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create archive writer: {}", e);
            std::process::exit(1);
        }
    };

    // 添加目录内容
    if let Err(e) = writer.add_directory(&args.source, "") {
        eprintln!("Failed to add directory: {}", e);
        std::process::exit(1);
    }

    let entry_count = writer.entry_count();

    // 完成归档
    let header_offset = match writer.finish() {
        Ok(offset) => offset,
        Err(e) => {
            eprintln!("Failed to finish archive: {}", e);
            std::process::exit(1);
        }
    };

    println!("Archive created with {} entries", entry_count);
    println!("Header offset: {}", header_offset);

    // 如果指定了 Stub，拼接生成完整安装包
    if let Some(stub_path) = args.stub {
        println!();
        println!("Stub file: {:?}", stub_path);

        if !stub_path.exists() {
            eprintln!("Error: Stub file does not exist: {:?}", stub_path);
            std::process::exit(1);
        }

        println!("Creating installer...");

        // 读取 Stub
        let stub_data = match std::fs::read(&stub_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read stub: {}", e);
                std::process::exit(1);
            }
        };

        // 读取归档数据
        let archive_data = match std::fs::read(&temp_archive) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read archive: {}", e);
                std::process::exit(1);
            }
        };

        // 写入最终安装包
        let mut output = match std::fs::File::create(&args.output) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create output file: {}", e);
                std::process::exit(1);
            }
        };

        use std::io::Write;

        // 写入 Stub
        if let Err(e) = output.write_all(&stub_data) {
            eprintln!("Failed to write stub: {}", e);
            std::process::exit(1);
        }

        // 写入归档数据
        if let Err(e) = output.write_all(&archive_data) {
            eprintln!("Failed to write archive: {}", e);
            std::process::exit(1);
        }

        println!("Installer created: {:?}", args.output);
    } else {
        // 只输出归档文件
        if let Err(e) = std::fs::rename(&temp_archive, &args.output) {
            eprintln!("Failed to rename archive: {}", e);
            std::process::exit(1);
        }
        println!("Archive saved to: {:?}", args.output);
    }

    // 清理临时文件
    if temp_archive.exists() {
        let _ = std::fs::remove_file(&temp_archive);
    }

    println!();
    println!("Done!");
}
