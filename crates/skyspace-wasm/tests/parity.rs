#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

// Native and wasm must produce identical reports. Bundles go through
// `JSON.parse`, the shape a `fetch` response gives the real app, so this
// tests the serde attributes and the `serde-wasm-bindgen` settings as well
// as the engine.

use serde_json::Value;
use skyspace_core::catalog::{DaySet, Meeting, MeetingPattern, MeetingTime, MinuteOfDay};
use skyspace_core::schedule::MeetingOwner;
use skyspace_core::{Conflict, Crn, ScheduledMeeting, find_conflicts};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

const ARCH_BUNDLE: &str =
    include_str!("../../skyspace-core/tests/golden/arch-barch-direct-entry-2026/bundle.json");
const ARCH_EXPECTED: &str =
    include_str!("../../skyspace-core/tests/golden/arch-barch-direct-entry-2026/report.json");
const COMP_BUNDLE: &str =
    include_str!("../../skyspace-core/tests/golden/comp-bscs-2026/bundle.json");
const COMP_EXPECTED: &str =
    include_str!("../../skyspace-core/tests/golden/comp-bscs-2026/report.json");

fn wasm_report(bundle_json: &str) -> Value {
    let bundle = js_sys::JSON::parse(bundle_json).expect("golden bundle is valid JSON");
    let report =
        skyspace_wasm::evaluate_plan(bundle).expect("evaluate_plan accepts the golden bundle");
    serde_wasm_bindgen::from_value(report).expect("report is JSON-compatible")
}

fn native_report(expected_json: &str) -> Value {
    serde_json::from_str(expected_json).expect("golden report is valid JSON")
}

#[wasm_bindgen_test]
fn wasm_report_matches_the_native_golden_file() {
    assert_eq!(wasm_report(ARCH_BUNDLE), native_report(ARCH_EXPECTED));
}

#[wasm_bindgen_test]
fn comp_bscs_report_matches_the_native_golden_file() {
    assert_eq!(wasm_report(COMP_BUNDLE), native_report(COMP_EXPECTED));
}

fn meeting(crn: u32, days: &str, start: u16, end: u16) -> ScheduledMeeting {
    ScheduledMeeting {
        owner: MeetingOwner::Section(Crn(crn)),
        meeting: Meeting {
            pattern: MeetingPattern::Timed(MeetingTime {
                days: DaySet::from_letters(days).unwrap(),
                start: MinuteOfDay::new(start).unwrap(),
                end: MinuteOfDay::new(end).unwrap(),
            }),
            dates: None,
        },
        part_of_term: None,
    }
}

#[wasm_bindgen_test]
fn schedule_conflicts_round_trips_through_js() {
    let meetings = vec![
        meeting(10_001, "MWF", 600, 650),
        meeting(10_002, "WF", 630, 720),
        meeting(10_003, "TR", 600, 650),
    ];
    let json = serde_json::to_string(&meetings).unwrap();
    let input = js_sys::JSON::parse(&json).unwrap();
    let output = skyspace_wasm::schedule_conflicts(input).unwrap();
    let conflicts: Vec<Conflict> = serde_wasm_bindgen::from_value(output).unwrap();
    assert_eq!(conflicts, find_conflicts(&meetings));
    assert_eq!(conflicts.len(), 1);
}

#[wasm_bindgen_test]
fn engine_version_is_the_core_version() {
    assert_eq!(
        skyspace_wasm::engine_version(),
        skyspace_core::ENGINE_VERSION
    );
}

#[wasm_bindgen_test]
fn a_malformed_bundle_is_an_error_not_a_panic() {
    let bad = js_sys::JSON::parse(r#"{"plan": 1}"#).unwrap();
    assert!(skyspace_wasm::evaluate_plan(bad).is_err());
    assert!(skyspace_wasm::evaluate_plan(JsValue::NULL).is_err());
}
