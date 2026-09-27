use clawcrew_log_attribution_macro_support::FixtureAttributable;

fn main() {
    let _span = clawcrew_log::attribution_span!(&FixtureAttributable);
}
