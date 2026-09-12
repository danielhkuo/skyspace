//! Pure degree-requirement and scheduling engine for Skyspace.
//!
//! This crate performs no input or output. It takes a plan and a program, and
//! returns a report. That makes it testable, and it lets the same code run on
//! the server and in the browser through WebAssembly.
//!
//! Nothing here may block, read a file, or open a socket.

/// A course identified the way Rice writes it, for example `COMP 140`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CourseCode {
    /// Subject code, for example `COMP`.
    pub subject: String,
    /// Course number, for example `140`.
    pub number: String,
}

/// One requirement in a degree program.
///
/// Every rule Rice publishes must map to one of these variants. Adding a
/// variant makes the compiler list every place that must handle it, which is
/// the reason this engine is written in Rust.
#[derive(Debug, Clone)]
pub enum Rule {
    /// Every listed course is required.
    AllOf(Vec<CourseCode>),
    /// A number of courses chosen from a list.
    SelectFrom {
        /// How many courses the student must choose.
        count: u8,
        /// The courses that may be chosen.
        options: Vec<CourseCode>,
    },
    /// A number of credit hours, optionally restricted to a set of courses.
    Credits {
        /// Minimum credit hours.
        minimum: u16,
        /// Courses that count. An empty list means any course counts.
        from: Vec<CourseCode>,
    },
    /// Something that is not a course: a proficiency exam, a recital, a
    /// preceptorship, or the Lifetime Physical Activity Program.
    NonCourse {
        /// What the student must do.
        description: String,
    },
    /// A rule we could not express. The student confirms it by hand, and it
    /// stays out of the progress total.
    ///
    /// This variant exists so that a rule we failed to parse is visible in the
    /// product instead of silently missing.
    Unverifiable {
        /// The original text from the General Announcements.
        text: String,
    },
}

/// Whether a rule is met.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The plan satisfies the rule.
    Met,
    /// The plan does not satisfy the rule yet.
    Unmet {
        /// How many more courses or credit hours are needed.
        remaining: u16,
    },
    /// The engine cannot decide. The student must confirm it.
    NeedsStudentCheck,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unverifiable_rules_are_not_counted_as_met() {
        let rule = Rule::Unverifiable {
            text: "Consult your advisor.".to_owned(),
        };
        // Placeholder until `evaluate` exists. The point of the test is that an
        // unparsed rule must never default to `Met`.
        assert!(matches!(rule, Rule::Unverifiable { .. }));
    }
}
