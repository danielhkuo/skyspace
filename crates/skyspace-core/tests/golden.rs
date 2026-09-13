#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The four launch programs as golden files. Bundle and expected report are
//! compiled in, so the same bytes compile into the wasm parity test, which
//! has no filesystem. `UPDATE_GOLDEN=1 cargo test -p skyspace-core` rewrites
//! the files, and the rewrite lands in the commit diff for a reviewer.

mod common;

use skyspace_core::evaluate::{PlanBundle, evaluate};

struct Case {
    name: &'static str,
    bundle: &'static str,
    expected: &'static str,
    dir: &'static str,
    build: Option<fn() -> PlanBundle>,
}

const CASES: &[Case] = &[
    Case {
        name: "comp-bscs-2026",
        bundle: include_str!("golden/comp-bscs-2026/bundle.json"),
        expected: include_str!("golden/comp-bscs-2026/report.json"),
        dir: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/comp-bscs-2026"),
        build: None,
    },
    Case {
        name: "bioe-bs-2026",
        bundle: include_str!("golden/bioe-bs-2026/bundle.json"),
        expected: include_str!("golden/bioe-bs-2026/report.json"),
        dir: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/bioe-bs-2026"),
        build: Some(common::bioe_bundle),
    },
    Case {
        name: "musi-bassoon-bmus-2026",
        bundle: include_str!("golden/musi-bassoon-bmus-2026/bundle.json"),
        expected: include_str!("golden/musi-bassoon-bmus-2026/report.json"),
        dir: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/golden/musi-bassoon-bmus-2026"
        ),
        build: Some(common::bmus_bundle),
    },
    Case {
        name: "arch-barch-direct-entry-2026",
        bundle: include_str!("golden/arch-barch-direct-entry-2026/bundle.json"),
        expected: include_str!("golden/arch-barch-direct-entry-2026/report.json"),
        dir: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/golden/arch-barch-direct-entry-2026"
        ),
        build: Some(common::barch_bundle),
    },
];

fn pretty<T: serde::Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    text
}

#[test]
fn golden_reports_match() -> Result<(), Box<dyn std::error::Error>> {
    let update = std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1");
    let mut failures = Vec::new();
    for case in CASES {
        let bundle: PlanBundle = match case.build {
            Some(make) => {
                let built = make();
                let text = pretty(&built);
                if update {
                    std::fs::write(format!("{}/bundle.json", case.dir), &text)?;
                } else if text != case.bundle {
                    failures.push(format!("{}: bundle.json is stale", case.name));
                }
                built
            }
            None => serde_json::from_str(case.bundle)?,
        };
        let report = evaluate(&bundle);
        let actual = pretty(&report);
        if update {
            std::fs::write(format!("{}/report.json", case.dir), &actual)?;
        } else if actual != case.expected {
            failures.push(format!(
                "{}: report.json differs; run UPDATE_GOLDEN=1 cargo test -p skyspace-core and review the diff",
                case.name
            ));
        }
        // The bundle round-trips through JSON unchanged.
        let again: PlanBundle = serde_json::from_str(&pretty(&bundle))?;
        assert_eq!(again, bundle, "{}: bundle round trip", case.name);
        let again: skyspace_core::evaluate::Report = serde_json::from_str(&actual)?;
        assert_eq!(again, report, "{}: report round trip", case.name);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
