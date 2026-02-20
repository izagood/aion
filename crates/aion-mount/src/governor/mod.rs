use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};


/// Budget tracking for agent invocations.
///
/// Enforces: daily budget, per-hour invocation limits, max concurrent invocations.
pub struct BudgetGovernor {
    state: Arc<RwLock<BudgetState>>,
    daily_budget_usd: f64,
}

#[derive(Debug)]
struct BudgetState {
    daily_spent_usd: f64,
    daily_reset_at: Instant,
    agent_budgets: HashMap<String, AgentBudgetState>,
}

#[derive(Debug)]
struct AgentBudgetState {
    invocations_this_hour: u32,
    hour_start: Instant,
    current_concurrent: u32,
    max_invocations_per_hour: u32,
    max_concurrent: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    #[error("daily budget exceeded: spent ${spent:.2} of ${limit:.2}")]
    DailyBudgetExceeded { spent: f64, limit: f64 },

    #[error("hourly invocation limit reached for agent kind '{kind}': {count}/{limit}")]
    HourlyLimitReached { kind: String, count: u32, limit: u32 },

    #[error("concurrent limit reached for agent kind '{kind}': {count}/{limit}")]
    ConcurrentLimitReached { kind: String, count: u32, limit: u32 },
}

impl BudgetGovernor {
    pub fn new(daily_budget_usd: f64) -> Self {
        Self {
            daily_budget_usd,
            state: Arc::new(RwLock::new(BudgetState {
                daily_spent_usd: 0.0,
                daily_reset_at: Instant::now(),
                agent_budgets: HashMap::new(),
            })),
        }
    }

    /// Register budget limits for an agent kind.
    pub fn register_kind(
        &self,
        kind: &str,
        max_invocations_per_hour: u32,
        max_concurrent: u32,
    ) {
        let mut state = self.state.write().unwrap();
        state.agent_budgets.insert(
            kind.to_string(),
            AgentBudgetState {
                invocations_this_hour: 0,
                hour_start: Instant::now(),
                current_concurrent: 0,
                max_invocations_per_hour,
                max_concurrent,
            },
        );
    }

    /// Check if an invocation is allowed and reserve a slot.
    pub fn try_acquire(
        &self,
        agent_kind: &str,
        estimated_cost_usd: f64,
    ) -> Result<BudgetGuard, BudgetError> {
        let mut state = self.state.write().unwrap();

        // Reset daily counter if 24 hours have passed
        if state.daily_reset_at.elapsed() > Duration::from_secs(86400) {
            state.daily_spent_usd = 0.0;
            state.daily_reset_at = Instant::now();
        }
        if state.daily_spent_usd + estimated_cost_usd > self.daily_budget_usd {
            return Err(BudgetError::DailyBudgetExceeded {
                spent: state.daily_spent_usd,
                limit: self.daily_budget_usd,
            });
        }

        // Check agent-specific limits
        if let Some(agent_state) = state.agent_budgets.get_mut(agent_kind) {
            // Reset hourly counter
            if agent_state.hour_start.elapsed() > Duration::from_secs(3600) {
                agent_state.invocations_this_hour = 0;
                agent_state.hour_start = Instant::now();
            }

            if agent_state.invocations_this_hour >= agent_state.max_invocations_per_hour {
                return Err(BudgetError::HourlyLimitReached {
                    kind: agent_kind.to_string(),
                    count: agent_state.invocations_this_hour,
                    limit: agent_state.max_invocations_per_hour,
                });
            }

            if agent_state.current_concurrent >= agent_state.max_concurrent {
                return Err(BudgetError::ConcurrentLimitReached {
                    kind: agent_kind.to_string(),
                    count: agent_state.current_concurrent,
                    limit: agent_state.max_concurrent,
                });
            }

            agent_state.invocations_this_hour += 1;
            agent_state.current_concurrent += 1;
        }

        state.daily_spent_usd += estimated_cost_usd;

        Ok(BudgetGuard {
            state: self.state.clone(),
            agent_kind: agent_kind.to_string(),
        })
    }

    pub fn daily_remaining(&self) -> f64 {
        let state = self.state.read().unwrap();
        (self.daily_budget_usd - state.daily_spent_usd).max(0.0)
    }
}

/// RAII guard that releases the concurrent slot when dropped.
pub struct BudgetGuard {
    state: Arc<RwLock<BudgetState>>,
    agent_kind: String,
}

impl Drop for BudgetGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.write() {
            if let Some(agent_state) = state.agent_budgets.get_mut(&self.agent_kind) {
                agent_state.current_concurrent =
                    agent_state.current_concurrent.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_acquire_and_release() {
        let governor = BudgetGovernor::new(10.0);
        governor.register_kind("claude_code", 20, 3);

        let guard = governor.try_acquire("claude_code", 0.50);
        assert!(guard.is_ok());

        assert!((governor.daily_remaining() - 9.50).abs() < 0.01);
    }

    #[test]
    fn test_daily_budget_exceeded() {
        let governor = BudgetGovernor::new(1.0);
        governor.register_kind("claude_code", 100, 10);

        let _g1 = governor.try_acquire("claude_code", 0.60).unwrap();
        let _g2 = governor.try_acquire("claude_code", 0.30).unwrap();
        let result = governor.try_acquire("claude_code", 0.20);
        assert!(matches!(result, Err(BudgetError::DailyBudgetExceeded { .. })));
    }

    #[test]
    fn test_concurrent_limit() {
        let governor = BudgetGovernor::new(100.0);
        governor.register_kind("claude_code", 100, 2);

        let _g1 = governor.try_acquire("claude_code", 0.10).unwrap();
        let _g2 = governor.try_acquire("claude_code", 0.10).unwrap();
        let result = governor.try_acquire("claude_code", 0.10);
        assert!(matches!(result, Err(BudgetError::ConcurrentLimitReached { .. })));

        drop(_g1);
        let result = governor.try_acquire("claude_code", 0.10);
        assert!(result.is_ok());
    }
}
