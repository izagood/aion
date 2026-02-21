use colored::Colorize;
use serde::Serialize;

use crate::types::*;

pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn print_status(s: &StatusResponse) {
    let indicator = if s.status == "running" {
        "●".green()
    } else {
        "●".red()
    };
    println!("{} AION {}", indicator, s.status.to_uppercase().bold());
    println!("  {} {}", "Version:".dimmed(), s.version);
    println!("  {} {}s", "Uptime:".dimmed(), s.uptime_secs);
    println!(
        "  {} {} registered, {} enabled",
        "Agents:".dimmed(),
        s.agents_registered,
        s.agents_enabled
    );
}

pub fn print_agents(agents: &[AgentResponse]) {
    if agents.is_empty() {
        println!("{}", "No agents registered.".dimmed());
        return;
    }
    for a in agents {
        let status = if a.enabled {
            "enabled".green()
        } else {
            "disabled".red()
        };
        println!(
            "  {} [{}] {} ({})",
            a.display_name.bold(),
            status,
            a.id.dimmed(),
            a.kind,
        );
        println!(
            "    {} model={}, priority={}",
            "Config:".dimmed(),
            a.model,
            a.priority
        );
        if !a.specializations.is_empty() {
            println!(
                "    {} {}",
                "Specializations:".dimmed(),
                a.specializations.join(", ")
            );
        }
        if !a.capabilities.is_empty() {
            println!(
                "    {} {}",
                "Capabilities:".dimmed(),
                a.capabilities.join(", ")
            );
        }
    }
}

pub fn print_audit_log(entries: &[AuditEntryResponse]) {
    if entries.is_empty() {
        println!("{}", "No audit entries.".dimmed());
        return;
    }
    for e in entries {
        println!(
            "  {} #{} [{}] {}",
            e.timestamp.dimmed(),
            e.sequence.to_string().cyan(),
            e.action.yellow(),
            e.description,
        );
        println!(
            "    {} {}/{}",
            "Actor:".dimmed(),
            e.actor_type,
            e.actor_id,
        );
        if let Some(ref aid) = e.anomaly_id {
            print!("    {} {}", "Anomaly:".dimmed(), aid);
        }
        if let Some(ref pid) = e.proposal_id {
            print!("  {} {}", "Proposal:".dimmed(), pid);
        }
        if e.anomaly_id.is_some() || e.proposal_id.is_some() {
            println!();
        }
    }
}

pub fn print_integrity(result: &IntegrityResult) {
    if result.is_valid {
        println!(
            "{} Audit log integrity verified ({} entries)",
            "✓".green().bold(),
            result.entries_verified,
        );
    } else {
        println!(
            "{} Audit log integrity FAILED",
            "✗".red().bold(),
        );
        println!(
            "  {} {}",
            "Entries verified:".dimmed(),
            result.entries_verified
        );
        if let Some(seq) = result.first_broken_sequence {
            println!("  {} sequence #{}", "Broken at:".dimmed(), seq);
        }
        if let Some(ref err) = result.error {
            println!("  {} {}", "Error:".dimmed(), err);
        }
    }
}

pub fn print_trigger(result: &TriggerResponse) {
    if result.accepted {
        println!("{} {}", "✓".green().bold(), result.message);
        if let Some(ref id) = result.anomaly_id {
            println!("  {} {}", "Anomaly ID:".dimmed(), id);
        }
    } else {
        println!("{} {}", "✗".red().bold(), result.message);
    }
}

pub fn print_approve(result: &ApproveResponse) {
    if result.approved {
        println!(
            "{} Proposal {} approved",
            "✓".green().bold(),
            result.proposal_id.bold(),
        );
    } else {
        println!(
            "{} Proposal {} not approved",
            "✗".red().bold(),
            result.proposal_id.bold(),
        );
    }
    println!("  {}", result.message);
}

pub fn print_proposals(proposals: &[serde_json::Value]) {
    if proposals.is_empty() {
        println!("{}", "No proposals.".dimmed());
        return;
    }
    for p in proposals {
        println!("{}", serde_json::to_string_pretty(p).unwrap_or_default());
    }
}
