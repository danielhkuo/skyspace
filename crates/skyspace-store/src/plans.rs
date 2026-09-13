//! Plans: the student's document as one `jsonb` body, and the bundle the
//! engine evaluates it against.

use std::collections::{BTreeMap, BTreeSet};

use skyspace_core::code::CourseCode;
use skyspace_core::evaluate::{CourseFacts, CourseInfo, PlanBundle};
use skyspace_core::plan::{AccountId, Plan, PlanId, TermKind};
use skyspace_core::prereq::{Exclusion, PrereqExpr, PrereqObservation, fold_prerequisites};
use skyspace_core::program::{CatalogYear, CourseSelector, Program, RequirementBody};
use skyspace_core::term::{Credits, Season, TermCode, TermPosition};
use skyspace_core::warn::CreditLimits;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::convert::{attributes_from_db, code_from_db, credits_from_db, term_from_db};
use crate::courses::{year_from_db, year_to_db};
use crate::error::StoreError;
use crate::pool::Store;

/// One plan, for the plan list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSummaryRow {
    /// The plan.
    pub id: PlanId,
    /// Display name.
    pub name: String,
    /// The edition it is evaluated against.
    pub catalog_year: CatalogYear,
    /// The one the board opens on.
    pub is_active: bool,
    /// Optimistic-concurrency version.
    pub version: i32,
    /// Last write.
    pub updated_at: OffsetDateTime,
}

/// What deleting a plan did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    /// Gone.
    Deleted,
    /// No such plan for this account.
    NotFound,
    /// Refused: an account keeps at least one plan.
    LastPlan,
}

/// Rice's normal semester load, for the informational note.
#[must_use]
pub fn default_limits() -> CreditLimits {
    CreditLimits {
        fall_spring: Credits::from_cents(1800),
        music_and_architecture: Credits::from_cents(2000),
        summer: None,
    }
}

#[derive(sqlx::FromRow)]
struct SummaryDbRow {
    id: Uuid,
    name: String,
    catalog_year: i16,
    is_active: bool,
    version: i32,
    updated_at: OffsetDateTime,
}

impl TryFrom<SummaryDbRow> for PlanSummaryRow {
    type Error = StoreError;

    fn try_from(row: SummaryDbRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: PlanId(row.id),
            name: row.name,
            catalog_year: year_from_db("plans", row.catalog_year)?,
            is_active: row.is_active,
            version: row.version,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct BodyRow {
    version: i32,
    updated_at: OffsetDateTime,
    body: serde_json::Value,
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    version: i32,
    updated_at: OffsetDateTime,
}

/// Serialise this transaction's plan writes for one account: the
/// "last plan" count, the "first plan is active" rule and the one-active
/// index are all check-then-write, so two requests for the same account
/// take this lock first. Transaction scoped: released at commit or
/// rollback. Keyed by `hashtext` of the uuid text, which is stable
/// across Postgres releases.
async fn lock_account(conn: &mut PgConnection, account: AccountId) -> Result<(), StoreError> {
    sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
        .bind(account.0)
        .execute(conn)
        .await?;
    Ok(())
}

async fn insert_plan(
    conn: &mut PgConnection,
    account: AccountId,
    plan: &Plan,
    active_if_first: bool,
) -> Result<(PlanId, i32), StoreError> {
    let id = Uuid::new_v4();
    let stored = Plan {
        id: PlanId(id),
        ..plan.clone()
    };
    let version: i32 = sqlx::query_scalar(
        "insert into plans (id, account_id, name, catalog_year, is_active, body) \
         values ($1, $2, $3, $4, $5 and not exists (select 1 from plans where account_id = $2), $6) \
         returning version",
    )
    .bind(id)
    .bind(account.0)
    .bind(&stored.name)
    .bind(year_to_db(stored.catalog_year)?)
    .bind(active_if_first)
    .bind(serde_json::to_value(&stored)?)
    .fetch_one(conn)
    .await?;
    Ok((PlanId(id), version))
}

/// Every course code a plan names: catalog cards and the Rice equivalents
/// of manual cards.
fn plan_codes(plan: &Plan) -> BTreeSet<CourseCode> {
    let mut codes = BTreeSet::new();
    codes.extend(
        plan.incoming_credit
            .iter()
            .filter_map(|c| c.rice_equivalent.clone()),
    );
    for term in &plan.terms {
        match &term.kind {
            TermKind::Rice { courses, .. } => {
                codes.extend(courses.iter().map(|c| c.course.clone()));
            }
            TermKind::Away { cards } => {
                codes.extend(cards.iter().filter_map(|c| c.rice_equivalent.clone()));
            }
            TermKind::Off => {}
        }
    }
    codes
}

/// Every code a program's selectors name.
fn program_codes(program: &Program, codes: &mut BTreeSet<CourseCode>) {
    program.root.walk(&mut |r| {
        let filters = match &r.body {
            RequirementBody::Course { filter, .. } => vec![filter],
            RequirementBody::Credits { from, .. } => vec![from],
            _ => Vec::new(),
        };
        for filter in filters {
            for selector in filter.include.iter().chain(&filter.exclude) {
                if let CourseSelector::Code { code } = selector {
                    codes.insert(code.clone());
                }
            }
        }
    });
}

fn split(codes: &BTreeSet<CourseCode>) -> (Vec<String>, Vec<String>) {
    codes
        .iter()
        .map(|c| (c.subject.as_str().to_owned(), c.number.as_str().to_owned()))
        .unzip()
}

const WANT: &str = "join unnest($1::text[], $2::text[]) as want(subject, number) \
                    on want.subject = c.subject and want.number = c.number";

async fn aliases(
    conn: &mut PgConnection,
    codes: &BTreeSet<CourseCode>,
) -> Result<Vec<(CourseCode, CourseCode)>, StoreError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        alias_subject: String,
        alias_number: String,
        canonical_subject: String,
        canonical_number: String,
    }
    let (subjects, numbers) = split(codes);
    let rows: Vec<Row> = sqlx::query_as(
        "select distinct a.alias_subject, a.alias_number, a.canonical_subject, a.canonical_number \
         from course_aliases a join unnest($1::text[], $2::text[]) as want(subject, number) \
           on (want.subject = a.alias_subject and want.number = a.alias_number) \
           or (want.subject = a.canonical_subject and want.number = a.canonical_number)",
    )
    .bind(&subjects)
    .bind(&numbers)
    .fetch_all(conn)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok((
                code_from_db("course_aliases", &r.alias_subject, &r.alias_number)?,
                code_from_db("course_aliases", &r.canonical_subject, &r.canonical_number)?,
            ))
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct InfoRow {
    subject: String,
    number: String,
    title: String,
    credits_kind: String,
    credits_min_cents: i16,
    credits_max_cents: i16,
    attributes: Vec<String>,
    department: String,
    repeatable: bool,
}

#[derive(sqlx::FromRow)]
struct SeenRow {
    subject: String,
    number: String,
    term_code: String,
    live: bool,
}

/// Facts for `codes`: the catalog record at `year`, else the newest held,
/// plus the terms each course was seen in.
async fn course_facts(
    conn: &mut PgConnection,
    codes: &BTreeSet<CourseCode>,
    year: CatalogYear,
    current: Option<TermCode>,
) -> Result<CourseFacts, StoreError> {
    const TABLE: &str = "course_catalog";
    let (subjects, numbers) = split(codes);
    let info: Vec<InfoRow> = sqlx::query_as(&format!(
        "select distinct on (c.subject, c.number) c.subject, c.number, c.title, cc.credits_kind, \
                cc.credits_min_cents, cc.credits_max_cents, cc.attributes, cc.department, cc.repeatable \
         from courses c {WANT} join course_catalog cc on cc.course_id = c.id \
         order by c.subject, c.number, (cc.catalog_year = $3) desc, cc.catalog_year desc"
    ))
    .bind(&subjects)
    .bind(&numbers)
    .bind(year_to_db(year)?)
    .fetch_all(&mut *conn)
    .await?;
    let seen: Vec<SeenRow> = sqlx::query_as(&format!(
        "select c.subject, c.number, s.term_code, bool_or(s.withdrawn_at is null) as live \
         from sections s join courses c on c.id = s.course_id {WANT} \
         group by c.subject, c.number, s.term_code"
    ))
    .bind(&subjects)
    .bind(&numbers)
    .fetch_all(&mut *conn)
    .await?;
    let mut terms_by_code: BTreeMap<CourseCode, Vec<(TermCode, bool)>> = BTreeMap::new();
    for row in seen {
        let code = code_from_db("courses", &row.subject, &row.number)?;
        let term = term_from_db("sections", &row.term_code)?;
        terms_by_code
            .entry(code)
            .or_default()
            .push((term, row.live));
    }
    let mut facts = CourseFacts::default();
    for row in info {
        let code = code_from_db("courses", &row.subject, &row.number)?;
        let terms = terms_by_code.remove(&code).unwrap_or_default();
        let seasons: BTreeSet<_> = terms.iter().filter_map(|(t, _)| t.season()).collect();
        facts.insert(CourseInfo {
            title: row.title,
            credits: credits_from_db(
                TABLE,
                &row.credits_kind,
                row.credits_min_cents,
                row.credits_max_cents,
            )?,
            attributes: attributes_from_db(TABLE, &row.attributes)?,
            department: Some(row.department),
            repeatable: row.repeatable,
            seasons_offered: seasons.into_iter().collect(),
            terms_observed: u8::try_from(terms.len()).unwrap_or(u8::MAX),
            offered_now: terms.iter().any(|(t, live)| *live && Some(*t) == current),
            code,
        });
    }
    Ok(facts)
}

#[derive(sqlx::FromRow)]
struct PrereqRow {
    subject: String,
    number: String,
    catalog_year: i16,
    prerequisite_text: Option<String>,
    prerequisite_expr: Option<serde_json::Value>,
    corequisite_subject: Option<String>,
    corequisite_number: Option<String>,
}

async fn prerequisites(
    conn: &mut PgConnection,
    codes: &BTreeSet<CourseCode>,
) -> Result<Vec<skyspace_core::prereq::Prerequisite>, StoreError> {
    const TABLE: &str = "course_catalog";
    let (subjects, numbers) = split(codes);
    let rows: Vec<PrereqRow> = sqlx::query_as(&format!(
        "select c.subject, c.number, cc.catalog_year, cc.prerequisite_text, cc.prerequisite_expr, \
                cc.corequisite_subject, cc.corequisite_number \
         from courses c {WANT} join course_catalog cc on cc.course_id = c.id"
    ))
    .bind(&subjects)
    .bind(&numbers)
    .fetch_all(conn)
    .await?;
    let mut observations = Vec::with_capacity(rows.len());
    for row in rows {
        let raw = row.prerequisite_text.unwrap_or_default();
        let parsed = match row.prerequisite_expr {
            Some(value) => serde_json::from_value(value)?,
            None => PrereqExpr::parse(&raw),
        };
        let corequisite = match (&row.corequisite_subject, &row.corequisite_number) {
            (Some(s), Some(n)) => Some(code_from_db(TABLE, s, n)?),
            _ => None,
        };
        observations.push(PrereqObservation {
            course: code_from_db("courses", &row.subject, &row.number)?,
            raw,
            parsed,
            corequisite,
            catalog_year: year_from_db(TABLE, row.catalog_year)?,
        });
    }
    Ok(fold_prerequisites(&observations))
}

#[derive(sqlx::FromRow)]
struct ExclusionRow {
    subject: String,
    number: String,
    catalog_year: i16,
    blocker_subject: String,
    blocker_number: String,
    published: String,
}

/// Exclusion rows for `codes` as the blocked side, one per `(blocked,
/// blocker)` with the newest year winning, sorted.
async fn exclusions(
    conn: &mut PgConnection,
    codes: &BTreeSet<CourseCode>,
) -> Result<Vec<Exclusion>, StoreError> {
    const TABLE: &str = "course_exclusions";
    let (subjects, numbers) = split(codes);
    let rows: Vec<ExclusionRow> = sqlx::query_as(&format!(
        "select c.subject, c.number, e.catalog_year, e.subject as blocker_subject, \
                e.number as blocker_number, e.published \
         from course_exclusions e join courses c on c.id = e.course_id {WANT}"
    ))
    .bind(&subjects)
    .bind(&numbers)
    .fetch_all(conn)
    .await?;
    let mut newest: BTreeMap<(CourseCode, CourseCode), Exclusion> = BTreeMap::new();
    for row in rows {
        let exclusion = Exclusion {
            blocked: code_from_db("courses", &row.subject, &row.number)?,
            blocker: code_from_db(TABLE, &row.blocker_subject, &row.blocker_number)?,
            published_for: year_from_db(TABLE, row.catalog_year)?,
            published: row.published,
        };
        let key = (exclusion.blocked.clone(), exclusion.blocker.clone());
        let replace = newest
            .get(&key)
            .is_none_or(|held| held.published_for < exclusion.published_for);
        if replace {
            newest.insert(key, exclusion);
        }
    }
    Ok(newest.into_values().collect())
}

/// The term a calendar date falls in, for `today` before an operator has
/// set a current term (or while a quadmester is current): August to
/// December is the fall of the next academic year, January to May the
/// spring, June and July the summer. Matriculation is not a fallback:
/// it would make every planned term "now or future" and hide past-term
/// warnings.
#[must_use]
pub fn position_of_date(date: time::Date) -> TermPosition {
    let year = u16::try_from(date.year()).unwrap_or(u16::MAX);
    match u8::from(date.month()) {
        8..=12 => TermPosition {
            academic_year: year.saturating_add(1),
            season: Season::Fall,
        },
        1..=5 => TermPosition {
            academic_year: year,
            season: Season::Spring,
        },
        _ => TermPosition {
            academic_year: year,
            season: Season::Summer,
        },
    }
}

impl Store {
    /// The account's plans, active first, then by last write.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn plans(&self, account: AccountId) -> Result<Vec<PlanSummaryRow>, StoreError> {
        let rows: Vec<SummaryDbRow> = sqlx::query_as(
            "select id, name, catalog_year, is_active, version, updated_at from plans \
             where account_id = $1 order by is_active desc, updated_at desc",
        )
        .bind(account.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(PlanSummaryRow::try_from).collect()
    }

    /// One plan with its version and last write.
    ///
    /// # Errors
    /// `StoreError::Database` or `Json`.
    pub async fn plan(
        &self,
        account: AccountId,
        id: PlanId,
    ) -> Result<Option<(i32, OffsetDateTime, Plan)>, StoreError> {
        let row: Option<BodyRow> = sqlx::query_as(
            "select version, updated_at, body from plans where account_id = $1 and id = $2",
        )
        .bind(account.0)
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| Ok((r.version, r.updated_at, serde_json::from_value(r.body)?)))
            .transpose()
    }

    /// Store a new plan under a minted id, which replaces `plan.id` in the
    /// stored body. The account's first plan becomes active.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`.
    pub async fn create_plan(
        &self,
        account: AccountId,
        plan: &Plan,
    ) -> Result<(PlanId, i32), StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_account(&mut tx, account).await?;
        let created = insert_plan(&mut tx, account, plan, true).await?;
        tx.commit().await?;
        Ok(created)
    }

    /// Replace the body when `version` is still current. `None` means a
    /// newer version was written elsewhere; the API answers `409`.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`.
    pub async fn save_plan(
        &self,
        account: AccountId,
        id: PlanId,
        version: i32,
        plan: &Plan,
    ) -> Result<Option<(i32, OffsetDateTime)>, StoreError> {
        let stored = Plan { id, ..plan.clone() };
        let row: Option<VersionRow> = sqlx::query_as(
            "update plans set body = $4, name = $5, catalog_year = $6, version = version + 1, updated_at = now() \
             where id = $1 and account_id = $2 and version = $3 returning version, updated_at",
        )
        .bind(id.0)
        .bind(account.0)
        .bind(version)
        .bind(serde_json::to_value(&stored)?)
        .bind(&stored.name)
        .bind(year_to_db(stored.catalog_year)?)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| (r.version, r.updated_at)))
    }

    /// Delete a plan, refusing to delete the account's last one. When the
    /// active plan goes, the most recently written one becomes active.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn delete_plan(
        &self,
        account: AccountId,
        id: PlanId,
    ) -> Result<DeleteOutcome, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_account(&mut tx, account).await?;
        let count: i64 = sqlx::query_scalar("select count(*) from plans where account_id = $1")
            .bind(account.0)
            .fetch_one(&mut *tx)
            .await?;
        let was_active: Option<bool> = sqlx::query_scalar(
            "select is_active from plans where account_id = $1 and id = $2 for update",
        )
        .bind(account.0)
        .bind(id.0)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(was_active) = was_active else {
            return Ok(DeleteOutcome::NotFound);
        };
        if count <= 1 {
            return Ok(DeleteOutcome::LastPlan);
        }
        sqlx::query("delete from plans where account_id = $1 and id = $2")
            .bind(account.0)
            .bind(id.0)
            .execute(&mut *tx)
            .await?;
        if was_active {
            sqlx::query(
                "update plans set is_active = true where id = \
                 (select id from plans where account_id = $1 order by updated_at desc limit 1)",
            )
            .bind(account.0)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(DeleteOutcome::Deleted)
    }

    /// Copy a plan under a new id and name. `None` when the source is not
    /// the account's.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`.
    pub async fn duplicate_plan(
        &self,
        account: AccountId,
        id: PlanId,
        name: &str,
    ) -> Result<Option<(PlanId, i32)>, StoreError> {
        let Some((_, _, plan)) = self.plan(account, id).await? else {
            return Ok(None);
        };
        let copy = Plan {
            name: name.to_owned(),
            ..plan
        };
        let mut conn = self.pool.acquire().await?;
        Ok(Some(insert_plan(&mut conn, account, &copy, false).await?))
    }

    /// Make one plan the active one. `false` when it is not the account's.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn set_active_plan(
        &self,
        account: AccountId,
        id: PlanId,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_account(&mut tx, account).await?;
        sqlx::query("update plans set is_active = false where account_id = $1 and is_active")
            .bind(account.0)
            .execute(&mut *tx)
            .await?;
        let done =
            sqlx::query("update plans set is_active = true where account_id = $1 and id = $2")
                .bind(account.0)
                .bind(id.0)
                .execute(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(done.rows_affected() > 0)
    }

    /// How many plans the account holds.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn plan_count(&self, account: AccountId) -> Result<u32, StoreError> {
        let count: i64 = sqlx::query_scalar("select count(*) from plans where account_id = $1")
            .bind(account.0)
            .fetch_one(&self.pool)
            .await?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// Everything the engine needs for one plan, assembled here so the
    /// server and the browser cannot be handed different slices.
    ///
    /// # Errors
    /// `StoreError::Database`, `Corrupt` or `Json`.
    pub async fn plan_bundle(
        &self,
        account: AccountId,
        id: PlanId,
    ) -> Result<Option<PlanBundle>, StoreError> {
        let Some((_, _, plan)) = self.plan(account, id).await? else {
            return Ok(None);
        };
        let programs = self
            .programs_for_plan(&plan.programs, plan.catalog_year)
            .await?;
        let current = self.current_term().await?;
        let mut codes = plan_codes(&plan);
        for program in &programs {
            program_codes(program, &mut codes);
        }
        let mut conn = self.pool.acquire().await?;
        let alias_pairs = aliases(&mut conn, &codes).await?;
        for (alias, canonical) in &alias_pairs {
            codes.insert(alias.clone());
            codes.insert(canonical.clone());
        }
        let mut facts = course_facts(&mut conn, &codes, plan.catalog_year, current).await?;
        for (alias, canonical) in alias_pairs {
            facts.add_alias(alias, canonical);
        }
        let prerequisites = prerequisites(&mut conn, &codes).await?;
        let exclusions = exclusions(&mut conn, &codes).await?;
        let today = current
            .and_then(TermCode::position)
            .unwrap_or_else(|| position_of_date(OffsetDateTime::now_utc().date()));
        Ok(Some(PlanBundle {
            plan,
            programs,
            facts,
            prerequisites,
            exclusions,
            limits: default_limits(),
            invalidations: Vec::new(),
            today,
        }))
    }
}

#[cfg(test)]
mod tests {
    use skyspace_core::catalog::Attribute;
    use skyspace_core::plan::PlanId;
    use skyspace_core::prereq::{PrereqFact, prereq_fact};
    use skyspace_core::program::CatalogYear;
    use skyspace_core::term::{Season, TermPosition};
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::DeleteOutcome;
    use crate::pool::Store;
    use crate::programs::VersionSource;
    use crate::testing::{
        all, code, course, course_rule, fall, listing, plan, program, seed_account, seed_term,
        timed, with_attribute, with_exclusion, with_prereq,
    };

    #[sqlx::test]
    async fn create_list_read_save(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        let other = seed_account(&store, "b@rice.edu").await;
        assert_eq!(store.plan_count(account).await.unwrap(), 0);
        let (id, version) = store
            .create_plan(account, &plan("First", 2026, Vec::new(), &["COMP 140"]))
            .await
            .unwrap();
        assert_eq!(version, 1);
        let (id2, _) = store
            .create_plan(account, &plan("Second", 2026, Vec::new(), &[]))
            .await
            .unwrap();
        let list = store.plans(account).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, id);
        assert!(list[0].is_active);
        assert!(!list[1].is_active);
        assert!(store.plans(other).await.unwrap().is_empty());

        let (v, _, stored) = store.plan(account, id).await.unwrap().unwrap();
        assert_eq!(v, 1);
        assert_eq!(stored.id, id);
        assert_eq!(stored.name, "First");
        assert!(store.plan(other, id).await.unwrap().is_none());

        let mut edited = stored.clone();
        edited.name = "Renamed".to_owned();
        edited.id = PlanId(Uuid::new_v4());
        let (v2, _) = store
            .save_plan(account, id, 1, &edited)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v2, 2);
        assert!(
            store
                .save_plan(account, id, 1, &edited)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .save_plan(other, id, 2, &edited)
                .await
                .unwrap()
                .is_none()
        );
        let (_, _, stored) = store.plan(account, id).await.unwrap().unwrap();
        assert_eq!(stored.id, id);
        assert_eq!(stored.name, "Renamed");
        assert_eq!(store.plans(account).await.unwrap()[0].name, "Renamed");

        assert!(store.set_active_plan(account, id2).await.unwrap());
        assert!(!store.set_active_plan(other, id2).await.unwrap());
        assert_eq!(store.plans(account).await.unwrap()[0].id, id2);
        let (copy, _) = store
            .duplicate_plan(account, id, "Copy")
            .await
            .unwrap()
            .unwrap();
        assert!(
            store
                .duplicate_plan(other, id, "Copy")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store.plan(account, copy).await.unwrap().unwrap().2.name,
            "Copy"
        );
        assert_eq!(store.plan_count(account).await.unwrap(), 3);
    }

    #[sqlx::test]
    async fn delete_keeps_the_last_plan(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        let other = seed_account(&store, "b@rice.edu").await;
        let (first, _) = store
            .create_plan(account, &plan("First", 2026, Vec::new(), &[]))
            .await
            .unwrap();
        assert_eq!(
            store.delete_plan(account, first).await.unwrap(),
            DeleteOutcome::LastPlan
        );
        let (second, _) = store
            .create_plan(account, &plan("Second", 2026, Vec::new(), &[]))
            .await
            .unwrap();
        assert_eq!(
            store.delete_plan(other, first).await.unwrap(),
            DeleteOutcome::NotFound
        );
        assert_eq!(
            store.delete_plan(account, first).await.unwrap(),
            DeleteOutcome::Deleted
        );
        let list = store.plans(account).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, second);
        assert!(list[0].is_active);
        assert_eq!(
            store.delete_plan(account, first).await.unwrap(),
            DeleteOutcome::NotFound
        );
    }

    /// Two deletes at once cannot both see "two plans left".
    #[sqlx::test]
    async fn concurrent_deletes_keep_one_plan(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        let (first, _) = store
            .create_plan(account, &plan("First", 2026, Vec::new(), &[]))
            .await
            .unwrap();
        let (second, _) = store
            .create_plan(account, &plan("Second", 2026, Vec::new(), &[]))
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            store.delete_plan(account, first),
            store.delete_plan(account, second)
        );
        let outcomes = [a.unwrap(), b.unwrap()];
        assert!(outcomes.contains(&DeleteOutcome::Deleted), "{outcomes:?}");
        assert!(outcomes.contains(&DeleteOutcome::LastPlan), "{outcomes:?}");
        let left = store.plans(account).await.unwrap();
        assert_eq!(left.len(), 1);
        assert!(left[0].is_active, "the survivor is active");
    }

    /// Two first creates at once: both succeed, exactly one is active.
    #[sqlx::test]
    async fn concurrent_first_creates_make_one_active(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        let plan_a = plan("A", 2026, Vec::new(), &[]);
        let plan_b = plan("B", 2026, Vec::new(), &[]);
        let (a, b) = tokio::join!(
            store.create_plan(account, &plan_a),
            store.create_plan(account, &plan_b)
        );
        a.unwrap();
        b.unwrap();
        let list = store.plans(account).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list.iter().filter(|p| p.is_active).count(), 1);
        let (c, d) = tokio::join!(
            store.set_active_plan(account, list[0].id),
            store.set_active_plan(account, list[1].id)
        );
        assert!(c.unwrap() && d.unwrap());
        let list = store.plans(account).await.unwrap();
        assert_eq!(list.iter().filter(|p| p.is_active).count(), 1);
    }

    /// Before an operator sets a current term, `today` is the wall-clock
    /// date's term, not matriculation.
    #[sqlx::test]
    async fn today_falls_back_to_the_date(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        let mut p = plan("Plan", 2026, Vec::new(), &[]);
        p.matriculation = TermPosition {
            academic_year: 2000,
            season: Season::Fall,
        };
        let (id, _) = store.create_plan(account, &p).await.unwrap();
        let bundle = store.plan_bundle(account, id).await.unwrap().unwrap();
        assert_eq!(
            bundle.today,
            super::position_of_date(time::OffsetDateTime::now_utc().date())
        );
        assert_ne!(bundle.today, p.matriculation);
    }

    #[test]
    fn dates_map_to_terms() {
        use time::macros::date;
        let fall = TermPosition {
            academic_year: 2027,
            season: Season::Fall,
        };
        assert_eq!(super::position_of_date(date!(2026 - 08 - 24)), fall);
        assert_eq!(super::position_of_date(date!(2026 - 12 - 31)), fall);
        assert_eq!(
            super::position_of_date(date!(2027 - 01 - 10)),
            TermPosition {
                academic_year: 2027,
                season: Season::Spring
            }
        );
        assert_eq!(
            super::position_of_date(date!(2027 - 06 - 15)),
            TermPosition {
                academic_year: 2027,
                season: Season::Summer
            }
        );
    }

    #[sqlx::test]
    async fn bundle_assembles_facts_aliases_prerequisites_and_exclusions(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        seed_term(&store, "202620").await;
        store.set_current_term(fall()).await.unwrap();
        let mut comp182 = with_prereq(course("COMP 182", "Algorithmic Thinking", 2026), "COMP 140");
        comp182.cross_list = vec![code("ELEC 182")];
        comp182 = with_exclusion(
            comp182,
            &["COMP 310"],
            "Cannot register for COMP 182 if student has credit for COMP 310.",
        );
        store
            .upsert_courses(
                CatalogYear(2026),
                &[
                    with_attribute(
                        course("COMP 140", "Computational Thinking", 2026),
                        Attribute::DistributionThree,
                    ),
                    comp182,
                    course("MATH 101", "Calculus I", 2026),
                ],
            )
            .await
            .unwrap();
        let mut newer = course("COMP 140", "Computational Thinking", 2027);
        newer.department = "Renamed Department".to_owned();
        store
            .upsert_courses(CatalogYear(2027), &[newer])
            .await
            .unwrap();
        store
            .upsert_sections(
                fall(),
                &[listing(
                    10001,
                    "COMP 140",
                    "CT",
                    vec![timed("MWF", 540, 590)],
                )],
            )
            .await
            .unwrap();
        let mut spring = listing(20001, "COMP 140", "CT", vec![]);
        spring.term = crate::testing::term("202620");
        store
            .upsert_sections(crate::testing::term("202620"), &[spring])
            .await
            .unwrap();

        let program_id = store
            .publish_program(
                &program(
                    "example-bs",
                    2026,
                    all(
                        "Root",
                        vec![course_rule("COMP 140"), course_rule("MATH 101")],
                    ),
                ),
                "alice",
                VersionSource::Ga,
            )
            .await
            .unwrap();
        let account = seed_account(&store, "a@rice.edu").await;
        let (id, _) = store
            .create_plan(
                account,
                &plan("Plan", 2026, vec![program_id], &["COMP 182", "COMP 999"]),
            )
            .await
            .unwrap();
        assert!(
            store
                .plan_bundle(seed_account(&store, "b@rice.edu").await, id)
                .await
                .unwrap()
                .is_none()
        );
        let bundle = store.plan_bundle(account, id).await.unwrap().unwrap();

        assert_eq!(bundle.plan.id, id);
        assert_eq!(bundle.programs.len(), 1);
        assert_eq!(
            bundle.today,
            TermPosition {
                academic_year: 2027,
                season: Season::Fall
            }
        );
        assert_eq!(bundle.limits, super::default_limits());
        let comp140 = bundle.facts.get(&code("COMP 140")).unwrap();
        assert_eq!(comp140.title, "Computational Thinking");
        assert!(comp140.attributes.contains(&Attribute::DistributionThree));
        // The plan's year wins over the newer record; `courses.title` is per course, not per year.
        assert_eq!(comp140.department.as_deref(), Some("Computer Science"));
        assert_eq!(comp140.seasons_offered, vec![Season::Fall, Season::Spring]);
        assert_eq!(comp140.terms_observed, 2);
        assert!(comp140.offered_now);
        let comp182 = bundle.facts.get(&code("COMP 182")).unwrap();
        assert!(!comp182.offered_now);
        assert!(bundle.facts.get(&code("COMP 999")).is_none());
        assert_eq!(bundle.facts.canonical(&code("ELEC 182")), code("COMP 182"));
        assert!(bundle.facts.get(&code("ELEC 182")).is_some());
        assert!(matches!(
            prereq_fact(&bundle.prerequisites, &code("COMP 182")),
            PrereqFact::Requires { .. }
        ));
        assert!(matches!(
            prereq_fact(&bundle.prerequisites, &code("COMP 140")),
            PrereqFact::NoneRequired { .. }
        ));
        assert!(matches!(
            prereq_fact(&bundle.prerequisites, &code("COMP 999")),
            PrereqFact::Unknown
        ));
        assert_eq!(bundle.exclusions.len(), 1);
        assert_eq!(bundle.exclusions[0].blocked, code("COMP 182"));
        assert_eq!(bundle.exclusions[0].blocker, code("COMP 310"));
        assert!(bundle.invalidations.is_empty());
        let report = skyspace_core::evaluate(&bundle);
        assert_eq!(report.programs.len(), 1);
    }
}
