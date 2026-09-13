//! Loading a `Course` from its per-year catalog record.

use std::collections::BTreeMap;

use skyspace_core::catalog::{Course, CourseFlags, CourseType, GradeMode, Restrictions};
use skyspace_core::code::CourseCode;
use skyspace_core::prereq::{MutualExclusion, PrereqExpr, PrereqObservation};
use skyspace_core::program::CatalogYear;
use sqlx::PgConnection;

use crate::convert::{attributes_from_db, code_from_db, credits_from_db};
use crate::error::{StoreError, corrupt};

const TABLE: &str = "course_catalog";

#[derive(sqlx::FromRow)]
struct CourseRow {
    id: i64,
    subject: String,
    number: String,
    title: String,
    catalog_year: i16,
    department: String,
    credits_kind: String,
    credits_min_cents: i16,
    credits_max_cents: i16,
    attributes: Vec<String>,
    grade_mode: Option<String>,
    course_type: Option<String>,
    restriction_text: Option<String>,
    restrictions: Option<serde_json::Value>,
    prerequisite_text: Option<String>,
    prerequisite_expr: Option<serde_json::Value>,
    corequisite_subject: Option<String>,
    corequisite_number: Option<String>,
    description: String,
    repeatable: bool,
    instructor_permission: bool,
    second_half: bool,
    equivalents: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct ExclusionRow {
    subject: String,
    number: String,
    published: String,
}

#[derive(sqlx::FromRow)]
pub(crate) struct AliasRow {
    pub alias_subject: String,
    pub alias_number: String,
    pub canonical_subject: String,
    pub canonical_number: String,
}

/// A catalog year column.
pub(crate) fn year_from_db(table: &'static str, value: i16) -> Result<CatalogYear, StoreError> {
    u16::try_from(value)
        .map(CatalogYear)
        .map_err(|_| corrupt(table, "catalog_year", format!("negative year {value}")))
}

/// A catalog year into its `smallint`.
pub(crate) fn year_to_db(year: CatalogYear) -> Result<i16, StoreError> {
    i16::try_from(year.0).map_err(|_| StoreError::Input(format!("catalog year {}", year.0)))
}

/// Rice's stored prerequisite observation, or `None` when Rice printed no
/// field. A null `prerequisite_expr` beside a text is re-parsed.
fn prerequisites(
    row: &CourseRow,
    code: &CourseCode,
    year: CatalogYear,
) -> Result<Option<PrereqObservation>, StoreError> {
    let Some(raw) = row.prerequisite_text.clone() else {
        return Ok(None);
    };
    let parsed = match row.prerequisite_expr.clone() {
        Some(value) => serde_json::from_value(value)?,
        None => PrereqExpr::parse(&raw),
    };
    let corequisite = match (&row.corequisite_subject, &row.corequisite_number) {
        (Some(subject), Some(number)) => Some(code_from_db(TABLE, subject, number)?),
        _ => None,
    };
    Ok(Some(PrereqObservation {
        course: code.clone(),
        raw,
        parsed,
        corequisite,
        catalog_year: year,
    }))
}

/// Exclusion rows back into sentences: rows sharing one `published` text
/// were one sentence.
fn fold_exclusions(rows: Vec<ExclusionRow>) -> Result<Vec<MutualExclusion>, StoreError> {
    let mut order: Vec<String> = Vec::new();
    let mut by_sentence: BTreeMap<String, Vec<CourseCode>> = BTreeMap::new();
    for row in rows {
        let code = code_from_db("course_exclusions", &row.subject, &row.number)?;
        if !by_sentence.contains_key(&row.published) {
            order.push(row.published.clone());
        }
        by_sentence.entry(row.published).or_default().push(code);
    }
    Ok(order
        .into_iter()
        .map(|raw| MutualExclusion {
            with: by_sentence.remove(&raw).unwrap_or_default(),
            raw,
        })
        .collect())
}

/// Every alias row in the cross-list group `code` belongs to.
pub(crate) async fn alias_group(
    conn: &mut PgConnection,
    code: &CourseCode,
) -> Result<Vec<AliasRow>, StoreError> {
    let rows: Vec<AliasRow> = sqlx::query_as(
        "with canon as (\
            select coalesce(\
                (select canonical_subject || ' ' || canonical_number from course_aliases \
                 where alias_subject = $1 and alias_number = $2), \
                $1 || ' ' || $2) as key) \
         select alias_subject, alias_number, canonical_subject, canonical_number \
         from course_aliases where canonical_subject || ' ' || canonical_number = (select key from canon) \
         order by alias_subject, alias_number",
    )
    .bind(code.subject.as_str())
    .bind(code.number.as_str())
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// The other members of `code`'s cross-list group.
async fn cross_list(
    conn: &mut PgConnection,
    code: &CourseCode,
) -> Result<Vec<CourseCode>, StoreError> {
    let rows = alias_group(conn, code).await?;
    let mut out = Vec::new();
    for row in rows {
        let canonical = code_from_db(
            "course_aliases",
            &row.canonical_subject,
            &row.canonical_number,
        )?;
        let alias = code_from_db("course_aliases", &row.alias_subject, &row.alias_number)?;
        for member in [canonical, alias] {
            if member != *code && !out.contains(&member) {
                out.push(member);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The catalog record for `code`: the one for `year` when held, else the
/// newest year held. `None` when no year holds a record.
pub(crate) async fn load_course(
    conn: &mut PgConnection,
    code: &CourseCode,
    year: CatalogYear,
) -> Result<Option<Course>, StoreError> {
    let row: Option<CourseRow> = sqlx::query_as(
        "select c.id, c.subject, c.number, c.title, cc.catalog_year, cc.department, \
                cc.credits_kind, cc.credits_min_cents, cc.credits_max_cents, cc.attributes, \
                cc.grade_mode, cc.course_type, cc.restriction_text, cc.restrictions, \
                cc.prerequisite_text, cc.prerequisite_expr, cc.corequisite_subject, \
                cc.corequisite_number, cc.description, cc.repeatable, cc.instructor_permission, \
                cc.second_half, cc.equivalents \
         from courses c join course_catalog cc on cc.course_id = c.id \
         where c.subject = $1 and c.number = $2 \
         order by (cc.catalog_year = $3) desc, cc.catalog_year desc limit 1",
    )
    .bind(code.subject.as_str())
    .bind(code.number.as_str())
    .bind(year_to_db(year)?)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let year = year_from_db(TABLE, row.catalog_year)?;
    let exclusion_rows: Vec<ExclusionRow> = sqlx::query_as(
        "select subject, number, published from course_exclusions \
         where course_id = $1 and catalog_year = $2 order by ordinal",
    )
    .bind(row.id)
    .bind(row.catalog_year)
    .fetch_all(&mut *conn)
    .await?;
    let code = code_from_db("courses", &row.subject, &row.number)?;
    let restrictions = match (&row.restrictions, &row.restriction_text) {
        (Some(value), _) => Some(serde_json::from_value::<Restrictions>(value.clone())?),
        (None, Some(raw)) => Some(Restrictions {
            raw: raw.clone(),
            clauses: Vec::new(),
        }),
        (None, None) => None,
    };
    let equivalents = row
        .equivalents
        .iter()
        .map(|text| CourseCode::parse(text).map_err(|e| corrupt(TABLE, "equivalents", e)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(Course {
        catalog_year: year,
        prerequisites: prerequisites(&row, &code, year)?,
        cross_list: cross_list(&mut *conn, &code).await?,
        mutual_exclusions: fold_exclusions(exclusion_rows)?,
        code,
        title: row.title,
        credits: credits_from_db(
            TABLE,
            &row.credits_kind,
            row.credits_min_cents,
            row.credits_max_cents,
        )?,
        department: row.department,
        attributes: attributes_from_db(TABLE, &row.attributes)?,
        grade_mode: row.grade_mode.map(GradeMode),
        course_type: row.course_type.map(CourseType),
        restrictions,
        description: row.description,
        flags: CourseFlags {
            repeatable: row.repeatable,
            instructor_permission: row.instructor_permission,
            second_half: row.second_half,
        },
        equivalents,
    }))
}
