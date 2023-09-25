#[must_use = "must return need_wake back to interrupt callback"]
pub struct IsrResult<T, E> {
    pub result: Result<T, E>,
    pub need_wake: bool,
}

impl<E> Default for IsrResult<(), E> {
    fn default() -> Self {
        IsrResult::ok((), false)
    }
}

impl<T, E> IsrResult<T, E> {
    pub fn ok(value: T, need_wake: bool) -> Self {
        IsrResult { result: Ok(value), need_wake }
    }

    pub fn err(err: E, need_wake: bool) -> Self {
        IsrResult { result: Err(err), need_wake }
    }

    #[allow(unused)]
    pub fn map<U>(self, func: impl FnOnce(T) -> U) -> IsrResult<U, E> {
        IsrResult {
            result: self.result.map(func),
            need_wake: self.need_wake,
        }
    }

    pub fn chain<U, F>(self, other: IsrResult<U, F>) -> IsrResult<U, F> {
        IsrResult {
            result: other.result,
            need_wake: self.need_wake || other.need_wake,
        }
    }
}
