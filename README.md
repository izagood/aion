# AION — AI-Native Autonomous Infrastructure OS

AION is an AI-powered autonomous infrastructure control fabric that runs on Linux + Kubernetes.
It detects infrastructure anomalies via eBPF, mounts AI Agents (Claude Code, Codex CLI, Gemini CLI)
as subprocesses, and performs autonomous remediation through MCP (Model Context Protocol).

## Key Features

- **eBPF Observability** — Kernel-level monitoring for OOM kills, CPU throttling, and cgroup pressure via aya-rs
- **AI Agent Mount** — Dynamically spawn and manage AI agents as subprocesses with MCP stdio transport
- **Autonomous Remediation** — Detect → Analyze → Propose → Validate → Execute pipeline
- **Zero-Trust Capabilities** — Cryptographic capability tokens for fine-grained access control
- **Tamper-Proof Audit** — SHA-256 hash-chain audit logging for every action

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      AION Agent Daemon                      │
│                                                             │
│  ┌──────────┐   ┌──────────┐   ┌───────────┐   ┌────────┐ │
│  │ Observe  │──▶│  Mount   │──▶│ Propose   │──▶│Validate│ │
│  │ (eBPF,   │   │ (Agent   │   │ (Schema   │   │(Policy │ │
│  │  cgroup, │   │  Spawn,  │   │  Valid.)  │   │ Chain) │ │
│  │  K8s)    │   │  MCP)    │   │           │   │        │ │
│  └──────────┘   └──────────┘   └───────────┘   └───┬────┘ │
│                                                     │      │
│                                                     ▼      │
│  ┌──────────┐   ┌──────────┐   ┌───────────────────────┐  │
│  │ Audit    │◀──│Capability│◀──│       Execute          │  │
│  │(Hash     │   │(Zero-    │   │  (K8s Operations)      │  │
│  │ Chain)   │   │ Trust)   │   │                        │  │
│  └──────────┘   └──────────┘   └───────────────────────┘  │
│                                                             │
│  ┌──────────────────────┐   ┌────────────────────────────┐ │
│  │   REST API (axum)    │   │     gRPC API (tonic)       │ │
│  └──────────────────────┘   └────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
        │                              │
        ▼                              ▼
  ┌───────────┐              ┌───────────────────┐
  │ AI Agent  │◀── stdio ──▶│  AION MCP Server  │
  │ (Claude,  │    (MCP)    │  (Tool Provider)   │
  │  Codex,   │              │                   │
  │  Gemini)  │              └───────────────────┘
  └───────────┘
```

## Crate Structure

| Crate | Description |
|-------|-------------|
| `aion-common` | Shared types, errors, configuration, and opaque handles |
| `aion-observe` | Observability collectors — eBPF, cgroup, Kubernetes watchers |
| `aion-ebpf` | eBPF programs (kernel space, built with aya-ebpf, `no_std`) |
| `aion-ebpf-common` | Shared types between eBPF programs and userspace |
| `aion-mount` | Agent Mount system — registry, MCP, launcher, governor, permissions |
| `aion-propose` | Proposal types and JSON schema validation |
| `aion-validate` | Policy validation chain |
| `aion-execute` | Deterministic executor for Kubernetes operations |
| `aion-capability` | Zero-trust cryptographic capability tokens |
| `aion-audit` | SHA-256 hash-chain audit logging |
| `aion-api` | REST (axum) + gRPC (tonic) API server |
| `aion-mcp-server` | Standalone MCP server binary (spawned by agents) |
| `aion-agent` | Main daemon binary — orchestrates the full pipeline |

## Prerequisites

- **Rust nightly-2025-12-01** (automatically installed via `rust-toolchain.toml`)
  - Components: `rust-src`, `rustfmt`, `clippy`
  - Target: `x86_64-unknown-linux-gnu`
- **System packages**:
  ```bash
  # Debian/Ubuntu
  sudo apt-get install protobuf-compiler libssl-dev pkg-config

  # Fedora/RHEL
  sudo dnf install protobuf-compiler openssl-devel pkg-config
  ```
- **Kubernetes 1.24+** (for runtime — not needed for building)
- **Docker 20.10+** (for container image builds)

## Installation & Build

```bash
# Clone the repository
git clone https://github.com/aion-infra/aion.git
cd aion

# Build all crates (nightly toolchain installs automatically)
cargo build

# Build in release mode
cargo build --release

# Build eBPF programs (requires bpf-linker)
cargo xtask ebpf-build

# Generate protobuf code
cargo xtask proto-gen
```

The release build applies LTO (`thin`), strips symbols, and uses a single codegen unit for optimal binary size.

## Configuration

AION uses layered TOML configuration. Files are loaded from the `config/` directory by default.

### `config/default.toml` — Daemon Settings

```toml
[daemon]
listen_addr = "0.0.0.0"
grpc_port = 50051
rest_port = 8080
log_level = "info"
audit_dir = "/var/lib/aion/audit"

[observe]
enable_ebpf = true
enable_cgroup = true
enable_kube_watcher = true
poll_interval_secs = 10

[pipeline]
max_concurrent_mounts = 3
canary_duration_secs = 30
```

### `config/agents.toml` — AI Agent Registry

Defines which AI agents are available, their priorities, budget limits, and specializations.

```toml
[global]
daily_budget_usd = 50.0
default_timeout_secs = 120
mcp_server_binary = "./target/release/aion-mcp-server"

[[agents]]
id = "claude-primary"
kind = "claude_code"
display_name = "Claude Code (Primary)"
binary_path = "/usr/local/bin/claude"
model = "sonnet"
enabled = true
priority = 1
specializations = ["oom_analysis", "resource_optimization", "capacity_planning"]
```

### `config/production.toml` — Production Overrides

Layered on top of `default.toml` for production deployments. Adjusts log levels, poll intervals, and concurrency limits.

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AION_CONFIG` | `config/default.toml` | Path to daemon configuration |
| `AION_AGENTS_CONFIG` | `config/agents.toml` | Path to agent registry configuration |
| `AION_AUDIT_DIR` | `/var/lib/aion/audit` | Directory for audit log storage |
| `RUST_LOG` | `info` | Log level filter (tracing `EnvFilter` syntax) |

## Running

### Local Development

```bash
# Run the AION agent daemon
cargo run -p aion-agent

# Run with custom configuration
AION_CONFIG=config/production.toml cargo run -p aion-agent

# Run the MCP server standalone (for testing)
cargo run -p aion-mcp-server -- --token <invocation-token>
```

### REST API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/status` | Daemon status and agent counts |
| `GET` | `/api/v1/agents` | List registered AI agents |
| `GET` | `/api/v1/audit` | Retrieve audit log entries |
| `GET` | `/api/v1/audit/verify` | Verify audit chain integrity |
| `GET` | `/api/v1/proposals` | List pending remediation proposals |
| `POST` | `/api/v1/proposals/{id}/approve` | Approve a remediation proposal |
| `POST` | `/api/v1/trigger` | Manually trigger anomaly analysis |

## Testing

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p aion-mount
cargo test -p aion-audit
cargo test -p aion-api

# Run with output
cargo test -- --nocapture
```

### E2E Testing

End-to-end tests require a local Kubernetes cluster:

```bash
# Create a test cluster
kind create cluster --name aion-test
# or
k3d cluster create aion-test

# Run E2E tests
cargo test --test '*'
```

### OOM Trigger Tool

A test utility for simulating OOM kills is available in `tools/oom-trigger/`:

```bash
# Build and deploy the OOM trigger pod
kubectl apply -f tools/oom-trigger/oom-trigger-pod.yaml
```

## Deployment

### Docker Images

```bash
# Build the agent daemon image
docker build -f deploy/docker/Dockerfile.agent -t aion/agent:latest .

# Build the MCP server image
docker build -f deploy/docker/Dockerfile.mcp-server -t aion/mcp-server:latest .
```

### Kubernetes

AION deploys as a DaemonSet, running on every node for cluster-wide observability.

```bash
# Apply all manifests
kubectl apply -f deploy/k8s/namespace.yaml
kubectl apply -f deploy/k8s/serviceaccount.yaml
kubectl apply -f deploy/k8s/rbac.yaml
kubectl apply -f deploy/k8s/configmap.yaml
kubectl apply -f deploy/k8s/secrets.yaml
kubectl apply -f deploy/k8s/daemonset.yaml
```

Or apply all at once:

```bash
kubectl apply -f deploy/k8s/
```

### Required Capabilities

The DaemonSet requires the following Linux capabilities for eBPF operation:

| Capability | Purpose |
|------------|---------|
| `BPF` | Loading eBPF programs |
| `SYS_ADMIN` | eBPF map access |
| `PERFMON` | Perf events for eBPF |
| `SYS_RESOURCE` | rlimit for eBPF maps |

### API Keys

AI agent API keys are stored in a Kubernetes Secret:

```bash
kubectl create secret generic aion-api-keys \
  --namespace aion-system \
  --from-literal=ANTHROPIC_API_KEY=<key> \
  --from-literal=OPENAI_API_KEY=<key> \
  --from-literal=GOOGLE_API_KEY=<key>
```

## Project Structure

```
aion/
├── crates/
│   ├── aion-agent/          # Main daemon binary
│   ├── aion-api/            # REST + gRPC API server
│   ├── aion-audit/          # Hash-chain audit logging
│   ├── aion-capability/     # Zero-trust capability tokens
│   ├── aion-common/         # Shared types and configuration
│   ├── aion-ebpf/           # eBPF programs (no_std, built via xtask)
│   ├── aion-ebpf-common/    # Shared eBPF types
│   ├── aion-execute/        # Deterministic K8s executor
│   ├── aion-mcp-server/     # Standalone MCP server binary
│   ├── aion-mount/          # Agent mount system
│   ├── aion-observe/        # Observability collectors
│   ├── aion-propose/        # Proposal types + validation
│   └── aion-validate/       # Policy validation chain
├── config/
│   ├── default.toml         # Default daemon configuration
│   ├── agents.toml          # AI agent registry
│   └── production.toml      # Production overrides
├── deploy/
│   ├── docker/              # Dockerfiles for agent and MCP server
│   └── k8s/                 # Kubernetes manifests
├── proto/                   # Protobuf definitions
├── tools/
│   └── oom-trigger/         # OOM simulation test tool
└── xtask/                   # Build tasks (eBPF, protobuf)
```

## License

Apache-2.0
