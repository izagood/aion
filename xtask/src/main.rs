use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() -> anyhow::Result<()> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("ebpf-build") => ebpf_build()?,
        Some("proto-gen") => proto_gen()?,
        Some(other) => anyhow::bail!("unknown xtask: {other}"),
        None => print_help(),
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    let manifest = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().unwrap());
    // xtask/ is one level below workspace root
    manifest.parent().unwrap_or(&manifest).to_path_buf()
}

fn ebpf_build() -> anyhow::Result<()> {
    let root = workspace_root();
    let ebpf_dir = root.join("crates/aion-ebpf");

    println!("Building eBPF programs...");
    let status = Command::new("cargo")
        .args([
            "+nightly",
            "build",
            "--target=bpfel-unknown-none",
            "-Z",
            "build-std=core",
            "--release",
        ])
        .current_dir(&ebpf_dir)
        .status()?;

    if !status.success() {
        anyhow::bail!("eBPF build failed");
    }

    println!("eBPF programs built successfully");
    Ok(())
}

fn proto_gen() -> anyhow::Result<()> {
    let root = workspace_root();
    let proto_dir = root.join("proto");
    let out_dir = root.join("proto/gen");

    std::fs::create_dir_all(&out_dir)?;

    let protos = [
        "aion/v1/metrics.proto",
        "aion/v1/proposal.proto",
        "aion/v1/agent.proto",
        "aion/v1/audit.proto",
    ];

    println!("Generating protobuf code...");

    let proto_paths: Vec<PathBuf> = protos
        .iter()
        .map(|p| proto_dir.join(p))
        .collect();

    let mut config = prost_build::Config::new();
    config.out_dir(&out_dir);
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    tonic_build::configure()
        .out_dir(&out_dir)
        .build_server(true)
        .build_client(true)
        .compile_protos_with_config(config, &proto_paths, &[&proto_dir])?;

    println!("Protobuf code generated in {}", out_dir.display());
    Ok(())
}

fn print_help() {
    eprintln!(
        "Usage: cargo xtask <TASK>

Tasks:
  ebpf-build    Build eBPF programs
  proto-gen     Generate protobuf code
"
    );
}
