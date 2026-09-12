#[test]
fn rsx_ui() {
    let tests = trybuild::TestCases::new();

    tests.pass("tests/ui/rsx/pass/*.rs");
    tests.compile_fail("tests/ui/rsx/fail/*.rs");
}
