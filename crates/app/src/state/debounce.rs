//! A timer that fires once calls stop coming.

use std::time::Duration;

use leptos::prelude::*;

/// Runs the latest of a burst of calls once the burst has paused.
#[derive(Clone, Copy, Default)]
pub struct Debounce(StoredValue<u64>);

impl Debounce {
    /// Runs `action` after `delay`, unless `call` or `cancel` happens first.
    pub fn call(&self, delay: Duration, action: impl FnOnce() + 'static) {
        let pending = self.bump();
        let this = *self;
        set_timeout(
            move || {
                if this.0.get_value() == pending {
                    action();
                }
            },
            delay,
        );
    }

    pub fn cancel(&self) {
        self.bump();
    }

    fn bump(&self) -> u64 {
        self.0.update_value(|generation| *generation += 1);
        self.0.get_value()
    }
}
