//! Graph diagnostics and execution failures use the shared desktop locale and translations.
use yss_graph_editor::projection::EditorDiagnosticModel;
use yss_ui::text::{format, locale, translate};

pub fn graph_diagnostic(diagnostic: &EditorDiagnosticModel) -> String {
    let fallback = || {
        let code = diagnostic.code.as_ref();
        if code.len() <= 128
            && code.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && code.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.".contains(&byte)
            })
        {
            format("native.text.diagnosticCode", &[("code", code.to_owned())])
        } else {
            translate("native.text.diagnosticFallback")
        }
    };
    yss_graph_diagnostics::render_diagnostic(
        locale(),
        &diagnostic.code,
        &diagnostic.message_key,
        &diagnostic.arguments,
    )
    .unwrap_or_else(fallback)
}

pub fn run_failure_key(code: yss_graph_execution::error::RunFailureCode) -> &'static str {
    use yss_graph_execution::error::RunFailureCode;
    match code {
        RunFailureCode::KernelFailed => "kernelFailed",
        RunFailureCode::KernelNotFound => "kernelNotFound",
        RunFailureCode::InvalidNumericInput => "invalidNumericInput",
        RunFailureCode::ShapeMismatch => "shapeMismatch",
        RunFailureCode::GroupSchemaMismatch => "groupSchemaMismatch",
        RunFailureCode::GroupKeyCollision => "groupKeyCollision",
        RunFailureCode::InvalidParameter => "invalidParameter",
        RunFailureCode::UnalignedSeries => "unalignedSeries",
        RunFailureCode::BudgetExceeded => "budgetExceeded",
        RunFailureCode::InputLayoutMismatch => "inputLayoutMismatch",
        RunFailureCode::OutputContractMismatch => "outputContractMismatch",
        RunFailureCode::ScientificFailure => "scientificFailure",
        RunFailureCode::DivisionByZero => "divisionByZero",
        RunFailureCode::NonFiniteResult => "nonFiniteResult",
        RunFailureCode::DeadlineExceeded => "deadlineExceeded",
        RunFailureCode::ResourceUnavailable => "resourceUnavailable",
        RunFailureCode::InputResultUnavailable => "inputResultUnavailable",
        RunFailureCode::FinalizationFailed => "finalizationFailed",
    }
}

pub fn run_failure(code: yss_graph_execution::error::RunFailureCode) -> String {
    translate(&format!("runFailure.causes.{}", run_failure_key(code)))
}
