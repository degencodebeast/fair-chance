//! The judge-path report: one PASS line per check, a FAIL line on the first failed check, and
//! refusal lines with the plain text from `refusal-texts/refusal-texts.json`.

use anyhow::anyhow;
use fair_chance_sandbox_tests::pass_list::PassList;
use fair_chance_sandbox_tests::refusal_texts::refused_with;

#[test]
fn a_passed_check_adds_one_pass_line_with_its_detail() {
    let mut list = PassList::default();
    list.check("coin launched", Ok("bag.factory.sandbox".to_string()))
        .unwrap();
    list.check("buy", Ok("1 NEAR".to_string())).unwrap();
    assert_eq!(
        list.lines(),
        ["PASS coin launched: bag.factory.sandbox", "PASS buy: 1 NEAR"]
    );
}

#[test]
fn a_failed_check_gives_a_fail_line_and_adds_no_pass_line() {
    let mut list = PassList::default();
    let failure = list
        .check("cap full", Err(anyhow!("no fc_cap_filled event")))
        .unwrap_err();
    assert_eq!(failure.to_string(), "FAIL cap full: no fc_cap_filled event");
    assert!(list.lines().is_empty());
}

#[test]
fn a_refusal_with_the_expected_code_gives_the_code_and_its_plain_text() {
    let failure = "Smart contract panicked: FC_EARLY_WINDOW";
    assert_eq!(
        refused_with(failure, "FC_EARLY_WINDOW").unwrap(),
        "FC_EARLY_WINDOW (Early supply cannot sell in the early blocks, and selling opens when the early window ends.)"
    );
}

#[test]
fn a_refusal_with_another_code_is_an_error() {
    let failure = "Smart contract panicked: FC_SLIPPAGE";
    let error = refused_with(failure, "FC_EARLY_WINDOW").unwrap_err();
    assert!(error.to_string().contains("FC_EARLY_WINDOW"), "{error}");
}

#[test]
fn a_code_with_no_plain_text_is_an_error() {
    assert!(refused_with("FC_NOT_A_CODE", "FC_NOT_A_CODE").is_err());
}
