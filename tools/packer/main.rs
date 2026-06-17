use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Wind Packer - 分阶段打包工具
#[derive(Parser, Debug)]
#[command(name = "wind-packer")]
#[command(about = "Pack files into Wind Installer archive")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 阶段一：将源目录压缩打包为 .bin 归档文件（慢，仅需执行一次）
    Pack {
        /// 源目录
        #[arg(short, long)]
        source: PathBuf,

        /// 输出 .bin 文件
        #[arg(short, long)]
        output: PathBuf,

        /// 压缩算法 (zstd/lzma)
        #[arg(short, long, default_value = "zstd")]
        compression: String,
    },

    /// 阶段二：将 Stub (.exe) + 归档 (.bin) 快速拼接为安装程序（快，可反复执行）
    Bundle {
        /// Stub 文件 (wind-installer.exe)
        #[arg(short, long)]
        stub: PathBuf,

        /// 归档文件 (.bin)
        #[arg(short, long)]
        archive: PathBuf,

        /// 输出安装程序 (.exe)
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() {
    let args = Args::parse();

    match args.command {
        Command::Pack { source, output, compression } => cmd_pack(source, output, compression),
        Command::Bundle { stub, archive, output } => cmd_bundle(stub, archive, output),
    }
}

/// 阶段一：压缩打包
fn cmd_pack(source: PathBuf, output: PathBuf, compression: String) {
    println!("Wind Packer - pack");
    println!("==================");
    println!("Source:      {:?}", source);
    println!("Output:      {:?}", output);
    println!("Compression: {}", compression);

    if !source.exists() {
        eprintln!("Error: Source directory does not exist: {:?}", source);
        std::process::exit(1);
    }

    let compression_type = match compression.to_lowercase().as_str() {
        "lzma" | "xz" => wind_installer::archive::CompressionType::Lzma,
        _ => wind_installer::archive::CompressionType::Zstd,
    };

    let mut writer = match wind_installer::archive::ArchiveWriter::new(&output, compression_type) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create archive writer: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = writer.add_directory(&source, "") {
        eprintln!("Failed to add directory: {}", e);
        std::process::exit(1);
    }

    let entry_count = writer.entry_count();

    match writer.finish() {
        Ok(header_offset) => {
            println!();
            println!("Archive created: {} entries, header at {}", entry_count, header_offset);
        }
        Err(e) => {
            eprintln!("Failed to finish archive: {}", e);
            std::process::exit(1);
        }
    }

    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    println!("Size:        {:.2} MB", size as f64 / 1048576.0);
    println!();
    println!("Done! Use 'bundle' to combine with stub into installer exe.");
}

/// 阶段二：快速拼接
fn cmd_bundle(stub: PathBuf, archive: PathBuf, output: PathBuf) {
    println!("Wind Packer - bundle");
    println!("====================");
    println!("Stub:    {:?}", stub);
    println!("Archive: {:?}", archive);
    println!("Output:  {:?}", output);

    if !stub.exists() {
        eprintln!("Error: Stub file does not exist: {:?}", stub);
        std::process::exit(1);
    }
    if !archive.exists() {
        eprintln!("Error: Archive file does not exist: {:?}", archive);
        std::process::exit(1);
    }

    // 快速拼接：stub + archive，利用 BufWriter 减少系统调用
    use std::io::{BufReader, BufWriter, Write};

    let archive_file = std::fs::File::open(&archive).unwrap_or_else(|e| {
        eprintln!("Failed to open archive: {}", e);
        std::process::exit(1);
    });
    let archive_len = archive_file.metadata().map(|m| m.len()).unwrap_or(0);

    let mut out = BufWriter::new(std::fs::File::create(&output).unwrap_or_else(|e| {
        eprintln!("Failed to create output: {}", e);
        std::process::exit(1);
    }));

    // 写入 stub
    let mut stub_reader = BufReader::new(std::fs::File::open(&stub).unwrap_or_else(|e| {
        eprintln!("Failed to open stub: {}", e);
        std::process::exit(1);
    }));

    std::io::copy(&mut stub_reader, &mut out).unwrap_or_else(|e| {
        eprintln!("Failed to write stub: {}", e);
        std::process::exit(1);
    });

    // 写入归档
    let mut archive_reader = BufReader::new(archive_file);
    std::io::copy(&mut archive_reader, &mut out).unwrap_or_else(|e| {
        eprintln!("Failed to write archive: {}", e);
        std::process::exit(1);
    });

    out.flush().ok();

    let total_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    println!();
    println!("Bundle complete!");
    println!("Archive entries size: {:.2} MB", archive_len as f64 / 1048576.0);
    println!("Installer size:       {:.2} MB", total_size as f64 / 1048576.0);
    println!();
    println!("Done!");
}
