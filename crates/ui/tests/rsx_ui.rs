#[test]
fn rsx_ui() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/rsx/fail/*.rs");
}

#[test]
fn entity_reads() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/read/fail/*.rs");
}
