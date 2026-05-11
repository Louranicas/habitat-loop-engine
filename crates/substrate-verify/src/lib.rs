#![forbid(unsafe_code)]

// End-to-end stack cross-reference: this source file is the terminal implementation node for M002_SUBSTRATE_VERIFY.md / L04_VERIFICATION.md / L07_RUNBOOK_SEMANTICS.md.
// Keep reciprocal alignment with CLAUDE.local.md -> README.md -> QUICKSTART.md -> Obsidian HOME -> ULTRAMAP.md -> ai_docs/layers -> ai_docs/modules -> this source file while deploying the full codebase stack.

use substrate_types::{Authorization, HleError, Receipt, StepState, Workflow};

/// Verify authorization before runtime execution.
///
/// # Errors
///
/// Returns an error when M0 runtime authorization is false.
pub fn verify_authorization(authorization: Authorization) -> Result<(), HleError> {
    if !authorization.m0_runtime {
        return Err(HleError::new("M0 runtime authorization is false"));
    }
    if authorization.live_integrations {
        return Err(HleError::new(
            "live integrations are not authorized for M0 local runtime",
        ));
    }
    if authorization.cron_daemons {
        return Err(HleError::new(
            "cron daemons are not authorized for M0 local runtime",
        ));
    }
    Ok(())
}

/// Verifier authority gate: executor drafts are not PASS until this function
/// converts state into a receipt verdict.
///
/// # Errors
///
/// Returns an error when the workflow is invalid or the requested step id is
/// not present in the workflow.
pub fn verify_step(
    workflow: &Workflow,
    step_id: &str,
    draft_state: StepState,
) -> Result<Receipt, HleError> {
    workflow.validate()?;
    let step = workflow
        .steps
        .iter()
        .find(|candidate| candidate.id == step_id)
        .ok_or_else(|| HleError::new(format!("unknown workflow step: {step_id}")))?;

    let receipt = if step.requires_human {
        Receipt::new(
            workflow.name.clone(),
            step.id.clone(),
            StepState::AwaitingHuman,
            "AWAITING_HUMAN",
            "step requires human handoff before PASS authority",
        )
    } else if draft_state == step.desired_state && draft_state == StepState::Passed {
        Receipt::new(
            workflow.name.clone(),
            step.id.clone(),
            StepState::Passed,
            "PASS",
            "verifier accepted executor draft state",
        )
    } else if draft_state == StepState::Failed || step.desired_state == StepState::Failed {
        Receipt::new(
            workflow.name.clone(),
            step.id.clone(),
            StepState::Failed,
            "FAIL",
            "verifier observed failed draft or expected failure control",
        )
    } else {
        Receipt::new(
            workflow.name.clone(),
            step.id.clone(),
            StepState::Failed,
            "FAIL",
            "draft state did not match step acceptance state",
        )
    };
    Ok(receipt)
}

/// Verify the aggregate report verdict from step receipts.
///
/// # Errors
///
/// Returns an error when no receipts are supplied.
pub fn verify_report(receipts: &[Receipt]) -> Result<&'static str, HleError> {
    if receipts.is_empty() {
        return Err(HleError::new("no receipts to verify"));
    }
    if receipts
        .iter()
        .any(|receipt| receipt.verifier_verdict != expected_verdict(receipt.state))
    {
        return Ok("FAIL");
    }
    if receipts
        .iter()
        .any(|receipt| receipt.state == StepState::Failed)
        || receipts
            .iter()
            .any(|receipt| !matches!(receipt.state, StepState::Passed | StepState::AwaitingHuman))
    {
        Ok("FAIL")
    } else if receipts
        .iter()
        .any(|receipt| receipt.state == StepState::AwaitingHuman)
    {
        Ok("AWAITING_HUMAN")
    } else {
        Ok("PASS")
    }
}

fn expected_verdict(state: StepState) -> &'static str {
    match state {
        StepState::Passed => "PASS",
        StepState::AwaitingHuman => "AWAITING_HUMAN",
        StepState::Pending | StepState::Running | StepState::Failed | StepState::RolledBack => {
            "FAIL"
        }
    }
}

#[cfg(test)]
mod tests {
    use substrate_types::{Authorization, StepState, Workflow, WorkflowStep};

    use super::{verify_authorization, verify_report, verify_step};

    #[test]
    fn authorization_blocks_when_m0_false() {
        let auth = Authorization {
            m0_runtime: false,
            live_integrations: false,
            cron_daemons: false,
        };
        assert!(verify_authorization(auth).is_err());
    }

    #[test]
    fn authorization_blocks_non_local_runtime_permissions() {
        let live_auth = Authorization {
            m0_runtime: true,
            live_integrations: true,
            cron_daemons: false,
        };
        let daemon_auth = Authorization {
            m0_runtime: true,
            live_integrations: false,
            cron_daemons: true,
        };

        assert!(verify_authorization(live_auth).is_err());
        assert!(verify_authorization(daemon_auth).is_err());
    }

    #[test]
    fn verifier_can_pass_matching_step() {
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new("s1", "pass", StepState::Passed)],
        );
        let receipt = verify_step(&workflow, "s1", StepState::Passed);
        assert!(receipt.is_ok());
        let receipts = receipt.map_or_else(|_| Vec::new(), |value| vec![value]);
        assert_eq!(verify_report(&receipts), Ok("PASS"));
    }

    #[test]
    fn verifier_preserves_awaiting_human() {
        let workflow = Workflow::new("demo", vec![WorkflowStep::awaiting_human("s1", "ask Luke")]);
        let receipt = verify_step(&workflow, "s1", StepState::Passed);
        assert!(receipt.is_ok());
        let receipts = receipt.map_or_else(|_| Vec::new(), |value| vec![value]);
        assert_eq!(verify_report(&receipts), Ok("AWAITING_HUMAN"));
    }

    fn workflow_with_step(state: StepState) -> Workflow {
        Workflow::new("demo", vec![WorkflowStep::new("s1", "step", state)])
    }

    #[test]
    fn authorization_accepts_m0_local() {
        assert!(verify_authorization(Authorization::m0_local()).is_ok());
    }

    #[test]
    fn authorization_rejects_full_deployment_profile() {
        assert!(verify_authorization(Authorization::full_deployment()).is_err());
    }

    #[test]
    fn authorization_error_mentions_m0_when_disabled() {
        let auth = Authorization {
            m0_runtime: false,
            live_integrations: false,
            cron_daemons: false,
        };
        let message =
            verify_authorization(auth).map_or_else(|err| err.to_string(), |()| String::new());
        assert!(message.contains("M0 runtime"));
    }

    #[test]
    fn authorization_error_mentions_live_integrations() {
        let auth = Authorization {
            m0_runtime: true,
            live_integrations: true,
            cron_daemons: false,
        };
        let message =
            verify_authorization(auth).map_or_else(|err| err.to_string(), |()| String::new());
        assert!(message.contains("live integrations"));
    }

    #[test]
    fn authorization_error_mentions_cron_daemons() {
        let auth = Authorization {
            m0_runtime: true,
            live_integrations: false,
            cron_daemons: true,
        };
        let message =
            verify_authorization(auth).map_or_else(|err| err.to_string(), |()| String::new());
        assert!(message.contains("cron daemons"));
    }

    #[test]
    fn verifier_rejects_unknown_step() {
        assert!(verify_step(
            &workflow_with_step(StepState::Passed),
            "missing",
            StepState::Passed
        )
        .is_err());
    }

    #[test]
    fn verifier_rejects_invalid_workflow_before_step_lookup() {
        let workflow = Workflow::new("", vec![WorkflowStep::new("s1", "step", StepState::Passed)]);
        assert!(verify_step(&workflow, "s1", StepState::Passed).is_err());
    }

    #[test]
    fn verifier_fails_pending_draft_for_pass_step() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Pending,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_fails_running_draft_for_pass_step() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Running,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_fails_awaiting_human_draft_for_pass_step() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::AwaitingHuman,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_fails_failed_draft_for_pass_step() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Failed,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_fails_rolled_back_draft_for_pass_step() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::RolledBack,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_pass_receipt_has_pass_verdict() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Passed,
        );
        assert_eq!(
            receipt.map_or(String::new(), |value| value.verifier_verdict),
            "PASS"
        );
    }

    #[test]
    fn verifier_pass_receipt_names_workflow() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Passed,
        );
        assert_eq!(
            receipt.map_or(String::new(), |value| value.workflow),
            "demo"
        );
    }

    #[test]
    fn verifier_pass_receipt_names_step() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Passed,
        );
        assert_eq!(receipt.map_or(String::new(), |value| value.step_id), "s1");
    }

    #[test]
    fn verifier_pass_receipt_message_mentions_acceptance() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Passed,
        );
        assert!(receipt
            .map_or(String::new(), |value| value.message)
            .contains("accepted"));
    }

    #[test]
    fn verifier_failed_expected_control_fails_even_when_draft_passes() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Failed),
            "s1",
            StepState::Passed,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_failed_expected_control_has_fail_verdict() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Failed),
            "s1",
            StepState::Passed,
        );
        assert_eq!(
            receipt.map_or(String::new(), |value| value.verifier_verdict),
            "FAIL"
        );
    }

    #[test]
    fn verifier_awaiting_human_overrides_failed_draft() {
        let workflow = Workflow::new("demo", vec![WorkflowStep::awaiting_human("s1", "ask")]);
        let receipt = verify_step(&workflow, "s1", StepState::Failed);
        assert_eq!(
            receipt.map_or(StepState::Failed, |value| value.state),
            StepState::AwaitingHuman
        );
    }

    #[test]
    fn verifier_awaiting_human_has_awaiting_verdict() {
        let workflow = Workflow::new("demo", vec![WorkflowStep::awaiting_human("s1", "ask")]);
        let receipt = verify_step(&workflow, "s1", StepState::Passed);
        assert_eq!(
            receipt.map_or(String::new(), |value| value.verifier_verdict),
            "AWAITING_HUMAN"
        );
    }

    #[test]
    fn verifier_awaiting_human_message_mentions_handoff() {
        let workflow = Workflow::new("demo", vec![WorkflowStep::awaiting_human("s1", "ask")]);
        let receipt = verify_step(&workflow, "s1", StepState::Passed);
        assert!(receipt
            .map_or(String::new(), |value| value.message)
            .contains("human handoff"));
    }

    #[test]
    fn report_rejects_empty_receipts() {
        assert!(verify_report(&[]).is_err());
    }

    #[test]
    fn report_passes_single_pass_receipt() {
        let receipts = vec![substrate_types::Receipt::new(
            "demo",
            "s1",
            StepState::Passed,
            "PASS",
            "ok",
        )];
        assert_eq!(verify_report(&receipts), Ok("PASS"));
    }

    #[test]
    fn report_fails_single_failed_receipt() {
        let receipts = vec![substrate_types::Receipt::new(
            "demo",
            "s1",
            StepState::Failed,
            "FAIL",
            "bad",
        )];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn report_awaits_single_human_receipt() {
        let receipts = vec![substrate_types::Receipt::new(
            "demo",
            "s1",
            StepState::AwaitingHuman,
            "AWAITING_HUMAN",
            "ask",
        )];
        assert_eq!(verify_report(&receipts), Ok("AWAITING_HUMAN"));
    }

    #[test]
    fn report_failure_precedes_pass() {
        let receipts = vec![
            substrate_types::Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok"),
            substrate_types::Receipt::new("demo", "s2", StepState::Failed, "FAIL", "bad"),
        ];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn report_failure_precedes_awaiting_human() {
        let receipts = vec![
            substrate_types::Receipt::new(
                "demo",
                "s1",
                StepState::AwaitingHuman,
                "AWAITING_HUMAN",
                "ask",
            ),
            substrate_types::Receipt::new("demo", "s2", StepState::Failed, "FAIL", "bad"),
        ];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn report_awaiting_human_precedes_pass() {
        let receipts = vec![
            substrate_types::Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok"),
            substrate_types::Receipt::new(
                "demo",
                "s2",
                StepState::AwaitingHuman,
                "AWAITING_HUMAN",
                "ask",
            ),
        ];
        assert_eq!(verify_report(&receipts), Ok("AWAITING_HUMAN"));
    }

    #[test]
    fn verifier_fails_pending_receipt_state() {
        let receipts = vec![substrate_types::Receipt::new(
            "demo",
            "s1",
            StepState::Pending,
            "PASS",
            "pending",
        )];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn verifier_fails_rolled_back_receipt_state() {
        let receipts = vec![substrate_types::Receipt::new(
            "demo",
            "s1",
            StepState::RolledBack,
            "PASS",
            "rolled back",
        )];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn verifier_does_not_trust_pass_verdict_without_pass_state() {
        let receipts = vec![substrate_types::Receipt::new(
            "demo",
            "s1",
            StepState::Failed,
            "PASS",
            "bad",
        )];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn verifier_fails_pass_state_with_fail_verdict_string() {
        let receipts = vec![substrate_types::Receipt::new(
            "demo",
            "s1",
            StepState::Passed,
            "FAIL",
            "ok",
        )];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn verifier_unknown_step_error_names_step() {
        let message = verify_step(
            &workflow_with_step(StepState::Passed),
            "missing",
            StepState::Passed,
        )
        .map_or_else(|err| err.to_string(), |_| String::new());
        assert!(message.contains("missing"));
    }

    #[test]
    fn verifier_validates_step_with_blank_title() {
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new("s1", " ", StepState::Passed)],
        );
        assert!(verify_step(&workflow, "s1", StepState::Passed).is_err());
    }

    #[test]
    fn verifier_validates_step_with_blank_id() {
        let workflow = Workflow::new(
            "demo",
            vec![WorkflowStep::new(" ", "step", StepState::Passed)],
        );
        assert!(verify_step(&workflow, " ", StepState::Passed).is_err());
    }

    #[test]
    fn verifier_running_desired_state_cannot_pass() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Running),
            "s1",
            StepState::Running,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_pending_desired_state_cannot_pass() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Pending),
            "s1",
            StepState::Pending,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_rolled_back_desired_state_cannot_pass() {
        let receipt = verify_step(
            &workflow_with_step(StepState::RolledBack),
            "s1",
            StepState::RolledBack,
        );
        assert_eq!(
            receipt.map_or(StepState::Passed, |value| value.state),
            StepState::Failed
        );
    }

    #[test]
    fn verifier_failed_draft_message_mentions_failed() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Failed,
        );
        assert!(receipt
            .map_or(String::new(), |value| value.message)
            .contains("failed"));
    }

    #[test]
    fn verifier_mismatch_message_mentions_draft_state() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Pending,
        );
        assert!(receipt
            .map_or(String::new(), |value| value.message)
            .contains("draft state"));
    }

    #[test]
    fn report_error_message_mentions_no_receipts() {
        let message = verify_report(&[]).map_or_else(|err| err.to_string(), |_| String::new());
        assert!(message.contains("no receipts"));
    }

    #[test]
    fn report_passes_multiple_pass_receipts() {
        let receipts = vec![
            substrate_types::Receipt::new("demo", "s1", StepState::Passed, "PASS", "ok"),
            substrate_types::Receipt::new("demo", "s2", StepState::Passed, "PASS", "ok"),
        ];
        assert_eq!(verify_report(&receipts), Ok("PASS"));
    }

    #[test]
    fn report_failure_is_independent_of_receipt_order() {
        let receipts = vec![
            substrate_types::Receipt::new("demo", "s1", StepState::Failed, "FAIL", "bad"),
            substrate_types::Receipt::new("demo", "s2", StepState::Passed, "PASS", "ok"),
        ];
        assert_eq!(verify_report(&receipts), Ok("FAIL"));
    }

    #[test]
    fn report_awaiting_human_is_independent_of_receipt_order() {
        let receipts = vec![
            substrate_types::Receipt::new(
                "demo",
                "s1",
                StepState::AwaitingHuman,
                "AWAITING_HUMAN",
                "ask",
            ),
            substrate_types::Receipt::new("demo", "s2", StepState::Passed, "PASS", "ok"),
        ];
        assert_eq!(verify_report(&receipts), Ok("AWAITING_HUMAN"));
    }

    #[test]
    fn verifier_passes_second_step_by_id() {
        let workflow = Workflow::new(
            "demo",
            vec![
                WorkflowStep::new("s1", "first", StepState::Passed),
                WorkflowStep::new("s2", "second", StepState::Passed),
            ],
        );
        let receipt = verify_step(&workflow, "s2", StepState::Passed);
        assert_eq!(receipt.map_or(String::new(), |value| value.step_id), "s2");
    }

    #[test]
    fn verifier_first_step_failure_does_not_hide_second_step_lookup() {
        let workflow = Workflow::new(
            "demo",
            vec![
                WorkflowStep::new("s1", "first", StepState::Failed),
                WorkflowStep::new("s2", "second", StepState::Passed),
            ],
        );
        let receipt = verify_step(&workflow, "s2", StepState::Passed);
        assert_eq!(
            receipt.map_or(String::new(), |value| value.verifier_verdict),
            "PASS"
        );
    }

    #[test]
    fn verifier_keeps_workflow_name_on_failed_receipt() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Failed,
        );
        assert_eq!(
            receipt.map_or(String::new(), |value| value.workflow),
            "demo"
        );
    }

    #[test]
    fn verifier_keeps_step_id_on_failed_receipt() {
        let receipt = verify_step(
            &workflow_with_step(StepState::Passed),
            "s1",
            StepState::Failed,
        );
        assert_eq!(receipt.map_or(String::new(), |value| value.step_id), "s1");
    }

    #[test]
    fn verifier_keeps_workflow_name_on_awaiting_receipt() {
        let workflow = Workflow::new("demo", vec![WorkflowStep::awaiting_human("s1", "ask")]);
        let receipt = verify_step(&workflow, "s1", StepState::Passed);
        assert_eq!(
            receipt.map_or(String::new(), |value| value.workflow),
            "demo"
        );
    }

    #[test]
    fn verifier_keeps_step_id_on_awaiting_receipt() {
        let workflow = Workflow::new("demo", vec![WorkflowStep::awaiting_human("s1", "ask")]);
        let receipt = verify_step(&workflow, "s1", StepState::Passed);
        assert_eq!(receipt.map_or(String::new(), |value| value.step_id), "s1");
    }
}
