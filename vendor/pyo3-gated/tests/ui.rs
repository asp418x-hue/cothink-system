#[test]
fn ui_passes() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/*.rs");
}

#[test]
fn ui_failures() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/conflicting_sentinels.rs");
    t.compile_fail("tests/ui/fail/*.rs");
}
