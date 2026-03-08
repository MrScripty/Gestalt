use dioxus::prelude::*;

pub(crate) fn bump_refresh_nonce(mut git_refresh_nonce: Signal<u64>) {
    let next = git_refresh_nonce.read().saturating_add(1);
    git_refresh_nonce.set(next);
}
