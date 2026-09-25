#[test]
fn rsx_ui() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/rsx/fail/*.rs");
}
