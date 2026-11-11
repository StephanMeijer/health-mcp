//! Comment threads on a workout, an exercise or a set: flat threads (a first comment, then replies in order) that the
//! owner and the users they share gym data with write in. `owner` is whose logbook it is; `author` who is writing.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use sqlx::AssertSqlSafe;

use crate::gym::db::{self, App, AppResult, Tx, user_err};
use crate::gym::resolve::{self, Kind};
use crate::share::Author;

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// A training session.
    Workout,
    Exercise,
    /// One logged set.
    Set,
}

impl Target {
    fn as_str(self) -> &'static str {
        match self {
            Target::Workout => "workout",
            Target::Exercise => "exercise",
            Target::Set => "set",
        }
    }
}

/// Threads of `$1` (owner) as a JSON array, newest first, each with its comments oldest first. `$2` is the author,
/// for `yours` and `can_resolve` (the thread's starter or the owner). Filters: `$3` target, `$4` target id, `$5` include
/// resolved, `$6` limit, `$7` one thread, `$8` a workout's own threads plus those on its sets.
const THREADS: &str = "
SELECT coalesce(json_agg(j ORDER BY created_at DESC, id DESC), '[]'::json) FROM (
  SELECT t.id, t.created_at, json_build_object(
    'thread_id', t.id, 'target', t.target,
    'workout_id', t.workout_id, 'exercise_id', t.exercise_id, 'set_id', t.set_id,
    'exercise', (SELECT e.name FROM exercise e
                 WHERE e.id = coalesce(t.exercise_id, (SELECT s.exercise_id FROM workout_set s WHERE s.id = t.set_id))),
    'resolved', t.resolved_at IS NOT NULL, 'resolved_at', t.resolved_at, 'created_at', t.created_at,
    'can_resolve', t.started_by_sub = $2 OR t.owner_sub = $2,
    'comments', (SELECT json_agg(json_build_object(
        'id', c.id, 'author', c.author_email, 'by_owner', c.author_sub = t.owner_sub, 'yours', c.author_sub = $2,
        'body', CASE WHEN c.deleted_at IS NULL THEN c.body END, 'deleted', c.deleted_at IS NOT NULL,
        'created_at', c.created_at, 'edited_at', c.edited_at) ORDER BY c.created_at, c.id)
      FROM comment c WHERE c.thread_id = t.id)) AS j
  FROM comment_thread t
  WHERE t.owner_sub = $1
    AND ($3::text IS NULL OR t.target = $3)
    AND ($4::bigint IS NULL OR coalesce(t.workout_id, t.exercise_id, t.set_id) = $4)
    AND ($5 OR $7::bigint IS NOT NULL OR t.resolved_at IS NULL)
    AND ($7::bigint IS NULL OR t.id = $7)
    AND ($8::bigint IS NULL OR t.workout_id = $8
         OR t.set_id IN (SELECT s.id FROM workout_set s WHERE s.workout_id = $8))
  ORDER BY t.created_at DESC, t.id DESC
  LIMIT $6) x";

/// Which threads [`threads`] returns.
#[derive(Default)]
struct Filter {
    target: Option<Target>,
    target_id: Option<i64>,
    include_resolved: bool,
    limit: i64,
    thread: Option<i64>,
    workout: Option<i64>,
}

async fn threads(tx: &mut Tx, owner: &str, author: &Author, f: Filter) -> AppResult<Value> {
    Ok(sqlx::query_scalar(THREADS)
        .bind(owner)
        .bind(&author.sub)
        .bind(f.target.map(Target::as_str))
        .bind(f.target_id)
        .bind(f.include_resolved)
        .bind(f.limit)
        .bind(f.thread)
        .bind(f.workout)
        .fetch_one(&mut **tx)
        .await?)
}

/// One thread, as `threads` renders it.
async fn thread(tx: &mut Tx, owner: &str, author: &Author, id: i64) -> AppResult<Value> {
    let only = Filter {
        limit: 1,
        thread: Some(id),
        ..Filter::default()
    };
    let all = threads(tx, owner, author, only).await?;
    all.get(0)
        .cloned()
        .ok_or_else(|| user_err(format!("no comment thread with id {id}")))
}

fn body(s: &str) -> AppResult<&str> {
    let b = s.trim();
    match b.chars().count() {
        0 => Err(user_err("the comment is empty")),
        n if n > 10_000 => Err(user_err("a comment is at most 10000 characters")),
        _ => Ok(b),
    }
}

async fn add_comment(tx: &mut Tx, thread: i64, author: &Author, text: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO comment (thread_id, author_sub, author_email, body) VALUES ($1, $2, $3, $4)")
        .bind(thread)
        .bind(&author.sub)
        .bind(&author.email)
        .bind(text)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThreadStartArgs {
    /// What the thread is about.
    pub target: Target,
    /// Id of the workout, exercise or set.
    pub id: Option<i64>,
    /// For target exercise: its name or alias instead of `id`.
    pub exercise: Option<String>,
    /// The first comment.
    pub body: String,
}

pub async fn thread_start(app: &App, owner: &str, author: &Author, a: ThreadStartArgs) -> AppResult<Value> {
    let text = body(&a.body)?;
    let mut tx = db::begin(app, owner).await?;
    let id = match (a.target, a.id, &a.exercise) {
        (Target::Exercise, None, Some(name)) => resolve::id(&mut tx, owner, Kind::Exercise, name).await?,
        (_, Some(id), None) => id,
        (Target::Exercise, ..) => return Err(user_err("give the exercise's id or its name (exercise), not both")),
        _ => return Err(user_err(format!("give the {}'s id", a.target.as_str()))),
    };
    // The target must be in the owner's logbook: ids from someone else's are simply not found.
    let exists: bool = sqlx::query_scalar(match a.target {
        Target::Workout => "SELECT EXISTS (SELECT FROM workout WHERE id = $1 AND owner_sub = $2)",
        Target::Exercise => "SELECT EXISTS (SELECT FROM exercise WHERE id = $1 AND owner_sub = $2)",
        Target::Set => {
            "SELECT EXISTS (SELECT FROM workout_set s JOIN workout w ON w.id = s.workout_id
                            WHERE s.id = $1 AND w.owner_sub = $2)"
        }
    })
    .bind(id)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(user_err(format!("no {} with id {id}", a.target.as_str())));
    }
    let column = match a.target {
        Target::Workout => "workout_id",
        Target::Exercise => "exercise_id",
        Target::Set => "set_id",
    };
    let thread_id: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "INSERT INTO comment_thread (owner_sub, target, {column}, started_by_sub) VALUES ($1, $2, $3, $4) RETURNING id"
    )))
    .bind(owner)
    .bind(a.target.as_str())
    .bind(id)
    .bind(&author.sub)
    .fetch_one(&mut *tx)
    .await?;
    add_comment(&mut tx, thread_id, author, text).await?;
    let v = thread(&mut tx, owner, author, thread_id).await?;
    tx.commit().await?;
    Ok(v)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommentReplyArgs {
    pub thread_id: i64,
    pub body: String,
}

/// Replying to a resolved thread reopens it.
pub async fn comment_reply(app: &App, owner: &str, author: &Author, a: CommentReplyArgs) -> AppResult<Value> {
    let text = body(&a.body)?;
    let mut tx = db::begin(app, owner).await?;
    let n = sqlx::query(
        "UPDATE comment_thread SET resolved_at = NULL, resolved_by_sub = NULL WHERE id = $1 AND owner_sub = $2",
    )
    .bind(a.thread_id)
    .bind(owner)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(user_err(format!("no comment thread with id {}", a.thread_id)));
    }
    add_comment(&mut tx, a.thread_id, author, text).await?;
    let v = thread(&mut tx, owner, author, a.thread_id).await?;
    tx.commit().await?;
    Ok(v)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommentListArgs {
    /// Only threads about this kind of thing. With `id`, about that one workout, exercise or set.
    pub target: Option<Target>,
    pub id: Option<i64>,
    /// Also resolved threads. Default false.
    #[serde(default)]
    pub include_resolved: bool,
    /// Most threads returned, newest first. Default 20, at most 100.
    pub limit: Option<i64>,
    /// The threads about this workout and about its sets, in one list. Not with `id`; with target, only that kind.
    pub workout_id: Option<i64>,
}

pub async fn comment_list(app: &App, owner: &str, author: &Author, a: CommentListArgs) -> AppResult<Value> {
    if a.id.is_some() && a.target.is_none() {
        return Err(user_err(
            "give the target (workout, exercise or set) that id belongs to",
        ));
    }
    if a.id.is_some() && a.workout_id.is_some() {
        return Err(user_err("give either id or workout_id, not both"));
    }
    let mut tx = db::begin_read(app, owner).await?;
    let filter = Filter {
        target: a.target,
        target_id: a.id,
        include_resolved: a.include_resolved,
        limit: a.limit.unwrap_or(20).clamp(1, 100),
        thread: None,
        workout: a.workout_id,
    };
    let v = threads(&mut tx, owner, author, filter).await?;
    tx.commit().await?;
    Ok(v)
}

/// The comment's author and its thread's starter, if `id` is a live comment in `owner`'s logbook.
async fn comment_row(tx: &mut Tx, owner: &str, id: i64) -> AppResult<(i64, String)> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT c.thread_id, c.author_sub FROM comment c JOIN comment_thread t ON t.id = c.thread_id
         WHERE c.id = $1 AND t.owner_sub = $2 AND c.deleted_at IS NULL FOR UPDATE OF c",
    )
    .bind(id)
    .bind(owner)
    .fetch_optional(&mut **tx)
    .await?;
    row.ok_or_else(|| user_err(format!("no comment with id {id}")))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommentUpdateArgs {
    pub comment_id: i64,
    /// The new text; replaces the old.
    pub body: String,
}

pub async fn comment_update(app: &App, owner: &str, author: &Author, a: CommentUpdateArgs) -> AppResult<Value> {
    let text = body(&a.body)?;
    let mut tx = db::begin(app, owner).await?;
    let (thread_id, by) = comment_row(&mut tx, owner, a.comment_id).await?;
    if by != author.sub {
        return Err(user_err("only its author can edit a comment"));
    }
    sqlx::query("UPDATE comment SET body = $2, edited_at = now() WHERE id = $1")
        .bind(a.comment_id)
        .bind(text)
        .execute(&mut *tx)
        .await?;
    let v = thread(&mut tx, owner, author, thread_id).await?;
    tx.commit().await?;
    Ok(v)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommentDeleteArgs {
    pub comment_id: i64,
}

/// The author, or the logbook's owner. The comment stays in its thread as deleted, so replies keep their context.
pub async fn comment_delete(app: &App, owner: &str, author: &Author, a: CommentDeleteArgs) -> AppResult<Value> {
    let mut tx = db::begin(app, owner).await?;
    let (thread_id, by) = comment_row(&mut tx, owner, a.comment_id).await?;
    if by != author.sub && author.sub != owner {
        return Err(user_err(
            "only its author or the owner of the logbook can delete a comment",
        ));
    }
    sqlx::query("UPDATE comment SET body = '', deleted_at = now() WHERE id = $1")
        .bind(a.comment_id)
        .execute(&mut *tx)
        .await?;
    let v = thread(&mut tx, owner, author, thread_id).await?;
    tx.commit().await?;
    Ok(v)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThreadResolveArgs {
    pub thread_id: i64,
    /// true resolves the thread, false reopens it.
    pub resolved: bool,
}

/// Whoever started the thread, or the logbook's owner.
pub async fn thread_resolve(app: &App, owner: &str, author: &Author, a: ThreadResolveArgs) -> AppResult<Value> {
    let mut tx = db::begin(app, owner).await?;
    let started_by: Option<String> =
        sqlx::query_scalar("SELECT started_by_sub FROM comment_thread WHERE id = $1 AND owner_sub = $2 FOR UPDATE")
            .bind(a.thread_id)
            .bind(owner)
            .fetch_optional(&mut *tx)
            .await?;
    let started_by = started_by.ok_or_else(|| user_err(format!("no comment thread with id {}", a.thread_id)))?;
    if started_by != author.sub && author.sub != owner {
        return Err(user_err(
            "only whoever started the thread or the owner of the logbook can resolve it",
        ));
    }
    sqlx::query(
        "UPDATE comment_thread SET resolved_at = CASE WHEN $2 THEN coalesce(resolved_at, now()) END,
           resolved_by_sub = CASE WHEN $2 THEN coalesce(resolved_by_sub, $3) END
         WHERE id = $1",
    )
    .bind(a.thread_id)
    .bind(a.resolved)
    .bind(&author.sub)
    .execute(&mut *tx)
    .await?;
    let v = thread(&mut tx, owner, author, a.thread_id).await?;
    tx.commit().await?;
    Ok(v)
}
