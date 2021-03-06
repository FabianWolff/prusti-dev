// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rustc_span::MultiSpan;
use prusti_interface::environment::Environment;
use prusti_common::config;

/// The Prusti message that will be reported to the user.
///
/// A Prusti message can originate from:
/// * an encoding error (see the `EncodingError` type)
/// * a Viper verification error
///
/// A `PrustiError` can be displayed as a *warning* to the user. (We should rename `PrustiError`,
/// `EncodingError` and similar types to something less confusing.)
#[derive(Clone, Debug)]
pub struct PrustiError {
    is_error: bool,
    message: String,
    span: MultiSpan,
    help: Option<String>,
    note: Option<(String, MultiSpan)>,
}

impl PrustiError {
    /// Private constructor. Use one of the following methods.
    fn new(message: String, span: MultiSpan) -> Self {
        PrustiError {
            is_error: true,
            message,
            span,
            help: None,
            note: None,
        }
    }

    /// Report a verification error of the verified Rust code
    pub fn verification<S: ToString>(message: S, span: MultiSpan) -> Self {
        check_message(message.to_string());
        PrustiError::new(
            format!("[Prusti: verification error] {}", message.to_string()),
            span
        )
    }

    /// Report an unsupported feature of the verified Rust code (e.g. dereferencing raw pointers)
    pub fn unsupported<S: ToString>(message: S, span: MultiSpan) -> Self {
        check_message(message.to_string());
        let mut error = PrustiError::new(
            format!("[Prusti: unsupported feature] {}", message.to_string()),
            span
        );
        if !config::error_on_partially_supported() && config::skip_unsupported_functions() {
            error.set_warning();
        }
        error
    }

    /// Report an incorrect usage of Prusti (e.g. call an impure function in a contract)
    pub fn incorrect<S: ToString>(message: S, span: MultiSpan) -> Self {
        check_message(message.to_string());
        PrustiError::new(
            format!("[Prusti: invalid specification] {}", message.to_string()),
            span
        )
    }

    /// Report an internal error of Prusti (e.g. failure of the fold-unfold)
    pub fn internal<S: ToString>(message: S, span: MultiSpan) -> Self {
        check_message(message.to_string());
        PrustiError::new(
            format!("[Prusti internal error] {}", message.to_string()),
            span
        )
    }

    /// Set that this Prusti error should be reported as a warning to the user
    pub fn set_warning(&mut self) {
        self.is_error = false;
    }

    pub fn is_error(&self) -> bool {
        self.is_error
    }

    pub fn set_help<S: ToString>(mut self, message: S) -> Self {
        self.help = Some(message.to_string());
        self
    }

    /// Report the encoding error using the compiler's interface
    pub fn emit(self, env: &Environment) {
        if self.is_error {
            env.span_err_with_help_and_note(
                self.span,
                &self.message,
                &self.help,
                &self.note,
            );
        } else {
            env.span_warn_with_help_and_note(
                self.span,
                &self.message,
                &self.help,
                &self.note,
            );
        }
    }

    /// Set the span of the failing assertion expression.
    ///
    /// Note: this is a noop if `opt_span` is None
    pub fn set_failing_assertion(mut self, opt_span: Option<&MultiSpan>) -> Self {
        if let Some(span) = opt_span {
            self.note = Some(("the failing assertion is here".to_string(), span.clone()));
        }
        self
    }

    /// Convert the original error span to a note, and add a new error span.
    ///
    /// Note: this is a noop if `opt_span` is None
    pub fn push_primary_span(mut self, opt_span: Option<&MultiSpan>) -> Self {
        if let Some(span) = opt_span {
            self.note = Some(("the error originates here".to_string(), self.span));
            self.span = span.clone();
        }
        self
    }
}

fn check_message(message: String) {
    debug_assert!(
        message.len() >= 3,
        "Message {:?} is too short",
        message
    );
    debug_assert!(
        message.get(0..1).unwrap() == message.get(0..1).unwrap().to_lowercase(),
        "Message {:?} should start with a lowercase character",
        message
    );
}
