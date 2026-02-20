# AION - AI-Native Autonomous Infrastructure OS

## Project Overview
AION is an AI-powered autonomous infrastructure control fabric running on Linux + Kubernetes.
It detects infrastructure anomalies, mounts AI Agents (Claude Code, Codex CLI, Gemini CLI)
as subprocesses, and performs autonomous remediation through MCP (Model Context Protocol).

## Architecture
- **Rust workspace** with multiple crates under `crates/`
- **eBPF** (aya-rs) for kernel-level observability
- **MCP** (rmcp) for AI Agent tool interface
- **gRPC** (tonic) + **REST** (axum) for APIs
- **Agent Mount Model**: AI agents are spawned as subprocesses with MCP stdio transport

## Build Commands
- `cargo build` — Build all crates
- `cargo test` — Run all tests
- `cargo xtask ebpf-build` — Build eBPF programs
- `cargo xtask proto-gen` — Generate protobuf code

## Key Patterns
- **Error handling**: `thiserror` in libraries, `anyhow` in binaries
- **Async runtime**: `tokio` (multi-threaded)
- **Configuration**: TOML files in `config/`, parsed via `config` crate
- **Handle system**: Opaque references (PodHandle, NodeHandle, ProcessHandle) for resource access control
- **Audit logging**: SHA-256 hash chain for tamper detection

## Crate Structure
- `aion-common` — Shared types, errors, config, handles
- `aion-observe` — Observability collectors (eBPF, cgroup, K8s)
- `aion-ebpf` — eBPF programs (kernel space, aya-ebpf)
- `aion-mount` — Agent Mount system (core: registry, MCP, launcher, governor, permission, selector, validator, pipeline)
- `aion-propose` — Proposal types + schema validation
- `aion-validate` — Policy validation chain
- `aion-execute` — Deterministic executor (K8s operations)
- `aion-capability` — Zero-trust capability tokens
- `aion-audit` — Hash-chain audit logging
- `aion-api` — gRPC + REST API server
- `aion-mcp-server` — Standalone MCP server binary (spawned by agents)
- `aion-agent` — Main daemon binary

## Testing
- Unit tests: `cargo test -p <crate-name>`
- Integration tests: `cargo test --test '*'`
- E2E: Requires kind/k3d cluster
