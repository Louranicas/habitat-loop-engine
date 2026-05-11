#![forbid(unsafe_code)]

// End-to-end stack cross-reference: this source file is the terminal implementation node for M001_SUBSTRATE_TYPES.md / L01_FOUNDATION.md.
// Keep reciprocal alignment with CLAUDE.local.md -> README.md -> QUICKSTART.md -> Obsidian HOME -> ULTRAMAP.md -> ai_docs/layers -> ai_docs/modules -> this source file while deploying the full codebase stack.

use std::fmt;
use std::str::FromStr;

/// Repository lifecycle stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectStatus {
    ScaffoldOnly,
    M0Runtime,
    LiveIntegrated,
    Deployed,
}

impl ProjectStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScaffoldOnly => "scaffold-only",
            Self::M0Runtime => "m0-runtime",
            Self::LiveIntegrated => "live-integrated",
            Self::Deployed => "deployed",
        }
    }
}

/// Authorization flags derived from plan.toml and status receipts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Authorization {
    pub m0_runtime: bool,
    pub live_integrations: bool,
    pub cron_daemons: bool,
}

impl Authorization {
    #[must_use]
    pub const fn m0_local() -> Self {
        Self {
            m0_runtime: true,
            live_integrations: false,
            cron_daemons: false,
        }
    }

    #[must_use]
    pub const fn full_deployment() -> Self {
        Self {
            m0_runtime: true,
            live_integrations: true,
            cron_daemons: true,
        }
    }
}

/// Workflow step state. The verifier is the only authority allowed to produce
/// Passed or Failed verdicts from a draft executor receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepState {
    Pending,
    Running,
    AwaitingHuman,
    Passed,
    Failed,
    RolledBack,
}

impl StepState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::AwaitingHuman => "awaiting-human",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::RolledBack => "rolled-back",
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Passed | Self::Failed | Self::RolledBack)
    }
}

impl fmt::Display for StepState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StepState {
    type Err = HleError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "awaiting-human" => Ok(Self::AwaitingHuman),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "rolled-back" => Ok(Self::RolledBack),
            other => Err(HleError::new(format!("unknown step state: {other}"))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowStep {
    pub id: String,
    pub title: String,
    pub desired_state: StepState,
    pub requires_human: bool,
}

impl WorkflowStep {
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>, desired_state: StepState) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            desired_state,
            requires_human: false,
        }
    }

    #[must_use]
    pub fn awaiting_human(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            desired_state: StepState::AwaitingHuman,
            requires_human: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<WorkflowStep>,
}

impl Workflow {
    #[must_use]
    pub fn new(name: impl Into<String>, steps: Vec<WorkflowStep>) -> Self {
        Self {
            name: name.into(),
            steps,
        }
    }

    /// Validate local workflow invariants before execution or verification.
    ///
    /// # Errors
    ///
    /// Returns an error when the workflow name is empty, the workflow has no
    /// steps, or any step is missing a non-empty id or title.
    pub fn validate(&self) -> Result<(), HleError> {
        if self.name.trim().is_empty() {
            return Err(HleError::new("workflow name cannot be empty"));
        }
        if self.steps.is_empty() {
            return Err(HleError::new("workflow must contain at least one step"));
        }
        for step in &self.steps {
            if step.id.trim().is_empty() {
                return Err(HleError::new("workflow step id cannot be empty"));
            }
            if step.title.trim().is_empty() {
                return Err(HleError::new(format!(
                    "workflow step {} has empty title",
                    step.id
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    pub workflow: String,
    pub step_id: String,
    pub state: StepState,
    pub verifier_verdict: String,
    pub message: String,
}

impl Receipt {
    #[must_use]
    pub fn new(
        workflow: impl Into<String>,
        step_id: impl Into<String>,
        state: StepState,
        verifier_verdict: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            workflow: workflow.into(),
            step_id: step_id.into(),
            state,
            verifier_verdict: verifier_verdict.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionReport {
    pub workflow: String,
    pub receipts: Vec<Receipt>,
}

impl ExecutionReport {
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        if self
            .receipts
            .iter()
            .any(|receipt| receipt.verifier_verdict != expected_receipt_verdict(receipt.state))
        {
            return "FAIL";
        }
        if self
            .receipts
            .iter()
            .any(|receipt| receipt.state == StepState::Failed)
            || self.receipts.iter().any(|receipt| {
                !matches!(receipt.state, StepState::Passed | StepState::AwaitingHuman)
            })
        {
            "FAIL"
        } else if self
            .receipts
            .iter()
            .any(|receipt| receipt.state == StepState::AwaitingHuman)
        {
            "AWAITING_HUMAN"
        } else {
            "PASS"
        }
    }
}

fn expected_receipt_verdict(state: StepState) -> &'static str {
    match state {
        StepState::Passed => "PASS",
        StepState::AwaitingHuman => "AWAITING_HUMAN",
        StepState::Pending | StepState::Running | StepState::Failed | StepState::RolledBack => {
            "FAIL"
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HleError {
    message: String,
}

impl HleError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for HleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HleError {}

#[cfg(test)]
mod tests {
    use super::{ExecutionReport, Receipt, StepState, Workflow, WorkflowStep};

    #[test]
    fn execution_report_fails_when_any_step_fails() {
        let report = ExecutionReport {
            workflow: String::from("demo"),
            receipts: vec![Receipt::new("demo", "s1", StepState::Failed, "FAIL", "x")],
        };
        assert_eq!(report.verdict(), "FAIL");
    }

    #[test]
    fn execution_report_fails_when_any_receipt_is_pending() {
        let report = ExecutionReport {
            workflow: String::from("demo"),
            receipts: vec![Receipt::new(
                "demo",
                "s1",
                StepState::Pending,
                "PENDING",
                "x",
            )],
        };
        assert_eq!(report.verdict(), "FAIL");
    }

    #[test]
    fn execution_report_fails_when_any_receipt_is_rolled_back() {
        let report = ExecutionReport {
            workflow: String::from("demo"),
            receipts: vec![Receipt::new(
                "demo",
                "s1",
                StepState::RolledBack,
                "ROLLBACK",
                "x",
            )],
        };
        assert_eq!(report.verdict(), "FAIL");
    }

    #[test]
    fn workflow_rejects_empty_steps() {
        let workflow = Workflow::new("demo", Vec::<WorkflowStep>::new());
        assert!(workflow.validate().is_err());
    }

    #[test]
    fn project_status_scaffold_string_is_stable() {
        assert_eq!(super::ProjectStatus::ScaffoldOnly.as_str(), "scaffold-only");
    }

    #[test]
    fn project_status_m0_string_is_stable() {
        assert_eq!(super::ProjectStatus::M0Runtime.as_str(), "m0-runtime");
    }

    #[test]
    fn project_status_live_string_is_stable() {
        assert_eq!(
            super::ProjectStatus::LiveIntegrated.as_str(),
            "live-integrated"
        );
    }

    #[test]
    fn project_status_deployed_string_is_stable() {
        assert_eq!(super::ProjectStatus::Deployed.as_str(), "deployed");
    }

    #[test]
    fn m0_authorization_enables_m0_runtime() {
        assert!(super::Authorization::m0_local().m0_runtime);
    }

    #[test]
    fn m0_authorization_disables_live_integrations() {
        assert!(!super::Authorization::m0_local().live_integrations);
    }

    #[test]
    fn m0_authorization_disables_cron_daemons() {
        assert!(!super::Authorization::m0_local().cron_daemons);
    }

    #[test]
    fn full_deployment_authorization_enables_m0_runtime() {
        assert!(super::Authorization::full_deployment().m0_runtime);
    }

    #[test]
    fn full_deployment_authorization_enables_live_integrations() {
        assert!(super::Authorization::full_deployment().live_integrations);
    }

    #[test]
    fn full_deployment_authorization_enables_cron_daemons() {
        assert!(super::Authorization::full_deployment().cron_daemons);
    }

    #[test]
    fn pending_state_string_is_stable() {
        assert_eq!(StepState::Pending.as_str(), "pending");
    }

    #[test]
    fn running_state_string_is_stable() {
        assert_eq!(StepState::Running.as_str(), "running");
    }

    #[test]
    fn awaiting_human_state_string_is_stable() {
        assert_eq!(StepState::AwaitingHuman.as_str(), "awaiting-human");
    }

    #[test]
    fn passed_state_string_is_stable() {
        assert_eq!(StepState::Passed.as_str(), "passed");
    }

    #[test]
    fn failed_state_string_is_stable() {
        assert_eq!(StepState::Failed.as_str(), "failed");
    }

    #[test]
    fn rolled_back_state_string_is_stable() {
        assert_eq!(StepState::RolledBack.as_str(), "rolled-back");
    }

    #[test]
    fn pending_state_is_not_terminal() {
        assert!(!StepState::Pending.is_terminal());
    }

    #[test]
    fn running_state_is_not_terminal() {
        assert!(!StepState::Running.is_terminal());
    }

    #[test]
    fn awaiting_human_state_is_not_terminal() {
        assert!(!StepState::AwaitingHuman.is_terminal());
    }

    #[test]
    fn passed_state_is_terminal() {
        assert!(StepState::Passed.is_terminal());
    }

    #[test]
    fn failed_state_is_terminal() {
        assert!(StepState::Failed.is_terminal());
    }

    #[test]
    fn rolled_back_state_is_terminal() {
        assert!(StepState::RolledBack.is_terminal());
    }

    #[test]
    fn parses_pending_state() {
        assert_eq!("pending".parse::<StepState>(), Ok(StepState::Pending));
    }

    #[test]
    fn parses_running_state() {
        assert_eq!("running".parse::<StepState>(), Ok(StepState::Running));
    }

    #[test]
    fn parses_awaiting_human_state() {
        assert_eq!(
            "awaiting-human".parse::<StepState>(),
            Ok(StepState::AwaitingHuman)
        );
    }

    #[test]
    fn parses_passed_state() {
        assert_eq!("passed".parse::<StepState>(), Ok(StepState::Passed));
    }

    #[test]
    fn parses_failed_state() {
        assert_eq!("failed".parse::<StepState>(), Ok(StepState::Failed));
    }

    #[test]
    fn parses_rolled_back_state() {
        assert_eq!(
            "rolled-back".parse::<StepState>(),
            Ok(StepState::RolledBack)
        );
    }

    #[test]
    fn rejects_unknown_state() {
        assert!("unknown".parse::<StepState>().is_err());
    }

    #[test]
    fn displays_passed_state_as_wire_value() {
        assert_eq!(StepState::Passed.to_string(), "passed");
    }

    #[test]
    fn workflow_step_new_keeps_id() {
        assert_eq!(WorkflowStep::new("s1", "title", StepState::Passed).id, "s1");
    }

    #[test]
    fn workflow_step_new_keeps_title() {
        assert_eq!(
            WorkflowStep::new("s1", "title", StepState::Passed).title,
            "title"
        );
    }

    #[test]
    fn workflow_step_new_does_not_require_human() {
        assert!(!WorkflowStep::new("s1", "title", StepState::Passed).requires_human);
    }

    #[test]
    fn awaiting_human_step_requires_human() {
        assert!(WorkflowStep::awaiting_human("s1", "title").requires_human);
    }

    #[test]
    fn awaiting_human_step_desires_awaiting_human_state() {
        assert_eq!(
            WorkflowStep::awaiting_human("s1", "title").desired_state,
            StepState::AwaitingHuman
        );
    }

    #[test]
    fn workflow_accepts_valid_single_step() {
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new("s1", "title", StepState::Passed)],
        );
        assert!(workflow.validate().is_ok());
    }

    #[test]
    fn workflow_rejects_blank_name() {
        let workflow = Workflow::new(
            "  ",
            vec![WorkflowStep::new("s1", "title", StepState::Passed)],
        );
        assert!(workflow.validate().is_err());
    }

    #[test]
    fn workflow_rejects_blank_step_id() {
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new(" ", "title", StepState::Passed)],
        );
        assert!(workflow.validate().is_err());
    }

    #[test]
    fn workflow_rejects_blank_step_title() {
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new("s1", " ", StepState::Passed)],
        );
        assert!(workflow.validate().is_err());
    }

    #[test]
    fn receipt_new_keeps_workflow() {
        assert_eq!(
            Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok").workflow,
            "demo"
        );
    }

    #[test]
    fn receipt_new_keeps_step_id() {
        assert_eq!(
            Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok").step_id,
            "s1"
        );
    }

    #[test]
    fn receipt_new_keeps_state() {
        assert_eq!(
            Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok").state,
            StepState::Passed
        );
    }

    #[test]
    fn receipt_new_keeps_verdict() {
        assert_eq!(
            Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok").verifier_verdict,
            "PASS"
        );
    }

    #[test]
    fn receipt_new_keeps_message() {
        assert_eq!(
            Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok").message,
            "ok"
        );
    }

    #[test]
    fn execution_report_passes_when_all_receipts_pass() {
        let report = ExecutionReport {
            workflow: String::from("demo"),
            receipts: vec![Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok")],
        };
        assert_eq!(report.verdict(), "PASS");
    }

    #[test]
    fn execution_report_fails_when_verdict_disagrees_with_state() {
        let report = ExecutionReport {
            workflow: String::from("demo"),
            receipts: vec![Receipt::new(
                "demo",
                "s1",
                StepState::Passed,
                "FAIL",
                "tampered",
            )],
        };
        assert_eq!(report.verdict(), "FAIL");
    }

    #[test]
    fn execution_report_awaits_human_when_any_receipt_awaits_human() {
        let report = ExecutionReport {
            workflow: String::from("demo"),
            receipts: vec![Receipt::new(
                "demo",
                "s1",
                StepState::AwaitingHuman,
                "AWAITING_HUMAN",
                "ask",
            )],
        };
        assert_eq!(report.verdict(), "AWAITING_HUMAN");
    }

    #[test]
    fn execution_report_failure_takes_precedence_over_awaiting_human() {
        let report = ExecutionReport {
            workflow: String::from("demo"),
            receipts: vec![
                Receipt::new(
                    "demo",
                    "s1",
                    StepState::AwaitingHuman,
                    "AWAITING_HUMAN",
                    "ask",
                ),
                Receipt::new("demo", "s2", StepState::Failed, "FAIL", "no"),
            ],
        };
        assert_eq!(report.verdict(), "FAIL");
    }

    #[test]
    fn hle_error_display_returns_message() {
        assert_eq!(super::HleError::new("boom").to_string(), "boom");
    }

    #[test]
    fn workflow_new_keeps_name() {
        assert_eq!(
            Workflow::new(
                "demo",
                vec![WorkflowStep::new("s1", "title", StepState::Passed)]
            )
            .name,
            "demo"
        );
    }

    #[test]
    fn workflow_new_keeps_steps() {
        assert_eq!(
            Workflow::new(
                "demo",
                vec![WorkflowStep::new("s1", "title", StepState::Passed)]
            )
            .steps
            .len(),
            1
        );
    }
}
