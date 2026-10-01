use crate::value::Value;

/// An error produced by the interpreter.
///
/// Most variants are internal control-flow signals (`Return`, `Break`,
/// `Continue`, `Suspend`, `SyncYieldLimit`) that `Context::eval` and
/// friends normalize before returning; the ones you will actually observe
/// from the public API are [`InterpError::Runtime`] (a Dart-catchable
/// runtime error, e.g. a permission denial) and [`InterpError::SourceCode`]
/// (parse errors). `InterpError::Throw(Value)` carries an uncaught Dart
/// exception value.
#[derive(Debug, Clone)]
pub enum InterpError {
    /// A runtime error, raised by the interpreter or by native code.
    Runtime(String),
    /// A source-code (parse) error.
    SourceCode(String),
    /// Internal: `return` statement unwinding.
    Return(Value),
    /// Internal: labeled `break` unwinding.
    Break(Option<String>),
    /// Internal: labeled `continue` unwinding.
    Continue(Option<String>),
    /// An uncaught Dart exception (the thrown value).
    Throw(Value),
    /// A pattern-matching failure.
    PatternMatch(String),
    /// Internal: sync-generator yield-limit guard hit.
    SyncYieldLimit,
    /// A feature the interpreter does not implement yet.
    Unimplemented(String),
    /// Internal: suspension attempted outside an async context.
    AsyncSuspensionOutsideAsync,
    /// Internal: async suspension signal (used by `await`).
    Suspend,
}

impl InterpError {
    pub fn runtime(msg: impl Into<String>) -> Self {
        InterpError::Runtime(msg.into())
    }

    pub fn unimplemented(msg: impl Into<String>) -> Self {
        InterpError::Unimplemented(msg.into())
    }

    pub fn throw_argument_error(message: impl Into<String>, invalid_value: Value) -> Self {
        InterpError::Throw(crate::stdlib::helpers::wrap(
            &crate::stdlib::helpers::class("ArgumentError"),
            Value::List(std::rc::Rc::new(std::cell::RefCell::new(vec![
                Value::str(message),
                invalid_value,
            ]))),
        ))
    }

    pub fn throw_range_error(message: impl Into<String>, invalid_value: Value) -> Self {
        InterpError::Throw(crate::stdlib::helpers::wrap(
            &crate::stdlib::helpers::class("RangeError"),
            Value::List(std::rc::Rc::new(std::cell::RefCell::new(vec![
                Value::str(message),
                invalid_value,
            ]))),
        ))
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            InterpError::Runtime(m) | InterpError::SourceCode(m) | InterpError::PatternMatch(m) => {
                Some(m)
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for InterpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpError::Runtime(m) => write!(f, "Runtime Error: {m}"),
            InterpError::SourceCode(m) => write!(f, "SourceCodeException: {m}"),
            InterpError::Return(_) => write!(f, "ReturnException"),
            InterpError::Break(l) => write!(f, "BreakException(label: {l:?})"),
            InterpError::Continue(l) => write!(f, "ContinueException(label: {l:?})"),
            InterpError::Throw(_) => write!(f, "InternalInterpreterException"),
            InterpError::PatternMatch(m) => write!(f, "PatternMatchException: {m}"),
            InterpError::SyncYieldLimit => write!(f, "SyncGeneratorYieldLimitReached"),
            InterpError::Unimplemented(m) => write!(f, "UnimplementedError: {m}"),
            InterpError::AsyncSuspensionOutsideAsync => {
                write!(f, "StateError: suspension outside async context")
            }
            InterpError::Suspend => write!(f, "InternalAsyncSuspension"),
        }
    }
}

impl std::error::Error for InterpError {}

impl From<InterpError> for crate::value::Value {
    fn from(_: InterpError) -> Self {
        unreachable!()
    }
}

pub fn is_control_flow(err: &InterpError) -> bool {
    matches!(
        err,
        InterpError::Return(_)
            | InterpError::Break(_)
            | InterpError::Continue(_)
            | InterpError::SyncYieldLimit
    )
}
