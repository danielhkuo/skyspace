//! WebAssembly bindings for the Skyspace engine.
//!
//! Bindings only: every export deserialises its arguments, calls
//! `skyspace-core` and serialises the result. No rule lives here, so the
//! browser and the server cannot disagree about a progress total.

use serde::Serialize;
use serde::de::DeserializeOwned;
use skyspace_core::{CourseCode, CourseFacts, EntryId, Program, RequirementId, ScheduledMeeting};
use wasm_bindgen::prelude::*;

/// Runs once when the module loads. A browser panic with no message costs a
/// day, so the hook is installed in every build.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Evaluates a `PlanBundle` and returns the `Report`, warnings included.
///
/// # Errors
/// A `bundle` that is not a `PlanBundle`.
#[wasm_bindgen]
pub fn evaluate_plan(bundle: JsValue) -> Result<JsValue, JsValue> {
    let bundle = from_js(bundle)?;
    to_js(&skyspace_core::evaluate(&bundle))
}

/// Every overlapping pair among `meetings`, a `Vec<ScheduledMeeting>`, as a
/// `Vec<Conflict>`.
///
/// # Errors
/// A `meetings` value that is not a list of `ScheduledMeeting`.
#[wasm_bindgen]
pub fn schedule_conflicts(meetings: JsValue) -> Result<JsValue, JsValue> {
    let meetings: Vec<ScheduledMeeting> = from_js(meetings)?;
    to_js(&skyspace_core::find_conflicts(&meetings))
}

/// What dropping `course` into each term of `bundle`'s plan would mean, one
/// `PlacementPreview` per term. `moving` is the `EntryId` being dragged when
/// it is already on the board; `null` or `undefined` for a course from the
/// tray.
///
/// # Errors
/// An argument of the wrong shape.
#[wasm_bindgen]
pub fn preview_placement(
    bundle: JsValue,
    course: JsValue,
    moving: JsValue,
) -> Result<JsValue, JsValue> {
    let bundle = from_js(bundle)?;
    let course: CourseCode = from_js(course)?;
    let moving: Option<EntryId> = from_js(moving)?;
    to_js(&skyspace_core::preview_placement(&bundle, &course, moving))
}

/// Which of `codes` (a `Vec<CourseCode>`) match the filter of `requirement`
/// (a `RequirementId`) in `program`, canonicalised through `facts` (a
/// `CourseFacts`). Empty for a rule with no filter.
///
/// # Errors
/// An argument of the wrong shape.
#[wasm_bindgen]
pub fn requirement_matches(
    program: JsValue,
    requirement: JsValue,
    codes: JsValue,
    facts: JsValue,
) -> Result<JsValue, JsValue> {
    let program: Program = from_js(program)?;
    let requirement: RequirementId = from_js(requirement)?;
    let codes: Vec<CourseCode> = from_js(codes)?;
    let facts: CourseFacts = from_js(facts)?;
    to_js(&skyspace_core::warn::requirement_matches(
        &program,
        requirement,
        &codes,
        &facts,
    ))
}

/// The engine version compiled into this module. The browser compares it
/// with the server's on start-up and reloads when they differ.
#[wasm_bindgen]
#[must_use]
pub fn engine_version() -> String {
    skyspace_core::ENGINE_VERSION.to_owned()
}

fn from_js<T: DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(JsValue::from)
}

/// `json_compatible` makes Rust maps cross as objects, not `Map`, and keeps
/// large integers as numbers, not `BigInt`, which is what TypeScript expects
/// and what `JSON.stringify` can serialise.
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(JsValue::from)
}
